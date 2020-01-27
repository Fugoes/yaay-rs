use std::future::Future;
use std::ops::Add;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::thread::{sleep, spawn};
use std::time::{Duration, SystemTime};

use yaay_mt_runtime::runtime::MTRuntime;

struct Sleep(SystemTime);

#[allow(dead_code)]
impl Sleep {
    fn new(n: u32) -> Self { Self(SystemTime::now().add(Duration::new(n as u64, 0))) }
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let now = SystemTime::now();
        if now >= self.0 {
            Poll::Ready(())
        } else {
            let duration = self.0.duration_since(now).unwrap();
            let waker = cx.waker().clone();
            spawn(move || {
                sleep(duration);
                waker.wake();
            });
            Poll::Pending
        }
    }
}

fn main() {
    MTRuntime::run_with(async_main(), 4);
}

async fn defer_empty() {
    for _ in 0..10000 {
        MTRuntime::defer(empty());
    }
}

async fn empty() {
    // empty
}

async fn async_main() {
    for _ in 0..10000 {
        MTRuntime::defer(defer_empty());
    };
    sleep(Duration::new(1, 0));
    MTRuntime::shutdown().await;
}
