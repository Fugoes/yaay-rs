use std::future::Future;
use std::sync::Arc;
use std::thread;

use crate::epoch::Epoch;
use crate::mem::{do_dealloc, do_new};
use crate::task::Task;
use crate::worker::{Shutdown, Worker};
use crate::worker_builder::WorkerBuilder;

pub struct MTRuntime {}

impl MTRuntime {
    pub fn run_with(n_workers: u32) {
        let builder = Arc::new(WorkerBuilder::new(n_workers));
        let epoch_ptr = unsafe { do_new(Epoch::new(n_workers)) }.unwrap();
        let mut handles = vec![];
        for tid in 0..n_workers {
            let builder = builder.clone();
            let epoch_ptr = epoch_ptr.as_ptr() as usize;
            let handle = thread::Builder::new()
                .name(format!("yaa-mt-runtime-worker-{}", tid))
                .spawn(move || {
                    let worker = builder.build(tid, n_workers, epoch_ptr as *mut Epoch);
                    drop(builder);
                    worker.main_loop();
                })
                .unwrap();
            handles.push(handle);
        }
        builder.wait_all_built();
        for handle in handles.into_iter() { let _ = handle.join(); };
        drop(builder);
        unsafe { do_dealloc(epoch_ptr) };
    }

    #[inline]
    pub fn shutdown() -> Shutdown {
        Shutdown()
    }

    #[inline]
    pub fn defer<T>(future: T) where T: Future<Output=()> + Send {
        let task = unsafe { Task::new(future) };
        let worker = Worker::get();
        worker.defer(task);
    }
}