use std::cell::Cell;
use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;
use std::ptr::NonNull;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::Relaxed;
use std::task::{Context, Poll};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::epoch::Epoch;
use crate::rng::next_seed;
use crate::static_var::{get_local, set_local};
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
pub(crate) struct Shared {
    /// The local task queue.
    pub(crate) task_list: SyncTaskList,
}

/// Allow access shared data from worker.
impl Deref for Worker {
    type Target = Shared;

    fn deref(&self) -> &Self::Target { &self.shared }
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
    other_workers: Box<[NonNull<Worker>]>,
    /// Store locally generated defer tasks. When done polling a task, the worker thread should
    /// check its `defer_list`, if it is not empty, push them in batch to local queue.
    defer_list: Cell<TaskList>,
    /// Shutdown indicator.
    shutdown: AtomicBool,
}

impl Worker {
    /// Create a new worker.
    pub(crate) fn new(n_workers: u32, epoch: NonNull<Epoch>, other_workers: Box<[NonNull<Worker>]>)
                      -> Self {
        let task_list = SyncTaskList::new();
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u32;
        let defer_list = Cell::new(TaskList::new());
        let shutdown = AtomicBool::new(false);

        let shared = Shared { task_list };
        let private = Private { seed, n_workers, epoch, other_workers, defer_list, shutdown };

        Self { shared, private }
    }

    /// Set the `worker` as the current thread's worker.
    #[inline]
    pub(crate) unsafe fn set(worker: *mut Worker) { set_local(worker); }

    /// Get mutable reference to this thread's worker.
    #[inline]
    pub(crate) fn get<'a>() -> &'a mut Worker { unsafe { &mut *get_local() } }

    /// Push a task to the defer list of the worker.
    #[inline]
    pub(crate) fn defer(&mut self, task: NonNull<Task>) {
        self.private.defer_list.get_mut().push_front(task);
    }

    /// Access the epoch instant stored in private data since it is actually shared globally.
    #[inline]
    pub(crate) fn epoch<'a>(&mut self) -> &'a Epoch {
        unsafe { &*self.private.epoch.as_ptr() }
    }

    /// The main loop for the worker thread.
    pub(crate) fn main_loop(&mut self) {
        loop {
            'local: loop { // drain local queue
                let task = self.task_list.lock().pop_front();
                match task {
                    Some(task) => {
                        self.poll_task(task);
                        if self.private.shutdown.load(Relaxed) { return; };
                    }
                    None => {
                        break 'local;
                    }
                };
            };
            let mut old_instant = self.epoch().get_instant();
            let mut old_status = self.epoch().set_inactive();
            let task = 'task: loop {
                if Epoch::active_count(old_status) != 0 {
                    // try steal task
                    let task = self.try_steal();
                    match task {
                        Some(task) => break 'task task,
                        None => (),
                    };
                } else {
                    // try wait_next_epoch
                    let task = self.task_list.lock().pop_front();
                    match task {
                        Some(task) => break 'task task,
                        None => self.epoch().wait_next_epoch(old_status, old_instant),
                    };
                }
                // update status
                old_instant = self.epoch().get_instant();
                let new_status = self.epoch().get_status();
                if Epoch::epoch(new_status) > Epoch::epoch(old_status) {
                    let task = self.task_list.lock().pop_front();
                    match task {
                        Some(task) => break 'task task,
                        None => (),
                    };
                };
                old_status = new_status;
            };
            self.epoch().set_active();
            self.poll_task(task);
            if self.private.shutdown.load(Relaxed) { return; };
        }
    }
}

impl Worker {
    #[inline]
    fn poll_task(&mut self, task: NonNull<Task>) {
        let _ = Task::poll(task, self);
        if !self.private.defer_list.get_mut().is_empty() {
            let tasks = self.private.defer_list.get_mut().pop_all();
            self.shared.task_list.lock().push_front_batch(tasks);
        };
    }

    #[inline]
    fn try_steal(&mut self) -> Option<NonNull<Task>> {
        let seed = self.private.seed;
        let index = seed % (self.private.n_workers - 1);
        let victim = unsafe { self.private.other_workers.get_unchecked(index as usize).as_ref() };
        let task = if victim.task_list.is_empty() { None } else {
            victim.task_list.lock().pop_front()
        };
        if task.is_none() { self.private.seed = next_seed(seed); };
        task
    }
}


/// A future for shutdown the whole runtime.
pub struct Shutdown();

impl Future for Shutdown {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let worker = Worker::get();
        worker.private.shutdown.store(true, Relaxed);
        for other_worker in worker.private.other_workers.iter() {
            unsafe {
                if !(*other_worker).as_ref().private.shutdown.load(Relaxed) {
                    let task = Task::new(Shutdown());
                    (*other_worker).as_ref().task_list.lock().push_front(task);
                };
            };
        };
        worker.epoch().next_epoch();
        Poll::Ready(())
    }
}

unsafe impl Send for Shutdown {}
