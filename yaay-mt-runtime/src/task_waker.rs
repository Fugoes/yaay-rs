use std::ptr::NonNull;
use std::task::Waker;

use crate::task::Task;

pub(crate) fn task_waker_from(task: NonNull<Task>) -> Waker { unimplemented!() }