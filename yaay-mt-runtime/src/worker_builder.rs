use parking_lot::{Condvar, Mutex};

use crate::worker::Worker;

/// Manage workers construction and destruction.
pub(crate) struct WorkerBuilder {
    cond: Condvar,
    workers: Mutex<Box<[*mut Worker]>>,
}

unsafe impl Sync for WorkerBuilder {}

impl WorkerBuilder {
    /// Build one worker. Shall only be called once inside each worker threads.
    pub(crate) fn build(&self, tid: u32) {
        let mut guard = self.workers.lock();
        drop(guard);
    }

    /// Wait all worker threads ready to accept tasks.
    pub(crate) fn wait_all() {}
}