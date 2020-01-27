use std::future::Future;
use std::ptr::{drop_in_place, NonNull, null_mut};

use parking_lot::{Condvar, Mutex};

use crate::epoch::Epoch;
use crate::mem::{do_alloc, do_dealloc, do_drop};
use crate::task::Task;
use crate::task_list::TaskList;
use crate::worker::Worker;

/// Manage workers construction and destruction.
pub(crate) struct WorkerBuilder {
    epoch: NonNull<Epoch>,
    workers_cond: Condvar,
    workers: Mutex<Box<[*mut Worker]>>,
    n_not_built_cond: Condvar,
    n_not_built: Mutex<u32>,
}

unsafe impl Send for WorkerBuilder {}

unsafe impl Sync for WorkerBuilder {}

impl WorkerBuilder {
    /// Create a new worker builder with `n_workers`.
    pub(crate) fn new(n_workers: u32) -> Self {
        if n_workers < 2 { panic!(); };
        let epoch = unsafe { do_alloc() }.unwrap();
        let workers_cond = Condvar::new();
        let workers = Mutex::new(vec![null_mut(); n_workers as usize].into_boxed_slice());
        let n_not_built_cond = Condvar::new();
        let n_not_built = Mutex::new(n_workers);
        Self { epoch, workers_cond, workers, n_not_built_cond, n_not_built }
    }

    /// Build one worker. Shall only be called once inside each worker threads.
    pub(crate) fn build<'a>(&self, tid: u32, n_workers: u32) -> &'a mut Worker {
        let epoch = self.epoch;

        let worker_ptr = unsafe { do_alloc().unwrap() };
        unsafe { Worker::set(worker_ptr.as_ptr()) };

        let mut guard = self.workers.lock();
        assert!(guard[tid as usize].is_null());
        guard[tid as usize] = worker_ptr.as_ptr();
        let flag = guard.iter_mut().all(|ptr| !ptr.is_null());
        drop(guard);
        if flag { self.workers_cond.notify_all(); };

        self.wait_other_workers();

        let other_workers = self.workers.lock().clone().iter()
            .enumerate().filter(|&(i, _)| i != tid as usize).map(|(_, x)| *x)
            .map(|x| unsafe { NonNull::new_unchecked(x) }).collect();
        unsafe { worker_ptr.as_ptr().write(Worker::new(n_workers, epoch, other_workers)) };

        let mut guard = self.n_not_built.lock();
        let flag = *guard;
        if flag < 1 { panic!(); };
        *guard -= 1;
        drop(guard);
        if flag == 1 { self.n_not_built_cond.notify_all(); };

        self.wait_all_built();

        Worker::get()
    }

    /// Wait for other workers done initialization. Should be called from a worker thread.
    fn wait_other_workers(&self) {
        let mut guard = self.workers.lock();
        while !guard.iter_mut().all(|ptr| !ptr.is_null()) { self.workers_cond.wait(&mut guard); };
        drop(guard);
    }

    /// Wait all worker threads ready to accept tasks.
    pub(crate) fn wait_all_built(&self) {
        let mut guard = self.n_not_built.lock();
        while *guard > 0 { self.n_not_built_cond.wait(&mut guard); };
        drop(guard);
    }

    pub(crate) fn push_async_main(&self, async_main: impl Future<Output=()> + Send) {
        let task = unsafe { Task::new(async_main) };
        let guard = self.workers.lock();
        let worker = *guard.get(0).unwrap();
        drop(guard);
        unsafe {
            (*worker).task_list.lock().push_back(task);
            (*self.epoch.as_ptr()).next_epoch();
        };
    }
}

impl Drop for WorkerBuilder {
    fn drop(&mut self) {
        let mut tasks = TaskList::new();

        self.workers.lock().iter()
            .for_each(|worker| unsafe {
                let task_list = (**worker).task_list.lock().pop_all();
                if !task_list.is_empty() { tasks.push_back_batch(task_list); };
                do_drop(NonNull::new_unchecked(*worker));
            });

        tasks.into_iter()
            .for_each(|task| {
                if Task::rc(task) > 1 {
                    eprint!("Task at {:p} still been referenced, memory leak might occur.");
                };
                Task::drop_in_place(task);
                Task::rc_dec(task);
            });

        unsafe { do_dealloc(self.epoch) };
    }
}