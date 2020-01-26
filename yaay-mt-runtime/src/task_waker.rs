use std::ptr::NonNull;
use std::task::{RawWaker, RawWakerVTable, Waker};

use crate::task::Task;

pub(crate) unsafe fn task_waker_from(task: NonNull<Task>) -> Waker {
    Waker::from_raw(RawWaker::new(task.cast().as_ptr(), &TASK_RAW_WAKER_VTABLE))
}

static TASK_RAW_WAKER_VTABLE: RawWakerVTable =
    RawWakerVTable::new(fn_clone, fn_wake, fn_wake_by_ref, fn_drop);

unsafe fn fn_clone(data: *const ()) -> RawWaker {
    let task = NonNull::new_unchecked(data as *mut ()).cast();
    do_clone(task)
}

unsafe fn fn_wake(data: *const ()) {
    let task = NonNull::new_unchecked(data as *mut ()).cast();
    do_wake_by_ref(task);
    Task::rc_dec(task);
}

unsafe fn fn_wake_by_ref(data: *const ()) {
    let task = NonNull::new_unchecked(data as *mut ()).cast();
    do_wake_by_ref(task);
}

unsafe fn fn_drop(data: *const ()) {
    let task = NonNull::new_unchecked(data as *mut ()).cast();
    do_drop(task);
}

#[inline]
unsafe fn do_clone(task: NonNull<Task>) -> RawWaker {
    Task::rc_inc(task);
    RawWaker::new(task.cast().as_ptr(), &TASK_RAW_WAKER_VTABLE)
}

#[inline]
unsafe fn do_wake_by_ref(task: NonNull<Task>) {
    Task::wake(task);
}

#[inline]
unsafe fn do_drop(task: NonNull<Task>) {
    Task::rc_dec(task);
}
