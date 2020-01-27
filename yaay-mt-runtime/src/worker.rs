use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::ptr::{NonNull, null_mut};
use std::sync::atomic::{AtomicBool, AtomicPtr};
use std::sync::atomic::Ordering::Relaxed;
use std::task::{Context, Poll};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::epoch::Epoch;
use crate::task::Task;
use crate::task_list::{SyncTaskList, TaskList};

/// Runtime worker. Each runtime threads' worker is stored in a thread local variable.
pub(crate) struct Worker {
    /// Shared data that all threads might access.
    shared: Shared,
    /// Private data that only the owner worker thread would access.
    private: Private,
}

#[repr(align(64))]
struct Shared {
    /// The local task queue.
    task_list: SyncTaskList,
}

#[repr(align(64))]
struct Private {
    /// Seed for rng.
    seed: u32,
    /// Number of workers in the runtime.
    n_workers: u32,
    /// The global shared epoch, it is managed by `WorkerBuilder`.
    epoch: NonNull<Epoch>,
    /// Pointers to other workers.
    other_workers: Box<[*mut Worker]>,
    /// Store locally generated defer tasks. When done polling a task, the worker thread should
    /// check its `defer_list`, if it is not empty, push them in batch to local queue.
    defer_list: Cell<TaskList>,
    /// Shutdown indicator.
    shutdown: AtomicBool,
}

impl Worker {
    pub(crate) fn new(n_workers: u32, epoch: NonNull<Epoch>, other_workers: Box<[*mut Worker]>)
                      -> Self {
        let task_list = SyncTaskList::new();
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u32;
        let defer_list = Cell::new(TaskList::new());
        let shutdown = AtomicBool::new(false);

        let shared = Shared { task_list };
        let private = Private { seed, n_workers, epoch, other_workers, defer_list, shutdown };

        Self { shared, private }
    }

    pub(crate) fn set(worker: *mut Worker) {
        WORKER.with(|x| x.store(worker as *mut (), Relaxed));
    }

    /// Get mutable reference to this thread's worker.
    #[inline]
    pub(crate) fn get<'a>() -> &'a mut Worker {
        unsafe { &mut *(WORKER.with(|x| x.load(Relaxed)) as *mut Worker) }
    }

    pub(crate) fn main_loop(&mut self) {
        loop {
            'local: loop { // drain local queue
                let mut guard = self.shared.task_list.lock();
                let task = guard.pop_front();
                drop(guard);
                match task {
                    Some(task) => {
                        self.poll_task(task);
                        if self.private.shutdown.load(Relaxed) { return; };
                    }
                    None => {
                        break 'local;
                    }
                }
            }
            let mut old_instant = self.get_epoch().get_instant();
            let mut old_status = self.get_epoch().set_inactive();
            let task = 'task: loop {
                if Epoch::active_count(old_status) != 0 {
                    // try steal
                    let victim_index = self.private.seed % (self.private.n_workers - 1);
                    let victim = unsafe {
                        &mut **self.private.other_workers.get_unchecked(victim_index as usize)
                    };
                    if !(victim.shared.task_list.is_empty()) {
                        let mut guard = victim.shared.task_list.lock();
                        let task = guard.pop_front();
                        drop(guard);
                        match task {
                            Some(task) => break 'task task,
                            None => self.next_seed(),
                        };
                    } else {
                        self.next_seed();
                    };
                } else {
                    // try wait_next_epoch
                    let mut guard = self.shared.task_list.lock();
                    let task = guard.pop_front();
                    drop(guard);
                    match task {
                        Some(task) => break 'task task,
                        None => self.get_epoch().wait_next_epoch(old_status, old_instant),
                    };
                }
                // update status
                old_instant = self.get_epoch().get_instant();
                let new_status = self.get_epoch().get_status();
                if Epoch::epoch(new_status) > Epoch::epoch(old_status) {
                    let mut guard = self.shared.task_list.lock();
                    let task = guard.pop_front();
                    drop(guard);
                    match task {
                        Some(task) => break 'task task,
                        None => (),
                    };
                };
                old_status = new_status;
            };
            self.get_epoch().set_active();
            self.poll_task(task);
            if self.private.shutdown.load(Relaxed) { return; };
        }
    }

    #[inline]
    pub(crate) fn get_local_queue(&mut self) -> &SyncTaskList {
        &self.shared.task_list
    }

    #[inline]
    fn poll_task(&mut self, task: NonNull<Task>) {
        let _ = Task::poll(task, self);
        if !self.private.defer_list.get_mut().is_empty() {
            let tasks = self.private.defer_list.get_mut().pop_all();
            self.shared.task_list.lock().push_front_batch(tasks);
        };
    }

    #[inline]
    pub(crate) fn get_epoch<'a>(&self) -> &'a Epoch {
        unsafe { &*self.private.epoch.as_ptr() }
    }

    #[inline]
    fn next_seed(&mut self) {
        let seed = self.private.seed;
        let seed = seed ^ (seed << 13);
        let seed = seed ^ (seed << 17);
        let seed = seed ^ (seed << 5);
        self.private.seed = seed;
    }

    #[inline]
    pub(crate) fn defer(&mut self, task: NonNull<Task>) {
        self.private.defer_list.get_mut().push_front(task);
    }
}

unsafe impl Send for Worker {}

pub struct Shutdown();

impl Future for Shutdown {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let worker = Worker::get();
        worker.private.shutdown.store(true, Relaxed);
        for other_worker in worker.private.other_workers.iter() {
            unsafe {
                if !(**other_worker).private.shutdown.load(Relaxed) {
                    let task = Task::new(Shutdown());
                    let mut guard = (**other_worker).shared.task_list.lock();
                    guard.push_front(task);
                    drop(guard);
                    worker.get_epoch().next_epoch();
                };
            };
        };
        Poll::Ready(())
    }
}

unsafe impl Send for Shutdown {}

thread_local! {
pub(crate) static WORKER: AtomicPtr<()> = AtomicPtr::new(null_mut());
}
