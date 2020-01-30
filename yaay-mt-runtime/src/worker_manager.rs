use std::cell::Cell;
use std::future::Future;
use std::ptr::{NonNull, null_mut};
use std::sync::Arc;

use parking_lot::{Condvar, Mutex};

use crate::epoch::Epoch;
use crate::mem::{do_alloc, do_drop, do_new};
use crate::static_var::{get_global, set_global};
use crate::task::Task;
use crate::task_list::TaskList;
use crate::worker::Worker;

struct WorkerManagerData {
    n_workers: u32,
    epoch: NonNull<Epoch>,
    worker_ptrs: Mutex<Box<[*mut Worker]>>,
    worker_ptrs_cond: Condvar,
    n_built: Mutex<u32>,
    n_built_cond: Condvar,
}

unsafe impl Sync for WorkerManagerData {}

/// Manage initialization of all workers. Used in the main thread.
pub(crate) struct WorkerManager {
    next_worker_id: Cell<u32>,
    data: Arc<WorkerManagerData>,
}

/// Manage each worker's initialization. Used in all worker threads.
pub(crate) struct WorkerBuilder {
    worker_id: u32,
    data: Arc<WorkerManagerData>,
}

unsafe impl Send for WorkerBuilder {}

/// Manage destruction of all workers. Used in the main thread.
pub(crate) struct WorkerGuard();

#[repr(align(64))]
pub(crate) struct RuntimeSharedData {
    pub(crate) epoch: NonNull<Epoch>,
    pub(crate) worker_ptrs: Box<[NonNull<Worker>]>,
}

impl RuntimeSharedData {
    #[inline]
    pub(crate) fn get<'a>() -> &'a RuntimeSharedData {
        unsafe { &*get_global() }
    }
}

impl WorkerManager {
    /// Create a worker manager that manages `n` workers.
    pub(crate) fn new(n: u32) -> Self {
        let next_worker_id = Cell::new(0);
        let data = Arc::new(WorkerManagerData {
            n_workers: n,
            epoch: unsafe { do_new(Epoch::new(n)) }.unwrap(),
            worker_ptrs: Mutex::new(vec![null_mut(); n as usize].into_boxed_slice()),
            worker_ptrs_cond: Condvar::new(),
            n_built: Mutex::new(0),
            n_built_cond: Condvar::new(),
        });
        Self { next_worker_id, data }
    }

    /// Get one builder.
    pub(crate) fn builder(&mut self) -> WorkerBuilder {
        let worker_id = self.next_worker_id.get();
        *self.next_worker_id.get_mut() += 1;
        let data = self.data.clone();
        WorkerBuilder { worker_id, data }
    }

    /// Wait all builder done built. Return a `WorkerGuard` to handle destruction for workers.
    pub(crate) fn wait(self) -> WorkerGuard {
        let mut guard = self.data.n_built.lock();
        while *guard != self.data.n_workers { self.data.n_built_cond.wait(&mut guard); };
        drop(guard);

        let epoch = self.data.epoch;
        let worker_ptrs = self.data.worker_ptrs.lock().clone().iter()
            .map(|x| NonNull::new(*x).unwrap())
            .collect();
        unsafe { set_global(do_new(RuntimeSharedData { epoch, worker_ptrs }).unwrap().as_ptr()) };

        WorkerGuard()
    }
}

impl WorkerBuilder {
    /// Build current thread's worker. After return, the `Worker::get()` method would be safe to
    /// use.
    pub(crate) fn build(self) {
        let worker_ptr = unsafe { do_alloc() }.unwrap();
        unsafe { Worker::set(worker_ptr.as_ptr()) };

        let mut guard = self.data.worker_ptrs.lock();
        guard[self.worker_id as usize] = worker_ptr.as_ptr();
        let flag = guard.iter_mut().all(|x| !x.is_null());
        drop(guard);

        if flag {
            self.data.worker_ptrs_cond.notify_all();
        } else {
            let mut guard = self.data.worker_ptrs.lock();
            while !guard.iter_mut().all(|x| !x.is_null()) {
                self.data.worker_ptrs_cond.wait(&mut guard);
            };
        };

        let worker_id = self.worker_id;
        let other_workers = self.data.worker_ptrs.lock().clone().iter()
            .enumerate().filter(|&(i, _)| i != worker_id as usize)
            .map(|(_, x)| NonNull::new(*x).unwrap())
            .collect();
        let worker = Worker::new(
            self.worker_id, self.data.n_workers, self.data.epoch, other_workers);
        unsafe { worker_ptr.as_ptr().write(worker) };

        let mut guard = self.data.n_built.lock();
        *guard += 1;
        let flag = *guard == self.data.n_workers;
        drop(guard);

        if flag {
            self.data.n_built_cond.notify_all();
        } else {
            let mut guard = self.data.n_built.lock();
            while *guard != self.data.n_workers { self.data.n_built_cond.wait(&mut guard); };
        };
    }
}

impl WorkerGuard {
    /// Push the async main function as a task to local queues. It would return immediately. And the
    /// called should wait for all worker threads exit.
    pub(crate) fn run<T>(&self, async_main: T) where T: Future<Output=()> + Send {
        let task = unsafe { Task::new(async_main) };
        let shared: &RuntimeSharedData = unsafe { &*(get_global() as *mut RuntimeSharedData) };
        unsafe { shared.worker_ptrs[0].as_ref().task_list.lock().push_back(task) };
        unsafe { shared.epoch.as_ref().next_epoch() };
    }
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        let mut tasks = TaskList::new();
        let shared = RuntimeSharedData::get();
        shared.worker_ptrs.iter()
            .for_each(|worker| unsafe {
                let mut guard = worker.as_ref().task_list.lock();
                if !guard.is_empty() { tasks.push_back_batch(guard.pop_all()); };
                drop(guard);
                do_drop(*worker);
            });
        tasks.into_iter().for_each(|task| {
            assert!(Task::rc(task) > 0);
            Task::drop_in_place(task);
            if Task::rc(task) != 1 {
                eprintln!("Task<{:p}> has references. Memory leak might occur!", task.as_ptr());
            };
            Task::rc_dec(task);
        });
        unsafe { do_drop(shared.epoch) };
        unsafe { do_drop(NonNull::new_unchecked(get_global::<RuntimeSharedData>())) };
        unsafe { set_global(null_mut() as *mut ()) };
    }
}
