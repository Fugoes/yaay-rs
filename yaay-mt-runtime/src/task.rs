use std::cell::Cell;
use std::future::Future;
use std::mem::forget;
use std::pin::Pin;
use std::ptr::{NonNull, null_mut};
use std::sync::atomic::{AtomicU8, AtomicUsize};
use std::sync::atomic::Ordering::{Relaxed, SeqCst};
use std::task::{Context, Poll};

use crate::mem::{do_dealloc, do_drop_in_place, do_new};
use crate::shared::get_local;
use crate::task_list::TaskList;
use crate::task_waker::task_waker_from;
use crate::worker_manager::RuntimeSharedData;

/// Dynamically dispatched `Task` which wraps up a `Future`. It meets the following requirements:
/// - One task shouldn't be in multiple workers' local queues.
/// - One task shouldn't be polled concurrently by multiple worker threads.
/// - After a task's `wake()` method is invoked, eventually the task would be polled again.
/// - When a task is dropped (after polled to ready, or after the runtime is shutdown), the task's
///   `wake()` method should still be safe to invoke.
pub(crate) struct Task {
    /// C++ style dynamic dispatch.
    vtable: &'static TaskVTable,
    /// Status of the task. Include 3 bits, namely `MUTED`, `NOTIFIED`, and `DROPPED`.
    status: AtomicU8,
    /// Reference count. When a waker is created from the task, the task's `rc` would be increased
    /// by 1, when the waker is dropped, the task's `rc` would be decreased by 1. The initial `rc`
    /// is set to 1, which represents it is referenced by the runtime. When the runtime finally
    /// polls the task to `Poll::Ready`, the task's `rc` would be decreased by 1 to release this
    /// reference. When the `rc` reaches 0, _ONLY_ deallocate the memory.
    rc: AtomicUsize,
    /// A next pointer for creating single linked list of tasks, so that all operations to the
    /// linked list are memory allocation free.
    next: *mut Task,
    /// Provide a hint for which worker to schedule the task to. When a worker steal a task from
    /// another worker, it would set the task's `worker_id` to itself.
    worker_id: u32,
}

impl Task {
    const MUTED: u8 = 1;
    const NOTIFIED: u8 = Self::MUTED << 1;
    const DROPPED: u8 = Self::NOTIFIED << 1;

    /// Alloc and initialize a new `NonNull<Task>` which contains the `future`. It is set to
    /// `| MUTED | NOTIFIED | !DROPPED |` state.
    #[inline]
    pub(crate) unsafe fn new<T>(future: T) -> NonNull<Task> where T: Future<Output=()> + Send {
        let task = Task {
            vtable: vtable_of::<T>(),
            status: AtomicU8::new(Self::MUTED | Self::NOTIFIED),
            rc: AtomicUsize::new(1),
            next: null_mut(),
            worker_id: 0,
        };
        let task_inner = TaskInner { task, future };
        do_new(task_inner).unwrap().cast()
    }

    /// Get method of the `next` pointer.
    #[inline]
    pub(crate) fn get_next(task: NonNull<Task>) -> *mut Task {
        unsafe { task.as_ref().next }
    }

    /// Set method of the `next` pointer.
    #[inline]
    pub(crate) fn set_next(mut task: NonNull<Task>, next: *mut Task) {
        unsafe { task.as_mut().next = next; };
    }

    /// Get method of the `worker_id`.
    #[inline]
    pub(crate) fn get_worker_id(task: NonNull<Task>) -> u32 {
        unsafe { task.as_ref().worker_id }
    }

    /// Set method of the `worker_id`.
    #[inline]
    pub(crate) fn set_worker_id(task: NonNull<Task>, worker_id: u32) {
        unsafe { (&mut *task.as_ptr()).worker_id = worker_id };
    }

    /// Get current reference count.
    #[inline]
    pub(crate) fn rc(task: NonNull<Task>) -> usize {
        unsafe { task.as_ref().rc.load(Relaxed) }
    }

    /// Increase the reference count by 1.
    #[inline]
    pub(crate) fn rc_inc(task: NonNull<Task>) {
        unsafe { task.as_ref().rc.fetch_add(1, SeqCst) };
    }

    /// Decrease the reference count by 1. Dealloc the memory when the reference count reaches 0.
    /// Since the runtime only release its reference to a task when the task is dropped, so only
    /// dealloc is needed here.
    #[inline]
    pub(crate) fn rc_dec(task: NonNull<Task>) {
        unsafe {
            if task.as_ref().rc.fetch_sub(1, SeqCst) == 1 {
                (task.as_ref().vtable.fn_dealloc)(task);
            }
        }
    }

    /// Atomically wake up the task. Guarantee the task would be polled again eventually, though
    /// multiple invocations of `wake()` might share one `poll()` invocation. Guarantee dropped
    /// tasks won't be scheduled again.
    #[inline]
    pub(crate) fn wake(task: NonNull<Task>) {
        // Unconditionally set the `MUTED` bit and the `NOTIFIED` bit.
        let prev = unsafe { task.as_ref().status.fetch_or(Self::MUTED | Self::NOTIFIED, SeqCst) };
        // If previous status is `| MUTED | _ | _ |` or `| _ | _ | DROPPED |, do nothing.
        if prev & (Self::MUTED | Self::DROPPED) != 0 { return; };
        // Previous status could not be `| !MUTED | NOTIFIED | !DROPPED |`.
        // assert!(prev == 0);
        // Now previous should be `| !MUTED | !NOTIFIED | !DROPPED |`, we need to schedule it to a
        // local queue.
        let worker_id = Task::get_worker_id(task) as usize;
        match RuntimeLocalData::get() {
            Some(local) => {
                let task_lists = local.task_lists.get_mut();
                let task_list = unsafe { task_lists.get_unchecked_mut(worker_id) };
                task_list.push_back(task);
            }
            None => {
                let shared = RuntimeSharedData::get();
                let worker = unsafe { shared.worker_ptrs.get_unchecked(worker_id) };
                unsafe { worker.as_ref().task_list.lock().push_back(task) };
                unsafe { shared.epoch.as_ref().next_epoch() };
            }
        };
    }

    /// Dispatched to `TaskVTable.fn_poll`. Shall only be called from one thread at the same time.
    #[inline]
    pub(crate) fn poll(task: NonNull<Task>) {
        // After the task is pushed to the queue, its status is `| MUTED | NOTIFIED | !DROPPED |`.
        let status = unsafe { &task.as_ref().status };
        let waker = unsafe { task_waker_from(task) };
        let mut cx = Context::from_waker(&waker);
        // Unset the `NOTIFIED` bit, in case during the polling, new wake up events happen.
        unsafe { task.as_ref().status.fetch_and(!Self::NOTIFIED, SeqCst) };
        'outer: loop {
            // assert!(prev & Self::MUTED != 0);
            // Now its status is `| MUTED | !NOTIFIED | !DROPPED |`.
            let result = unsafe { (task.as_ref().vtable.fn_poll)(task, &mut cx) };
            if result.is_pending() {
                // We expect the status is still `| MUTED | !NOTIFIED | !DROPPED |`. Set status to
                // `| !MUTED | !NOTIFIED | !DROPPED |` if the task hasn't been waked during the
                // previous polling.
                let mut prev = Self::MUTED;
                'cas: loop {
                    if prev == Self::MUTED {
                        match status.compare_exchange_weak(prev, 0 as u8, SeqCst, Relaxed) {
                            Ok(_) => break 'outer, // return
                            Err(val) => prev = val,
                        };
                    } else {
                        // assert_eq!(prev, Self::NOTIFIED | Self::MUTED);
                        // the `NOTIFY` bit is set
                        match status.compare_exchange_weak(prev, Self::MUTED, SeqCst, Relaxed) {
                            Ok(_) => break 'cas, // repoll
                            Err(val) => prev = val,
                        };
                    };
                };
            } else {
                // Drop the task.
                unsafe { task.as_ref().status.fetch_or(Self::DROPPED, SeqCst) };
                unsafe { (task.as_ref().vtable.fn_drop_in_place)(task) };
                // The task might still have references from wakers. The memory deallocation would
                // be delayed to these wakers' `drop()`.
                Self::rc_dec(task);
                break 'outer;
            };
        };
        forget(waker);
    }

    /// Dispatched to `TaskVTable.fn_drop_in_place`. Safe to be called multiple times and from
    /// multiple threads concurrently.
    #[inline]
    pub(crate) fn drop_in_place(task: NonNull<Task>) {
        let prev = unsafe { task.as_ref().status.fetch_or(Self::DROPPED, SeqCst) };
        if prev & Self::DROPPED == 0 {
            unsafe { (task.as_ref().vtable.fn_drop_in_place)(task) };
        };
    }
}

struct TaskVTable {
    /// The poll function.
    fn_poll: unsafe fn(NonNull<Task>, &mut Context) -> Poll<()>,
    /// The drop function. It won't deallocate the memory.
    fn_drop_in_place: unsafe fn(NonNull<Task>),
    /// The deallocate function.
    fn_dealloc: unsafe fn(NonNull<Task>),
}

/// Wrapping up any `Future` type `T` inside a pointer of `Task`. With the `C` layout, pointer of
/// `TaskInner<T>` could be cast to pointer of `Task`.
#[repr(C)]
struct TaskInner<T> where T: Future<Output=()> + Send {
    task: Task,
    future: T,
}

/// A trick to get a static reference to the vtable of type `T`.
fn vtable_of<T>() -> &'static TaskVTable where T: Future<Output=()> + Send {
    &TaskVTable {
        fn_poll: fn_poll::<T>,
        fn_drop_in_place: fn_drop_in_place::<T>,
        fn_dealloc: fn_dealloc::<T>,
    }
}

unsafe fn fn_poll<T>(task: NonNull<Task>, cx: &mut Context) -> Poll<()>
    where T: Future<Output=()> + Send {
    let inner = task.cast::<TaskInner<T>>();
    Pin::new_unchecked(&mut (*inner.as_ptr()).future).poll(cx)
}

unsafe fn fn_drop_in_place<T>(task: NonNull<Task>) where T: Future<Output=()> + Send {
    let inner = task.cast::<TaskInner<T>>();
    do_drop_in_place(inner);
}

unsafe fn fn_dealloc<T>(task: NonNull<Task>) where T: Future<Output=()> + Send {
    let inner = task.cast::<TaskInner<T>>();
    do_dealloc(inner);
}

/// To support batching push tasks, as well as reduce `next_epoch()` calls, store a
/// `RuntimeLocalData` pointer in the reactor's thread local storage. When `Task::wake()` is called,
/// it would check if this pointer is null to decide whether using batch push mode.
pub(crate) struct RuntimeLocalData {
    pub(crate) task_lists: Cell<Box<[TaskList]>>,
}

impl RuntimeLocalData {
    /// Return an empty instance.
    pub(crate) fn new() -> Self {
        let n = RuntimeSharedData::get().worker_ptrs.len();
        let mut task_lists = vec![];
        for _ in 0..n { task_lists.push(TaskList::new()); };
        RuntimeLocalData { task_lists: Cell::new(task_lists.into_boxed_slice()) }
    }

    /// Return a mutable reference to local data.
    #[inline]
    pub(crate) fn get<'a>() -> Option<&'a mut Self> {
        let local = unsafe { get_local() } as *mut RuntimeLocalData;
        if local.is_null() { None } else { Some(unsafe { &mut *local }) }
    }

    /// Return a mutable reference to local data without checking.
    #[inline]
    pub(crate) unsafe fn get_unchecked<'a>() -> &'a mut Self {
        let local = get_local() as *mut RuntimeLocalData;
        &mut *local
    }
}
