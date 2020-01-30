use std::future::Future;

pub trait RuntimeAPI {
    type Configuration;
    fn run_with<T, FnOnStart, FnOnShutdown, FnOnExit, R0, R1>(async_main: T,
                                                              config: Self::Configuration,
                                                              on_start: &mut FnOnStart,
                                                              on_shutdown: &mut FnOnShutdown,
                                                              on_exit: &mut FnOnExit)
        where T: Future<Output=()> + Send,
              FnOnStart: FnMut() -> R0,
              FnOnShutdown: FnMut(R0) -> R1,
              FnOnExit: FnMut(R1) -> ();

    fn shutdown_async();

    fn defer<T>(future: T) where T: Future<Output=()> + Send;
    fn spawn<T>(future: T) where T: Future<Output=()> + Send;

    type BatchGuard;
    unsafe fn batch_guard() -> Self::BatchGuard;
    unsafe fn push_batch(batch_guard: &Self::BatchGuard);
}
