use std::future::Future;

pub trait RuntimeAPI {
    type Configuration;
    fn run_with<T>(async_main: T, config: Self::Configuration) where T: Future<Output=()> + Send;

    fn shutdown_async();

    fn defer<T>(future: T) where T: Future<Output=()> + Send;

    type BatchGuard;
    unsafe fn batch_guard() -> Self::BatchGuard;
}
