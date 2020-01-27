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
    pub fn run_with<T>(future: T, n_workers: u32) where T: Future<Output=()> + Send {
        let builder = Arc::new(WorkerBuilder::new(n_workers));
        let mut handles = vec![];
        for tid in 0..n_workers {
            let builder = builder.clone();
            let handle = thread::Builder::new()
                .name(format!("yaa-mt-runtime-worker-{}", tid))
                .spawn(move || {
                    let worker = builder.build(tid, n_workers);
                    drop(builder);
                    worker.main_loop();
                })
                .unwrap();
            handles.push(handle);
        }
        builder.wait_all_built();
        builder.push_async_main(future);
        for handle in handles.into_iter() { let _ = handle.join(); };
    }

    #[inline]
    pub fn shutdown() -> Shutdown { Shutdown() }

    #[inline]
    pub fn defer<T>(future: T) where T: Future<Output=()> + Send {
        let task = unsafe { Task::new(future) };
        let worker = Worker::get();
        worker.defer(task);
    }
}