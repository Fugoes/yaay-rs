use std::future::Future;
use std::mem::forget;
use std::pin::Pin;
use std::ptr::{drop_in_place, NonNull, null_mut};
use std::sync::atomic::{AtomicU8, AtomicUsize, spin_loop_hint};
use std::sync::atomic::Ordering::{Acquire, Relaxed, SeqCst};
use std::task::{Context, Poll};

use crate::mem::{do_dealloc, do_new};
use crate::task_waker::task_waker_from;
use crate::worker::Worker;

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
    /// Worker pointer.
    worker_ptr: *mut Worker,
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
            worker_ptr: null_mut(),
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

    /// Increase the reference count by 1.
    #[inline]
    pub(crate) fn rc_inc(task: NonNull<Task>) {
        unsafe { task.as_ref().rc.fetch_add(1, SeqCst) };
    }

    /// Decrease the reference count by 1. Dealloc the memory when the reference count reaches 0.
    #[inline]
    pub(crate) fn rc_dec(task: NonNull<Task>) {
        unsafe {
            if task.as_ref().rc.fetch_sub(1, SeqCst) == 1 {
                (task.as_ref().vtable.fn_dealloc)(task);
            }
        }
    }

    /// Get current reference count.
    #[inline]
    pub(crate) fn rc(task: NonNull<Task>) -> usize {
        unsafe { task.as_ref().rc.load(Relaxed) }
    }

    /// Dispatched to `TaskVTable.fn_drop_in_place`.
    #[inline]
    pub(crate) fn drop_in_place(task: NonNull<Task>) {
        unsafe { (task.as_ref().vtable.fn_drop_in_place)(task) };
    }

    /// Atomically wake up the task. Guarantee the task would be polled again eventually, though
    /// multiple invocations of `wake()` might share one `poll()` invocation. Guarantee dropped
    /// tasks won't be scheduled.
    #[inline]
    pub(crate) fn wake(task: NonNull<Task>) {
        let mut status = Self::load_status(task);
        loop {
            if status == Self::STATUS_DROPPED { return; };
            let (succ, prev) = Self::cas_status(task, status, Self::STATUS_SCHEDULED);
            if succ {
                if prev == Self::STATUS_BLOCKED {
                    unsafe {
                        let worker = task.as_ref().worker_ptr;
                        let mut guard = (*worker).task_list.lock();
                        guard.push_back(task);
                        drop(guard);
                        // TODO (*worker).get_epoch().next_epoch();
                    };
                };
                return;
            } else {
                status = prev;
                spin_loop_hint();
            }
        }
    }

    /// Dispatched to `TaskVTable.fn_poll`.
    #[inline]
    pub(crate) fn poll(task: NonNull<Task>, worker: &mut Worker) -> Poll<()> {
        let result = unsafe { (task.as_ref().vtable.fn_poll)(task, worker) };
        if result.is_ready() {
            unsafe { task.as_ref().status.swap(Self::STATUS_DROPPED, SeqCst) };
            Self::drop_in_place(task);
            Self::rc_dec(task);
        };
        result
    }

    #[inline]
    fn cas_status(task: NonNull<Task>, current: u8, new: u8) -> (bool, u8) {
        match unsafe { task.as_ref().status.compare_exchange_weak(current, new, SeqCst, Relaxed) } {
            Ok(x) => (true, x),
            Err(x) => (false, x),
        }
    }

    #[inline]
    fn load_status(task: NonNull<Task>) -> u8 {
        unsafe { task.as_ref().status.load(Acquire) }
    }
}


struct TaskVTable {
    /// The poll function. A waker is created from the `&mut Worker` to poll the inner `Future`.
    fn_poll: unsafe fn(NonNull<Task>, &mut Worker) -> Poll<()>,
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

unsafe fn fn_poll<T>(task: NonNull<Task>, worker: &mut Worker) -> Poll<()>
    where T: Future<Output=()> + Send {
    let waker = task_waker_from(task);
    let mut cx = Context::from_waker(&waker);
    let inner = task.cast::<TaskInner<T>>();
    (*inner.as_ptr()).task.worker_ptr = worker as *mut Worker;
    inner.as_ref().task.status.swap(Task::STATUS_RUNNING, SeqCst);
    let result = 'outer: loop {
        let result = Future::poll(Pin::new_unchecked(&mut (*inner.as_ptr()).future), &mut cx);
        if !(result.is_ready()) {
            let mut status = Task::load_status(task);
            loop {
                let (succ, prev) =
                    if status == Task::STATUS_RUNNING {
                        Task::cas_status(task, Task::STATUS_RUNNING, Task::STATUS_BLOCKED)
                    } else { // status == Task::STATUS_SCHEDULED
                        Task::cas_status(task, Task::STATUS_SCHEDULED, Task::STATUS_RUNNING)
                    };
                if succ {
                    if prev == Task::STATUS_RUNNING {
                        break 'outer result;
                    } else { // prev == Task::STATUS_SCHEDULED
                        break;
                    };
                } else {
                    status = prev;
                    spin_loop_hint();
                }
            };
        } else {
            break 'outer result;
        };
    };
    forget(waker);
    result
}

unsafe fn fn_drop_in_place<T>(task: NonNull<Task>) where T: Future<Output=()> + Send {
    drop_in_place(task.cast::<TaskInner<T>>().as_ptr());
}

unsafe fn fn_dealloc<T>(task: NonNull<Task>) where T: Future<Output=()> + Send {
    do_dealloc(task.cast::<TaskInner<T>>());
}
