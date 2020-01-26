use std::future::Future;
use std::ptr::{drop_in_place, NonNull, null_mut};
use std::sync::atomic::{AtomicU8, AtomicUsize};
use std::sync::atomic::Ordering::SeqCst;
use std::task::Poll;

use crate::mem::{do_dealloc, do_new};
use crate::worker::Worker;

/// Dynamically dispatched `Task` which wraps up a `Future`.
pub(crate) struct Task {
    /// C++ style dynamic dispatch.
    vtable: &'static TaskVTable,
    /// A next pointer for creating single linked list of tasks, so that all operations to the
    /// linked list are memory allocation free.
    next: *mut Task,
    /// One of `BLOCKED`, `SCHEDULED`, `RUNNING`, `DROPPED`.
    status: AtomicU8,
    /// Worker pointer.
    worker_ptr: *mut Worker,
    /// Reference count. When a waker is created from the task, the task's `rc` would be increased
    /// by 1, when the waker is dropped, the task's `rc` would be decreased by 1. The initial `rc`
    /// is set to 1, which represents it is referenced by the runtime. When the runtime finally
    /// polls the task to `Poll::Ready`, the task's `rc` would be decreased by 1 to release this
    /// reference. When the `rc` reaches 0, _ONLY_ deallocate the memory.
    rc: AtomicUsize,
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
            next: null_mut(),
            status: AtomicU8::new(Self::STATUS_BLOCKED),
            worker_ptr: null_mut(),
            rc: AtomicUsize::new(0),
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

    /// Dispatched to `TaskVTable.fn_poll`.
    #[inline]
    pub(crate) fn poll(task: NonNull<Task>, worker: &mut Worker) -> Poll<()> {
        unsafe { (task.as_ref().vtable.fn_poll)(task, worker) }
    }

    /// Dispatched to `TaskVTable.fn_drop_in_place`.
    #[inline]
    pub(crate) fn drop_in_place(task: NonNull<Task>) {
        unsafe { (task.as_ref().vtable.fn_drop_in_place)(task); }
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
    unimplemented!()
}

unsafe fn fn_drop_in_place<T>(task: NonNull<Task>) where T: Future<Output=()> + Send {
    drop_in_place(task.cast::<TaskInner<T>>().as_ptr());
}

unsafe fn fn_dealloc<T>(task: NonNull<Task>) where T: Future<Output=()> + Send {
    do_dealloc(task);
}
