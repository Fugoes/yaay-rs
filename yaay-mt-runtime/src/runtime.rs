use std::future::Future;
use std::thread;

use crate::task::Task;
use crate::worker::{Shutdown, Worker};
use crate::worker_manager::{RuntimeSharedData, WorkerManager};

pub struct MTRuntime {}

impl MTRuntime {
    pub fn run_with<T>(async_main: T, n_workers: u32) where T: Future<Output=()> + Send {
        let mut manager = WorkerManager::new(n_workers);
        let mut handles = vec![];
        for worker_id in 0..n_workers {
            let builder = manager.builder();
            let handle = thread::Builder::new()
                .name(format!("yaay-mt-runtime-worker-{}", worker_id))
                .spawn(move || {
                    builder.build();
                    Worker::get().main_loop();
                })
                .unwrap();
            handles.push(handle);
        };
        let guard = manager.wait();
        guard.run(async_main);
        for handle in handles.into_iter() { let _ = handle.join(); };
        drop(guard);
    }

    #[inline]
    pub fn shutdown_async() {
        let shared = RuntimeSharedData::get();
        shared.worker_ptrs.iter()
            .for_each(|x| unsafe {
                let task = Task::new(Shutdown());
                (*x.as_ptr()).task_list.lock().push_front(task);
            });
        unsafe { shared.epoch.as_ref().next_epoch() };
    }

    #[inline]
    pub fn defer<T>(future: T) where T: Future<Output=()> + Send {
        let task = unsafe { Task::new(future) };
        let worker = Worker::get();
        worker.defer(task);
    }
}