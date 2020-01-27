use std::ops::{Deref, DerefMut};
use std::path::Iter;
use std::ptr::{NonNull, null_mut};
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering::Acquire;

use parking_lot::lock_api::RawMutex as _;
use parking_lot::RawMutex;

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

/// An iterator iterate from head to tail.
impl Iterator for TaskList {
    type Item = NonNull<Task>;

    fn next(&mut self) -> Option<Self::Item> { self.pop_front() }
}

/// A thread safe task list. It is protected by a mutex. Since the critical section is small
/// (usually involves only two memory stores), and no memory allocation is needed, a fast
/// `parking_lot::RawMutex` is enough and there is no need for lock free queues which usually
/// evolve complicate memory management and provide less features (e.g. they usually provides either
/// FIFO push or LIFO push, while here both FIFO push and LIFO push are needed).
pub(crate) struct SyncTaskList {
    /// Use `RawMutex` to optimize the `is_empty` method.
    raw_mutex: RawMutex,
    /// The mutex protected task list.
    inner: TaskList,
}

unsafe impl Sync for SyncTaskList {}

impl SyncTaskList {
    /// Return an empty `SyncTaskList`.
    pub fn new() -> Self { Self { raw_mutex: RawMutex::INIT, inner: TaskList::new() } }

    /// Lock the task list.
    #[inline]
    pub fn lock(&self) -> SyncTaskListGuard {
        self.raw_mutex.lock();
        SyncTaskListGuard(&self)
    }

    /// A hint which might be out-date for whether the task list is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        let head_ref: &*mut Task = &(self.inner.head);
        let head_ptr = head_ref as *const *mut Task;
        let head_atomic_ptr: *const AtomicPtr<Task> = head_ptr as *const AtomicPtr<Task>;
        unsafe { (*head_atomic_ptr).load(Acquire).is_null() }
    }
}

/// Guard for locked `SyncTaskList`.
pub(crate) struct SyncTaskListGuard<'a>(&'a SyncTaskList);

impl<'a> Deref for SyncTaskListGuard<'a> {
    type Target = TaskList;

    fn deref(&self) -> &Self::Target { &self.0.inner }
}

impl<'a> DerefMut for SyncTaskListGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(&self.0.inner as *const TaskList as *mut TaskList) }
    }
}

impl<'a> Drop for SyncTaskListGuard<'a> {
    fn drop(&mut self) {
        self.0.raw_mutex.unlock();
    }
}
