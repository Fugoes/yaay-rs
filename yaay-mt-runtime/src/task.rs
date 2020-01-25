use std::alloc::{alloc, dealloc, Layout};
use std::future::Future;
use std::ptr::{drop_in_place, NonNull, null_mut};
use std::sync::atomic::{AtomicPtr, AtomicUsize};
use std::sync::atomic::Ordering::SeqCst;
use std::task::Poll;

use crate::worker::Worker;

/// Dynamically dispatched `Task` which wraps up a `Future`.
pub(crate) struct Task {
    /// C++ style dynamic dispatch.
    vtable: &'static TaskVTable,
    /// A next pointer for creating single linked list of tasks, so that all operations to the
    /// linked list are memory allocation free.
    next: *mut Task,
    /// A worker to schedule the task to when the task is woken. When a waker found the pointer is
    /// `null_mut()`, the task is already scheduled to exactly one worker, so the task won't need
    /// to be scheduled. This safety property is guaranteed by using atomic swap operation.
    worker: AtomicPtr<Worker>,
    /// Reference count. When a waker is created from the task, the task's `rc` would be increased
    /// by 1, when the waker is dropped, the task's `rc` would be decreased by 1. The initial `rc`
    /// is set to 1, which represents it is referenced by the runtime. When the runtime finally
    /// polls the task to `Poll::Ready`, the task's `rc` would be decreased by 1 to release this
    /// reference. When the `rc` reaches 0, _ONLY_ deallocate the memory.
    rc: AtomicUsize,
}

impl Task {
    /// Alloc and initialize a new `NonNull<Task>` which contains the `future`.
    #[inline]
    pub(crate) unsafe fn new<T>(future: T) -> NonNull<Task> where T: Future<Output=()> + Send {
        let task = Task {
            vtable: vtable_of::<T>(),
            next: null_mut(),
            worker: AtomicPtr::new(null_mut()),
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

    /// Atomic swap the `task.worker` pointer with `null_mut()`, return the original value of
    /// `task.worker` which could be `null_mut()`.
    #[inline]
    pub(crate) fn get_worker(task: NonNull<Task>) -> *mut Worker {
        unsafe { task.as_ref().worker.swap(null_mut(), SeqCst) }
    }

    /// Atomic swap the `task.worker` pointer with the `worker`, the original value of `task.worker`
    /// must be `null_mut()`.
    #[inline]
    pub(crate) fn set_worker(task: NonNull<Task>, worker: NonNull<Worker>) {
        let ptr = unsafe { task.as_ref().worker.swap(worker.as_ptr(), SeqCst) };
        assert!(ptr.is_null());
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

#[inline]
unsafe fn do_new<T>(data: T) -> Option<NonNull<T>> {
    let layout = Layout::new::<T>();
    let ptr = alloc(layout) as *mut T;
    if ptr.is_null() {
        return None;
    } else {
        ptr.write(data);
        Some(NonNull::new_unchecked(ptr))
    }
}

#[inline]
unsafe fn do_dealloc<T>(ptr: NonNull<T>) {
    let layout = Layout::new::<T>();
    dealloc(ptr.as_ptr() as *mut u8, layout);
}