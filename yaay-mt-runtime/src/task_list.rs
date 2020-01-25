use std::ptr::{NonNull, null_mut};

use crate::task::Task;

/// A single linked list of `Task`. Since the next pointer is stored in `Task`, all methods are
/// memory allocation free.
pub(crate) struct TaskList {
    /// Pointer to the head of the list. When the list is empty, it is set to `null_mut()`.
    head: *mut Task,
    /// Pointer to the tail of the list. When the list is empty, it is set to `null_mut()`. When the
    /// list has only one `Task`, the tail equals to the head.
    tail: *mut Task,
}

impl TaskList {
    /// Returns an empty list.
    #[inline]
    pub(crate) fn new() -> Self {
        Self { head: null_mut(), tail: null_mut() }
    }

    /// Push one task to the back of the list.
    #[inline]
    pub(crate) fn push_back(&mut self, task: NonNull<Task>) {
        self.push_back_batch(TaskList { head: task.as_ptr(), tail: task.as_ptr() });
    }

    /// Push another _non-empty_ list of tasks to the back of the list.
    #[inline]
    pub(crate) fn push_back_batch(&mut self, tasks: TaskList) {
        let tail = self.tail;
        if tail.is_null() {
            self.head = tasks.head;
        } else {
            unsafe { Task::set_next(NonNull::new_unchecked(tail), tasks.head) };
        };
        self.tail = tasks.tail;
    }

    /// Push one task to the front of the list.
    #[inline]
    pub(crate) fn push_front(&mut self, task: NonNull<Task>) {
        self.push_front_batch(TaskList { head: task.as_ptr(), tail: task.as_ptr() });
    }

    /// Push another _non-empty_ list of tasks to the front of the list.
    #[inline]
    pub(crate) fn push_front_batch(&mut self, tasks: TaskList) {
        let head = self.head;
        if head.is_null() {
            self.tail = tasks.tail;
        } else {
            unsafe { Task::set_next(NonNull::new_unchecked(tasks.tail), head) };
        };
        self.head = tasks.head;
    }

    /// Try pop one task from the front of the list. If the list is empty, returns `None`, if not,
    /// returns the previous head of the list.
    #[inline]
    pub(crate) fn pop_front(&mut self) -> Option<NonNull<Task>> {
        let head = self.head;
        if head.is_null() {
            None
        } else {
            let tail = self.tail;
            if head == tail {
                self.head = null_mut();
                self.tail = null_mut();
            } else {
                let head_next = unsafe { Task::get_next(NonNull::new_unchecked(head)) };
                self.head = head_next;
            };
            Some(unsafe { NonNull::new_unchecked(head) })
        }
    }

    /// Whether the list is empty.
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.head.is_null()
    }

    /// Pop all tasks from the list.
    #[inline]
    pub(crate) fn pop_all(&mut self) -> TaskList {
        let head = self.head;
        let tail = self.tail;
        self.head = null_mut();
        self.tail = null_mut();
        TaskList { head, tail }
    }
}

pub(crate) struct SyncTaskList {}