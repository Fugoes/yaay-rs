use std::future::Future;

pub trait RuntimeAPI {
    type Configuration;
    fn run_with<T, FnOnStart, FnOnShutdown, R>(async_main: T, config: Self::Configuration,
                                               on_start: &mut FnOnStart,
                                               on_shutdown: &mut FnOnShutdown)
        where T: Future<Output=()> + Send,
              FnOnStart: FnMut() -> R,
              FnOnShutdown: FnMut(R) -> ();

    fn shutdown_async();

    fn defer<T>(future: T) where T: Future<Output=()> + Send;
    fn spawn<T>(future: T) where T: Future<Output=()> + Send;

    type BatchGuard;
    unsafe fn batch_guard() -> Self::BatchGuard;
    unsafe fn push_batch(batch_guard: &Self::BatchGuard);
}
