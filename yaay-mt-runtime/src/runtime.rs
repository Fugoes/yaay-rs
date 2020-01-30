use std::future::Future;
use std::thread;

use yaay_runtime_api::RuntimeAPI;

use crate::batch::BatchGuard;
use crate::task::Task;
use crate::worker::{Shutdown, Worker};
use crate::worker_manager::{RuntimeSharedData, WorkerManager};

pub struct MTRuntime {}

impl RuntimeAPI for MTRuntime {
    type Configuration = u32;

    fn run_with<T, FnOnStart, FnOnShutdown, FnOnExit, R0, R1>(async_main: T,
                                                              n_threads: Self::Configuration,
                                                              on_start: &mut FnOnStart,
                                                              on_shutdown: &mut FnOnShutdown,
                                                              on_exit: &mut FnOnExit)
        where T: Future<Output=()> + Send,
              FnOnStart: FnMut() -> R0,
              FnOnShutdown: FnMut(R0) -> R1,
              FnOnExit: FnMut(R1) -> () {
        let mut manager = WorkerManager::new(n_threads);
        let mut handles = vec![];
        for worker_id in 0..n_threads {
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
        let r0 = (*on_start)();
        guard.run(async_main);
        for handle in handles.into_iter() { let _ = handle.join(); };
        let r1 = (*on_shutdown)(r0);
        drop(guard);
        (*on_exit)(r1);
    }

    #[inline]
    fn shutdown_async() {
        let shared = RuntimeSharedData::get();
        shared.worker_ptrs.iter()
            .for_each(|x| unsafe {
                let task = Task::new(Shutdown());
                (*x.as_ptr()).task_list.lock().push_front(task);
            });
        unsafe { shared.epoch.as_ref().next_epoch() };
    }

    #[inline]
    fn defer<T>(future: T) where T: Future<Output=()> + Send {
        let task = unsafe { Task::new(future) };
        let worker = Worker::get();
        worker.defer(task);
    }

    #[inline]
    fn spawn<T>(future: T) where T: Future<Output=()> + Send {
        let task = unsafe { Task::new(future) };
        let worker = Worker::get();
        worker.spawn(task);
    }

    type BatchGuard = BatchGuard;
    unsafe fn batch_guard() -> Self::BatchGuard { BatchGuard::new() }
    unsafe fn push_batch(batch_guard: &BatchGuard) { batch_guard.push_batch(); }
}
