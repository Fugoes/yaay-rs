use std::ptr::null_mut;

use parking_lot::{Condvar, Mutex};

use crate::epoch::Epoch;
use crate::mem::do_alloc;
use crate::worker::Worker;

/// Manage workers construction and destruction.
pub(crate) struct WorkerBuilder {
    workers_cond: Condvar,
    workers: Mutex<Box<[*mut Worker]>>,
    n_not_built_cond: Condvar,
    n_not_built: Mutex<u32>,
}

unsafe impl Sync for WorkerBuilder {}

impl WorkerBuilder {
    pub(crate) fn new(n_workers: u32) -> Self {
        let workers_cond = Condvar::new();
        let workers = Mutex::new(vec![null_mut(); n_workers as usize].into_boxed_slice());
        let n_not_built_cond = Condvar::new();
        let n_not_built = Mutex::new(n_workers);
        Self { workers_cond, workers, n_not_built_cond, n_not_built }
    }

    /// Build one worker. Shall only be called once inside each worker threads.
    pub(crate) fn build(&self, tid: u32, n_workers: u32, epoch: *mut Epoch) {
        let worker_ptr = unsafe { do_alloc().unwrap() };

        let mut guard = self.workers.lock();
        assert!(!guard[tid as usize].is_null());
        guard[tid as usize] = worker_ptr.as_ptr();
        let flag = guard.iter_mut().all(|ptr| !ptr.is_null());
        drop(guard);
        if flag { self.workers_cond.notify_all(); };

        self.wait_workers();

        let workers = self.workers.lock().clone();
        let mut other_workers = vec![null_mut(); n_workers as usize - 1].into_boxed_slice();
        let mut i = 0;
        for j in 0..n_workers {
            if j != tid {
                other_workers[i as usize] = workers[j as usize];
                i += 1;
            };
        };
        unsafe { worker_ptr.as_ptr().write(Worker::new(n_workers, epoch, other_workers)) };

        let mut guard = self.n_not_built.lock();
        let flag = *guard;
        if flag < 1 { panic!(); };
        *guard -= 1;
        drop(guard);
        if flag == 1 { self.n_not_built_cond.notify_all(); };

        self.wait_all_built();
    }

    fn wait_workers(&self) {
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
}