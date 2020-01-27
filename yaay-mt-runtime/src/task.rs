use std::cell::Cell;
use std::future::Future;
use std::mem::forget;
use std::pin::Pin;
use std::ptr::{NonNull, null_mut};
use std::sync::atomic::{AtomicU8, AtomicUsize, spin_loop_hint};
use std::sync::atomic::Ordering::{Acquire, Relaxed, SeqCst};
use std::task::{Context, Poll};

use crate::mem::{do_dealloc, do_drop_in_place, do_new};
use crate::static_var::get_local;
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
    /// One of `BLOCKED`, `SCHEDULED`, `RUNNING`, `DROPPED`.
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
    const STATUS_BLOCKED: u8 = 0;
    const STATUS_SCHEDULED: u8 = 1;
    const STATUS_RUNNING: u8 = 2;
    const STATUS_DROPPED: u8 = 3;

    /// Alloc and initialize a new `NonNull<Task>` which contains the `future`.
    #[inline]
    pub(crate) unsafe fn new<T>(future: T) -> NonNull<Task> where T: Future<Output=()> + Send {
        let task = Task {
            vtable: vtable_of::<T>(),
            status: AtomicU8::new(Self::STATUS_BLOCKED),
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
    /// tasks won't be scheduled. All possible state transition in this function are:
    /// - `BLOCKED` to `SCHEDULED`: then schedule the task.
    /// - `RUNNING` to `SCHEDULED`: do nothing, since the `Task::poll_with()` method would recheck
    ///   the status after each poll until it is `BLOCKED`.
    /// - `SCHEDULED` to `SCHEDULED`: do nothing, since the task has already been scheduled.
    /// If the task is `DROPPED`, do nothing.
    #[inline]
    pub(crate) fn wake(task: NonNull<Task>) {
        let mut status = Self::get_status(task);
        loop {
            if status == Self::STATUS_DROPPED { return; };
            let (succ, prev) = Self::try_change_status(task, status, Self::STATUS_SCHEDULED);
            if succ {
                if prev == Self::STATUS_BLOCKED { // `BLOCKED` to `SCHEDULED`
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
                };
                // `RUNNING` to `SCHEDULED`: do nothing
                // `SCHEDULED` to `SCHEDULED`: do nothing
                return;
            } else {
                status = prev;
                spin_loop_hint();
            };
        };
    }

    /// Dispatched to `TaskVTable.fn_poll`. Shall only be called from one thread at the same time.
    /// All possible state transition in this function are:
    /// - `SCHEDULED` to `BLOCKED`: the task is polled to `Pending`.
    /// - `SCHEDULED` to `DROPPED`: the task is polled to `Ready`.
    /// If the task is `DROPPED`, do nothing. When ever returning from the inner poll function of
    /// the task, the task's status is `SCHEDULED`, this function would repoll it.
    #[inline]
    pub(crate) fn poll(task: NonNull<Task>) {
        let status = Self::get_status(task);
        if status == Self::STATUS_DROPPED { return; };
        Self::swap_status(task, Self::STATUS_RUNNING);

        let waker = unsafe { task_waker_from(task) };
        let mut cx = Context::from_waker(&waker);
        'outer: loop {
            let result = unsafe { (task.as_ref().vtable.fn_poll)(task, &mut cx) };
            if result.is_pending() {
                let mut status = Self::STATUS_RUNNING;
                loop {
                    let (succ, prev) =
                        if status == Self::STATUS_RUNNING {
                            Self::try_change_status(task, status, Self::STATUS_BLOCKED)
                        } else { // status == Self::STATUS_SCHEDULED
                            Self::try_change_status(task, status, Self::STATUS_RUNNING)
                        };
                    if succ {
                        if prev == Self::STATUS_RUNNING {
                            break 'outer; // return
                        } else { // prev == Self::STATUS_SCHEDULED
                            break; // repoll
                        };
                    } else {
                        status = prev;
                        spin_loop_hint();
                    };
                };
            } else {
                Self::swap_status(task, Self::STATUS_DROPPED);
                unsafe { (task.as_ref().vtable.fn_drop_in_place)(task) };
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
        let status = Self::swap_status(task, Self::STATUS_DROPPED);
        assert_ne!(status, Self::STATUS_RUNNING);
        if status != Self::STATUS_DROPPED {
            unsafe { (task.as_ref().vtable.fn_drop_in_place)(task) };
        };
    }
}

impl Task {
    #[inline]
    fn try_change_status(task: NonNull<Task>, current: u8, new: u8) -> (bool, u8) {
        match unsafe { task.as_ref().status.compare_exchange_weak(current, new, SeqCst, Relaxed) } {
            Ok(x) => (true, x),
            Err(x) => (false, x),
        }
    }

    #[inline]
    fn get_status(task: NonNull<Task>) -> u8 {
        unsafe { task.as_ref().status.load(Acquire) }
    }

    #[inline]
    fn swap_status(task: NonNull<Task>, to: u8) -> u8 {
        unsafe { task.as_ref().status.swap(to, SeqCst) }
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
struct RuntimeLocalData {
    task_lists: Cell<Box<[TaskList]>>,
}

impl RuntimeLocalData {
    /// Return a mutable reference to local data.
    #[inline]
    fn get<'a>() -> Option<&'a mut Self> {
        let local = unsafe { get_local() } as *mut RuntimeLocalData;
        if local.is_null() { None } else { Some(unsafe { &mut *local }) }
    }
}
