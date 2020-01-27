use std::future::Future;
use std::ops::Add;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};
use std::thread::{sleep, spawn};
use std::time::{Duration, SystemTime};

use yaay_mt_runtime::runtime::MTRuntime;

struct Sleep(SystemTime);

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

struct Test();

impl Future for Test {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let waker = cx.waker().clone();
        let _h = spawn(move || { loop { waker.wake_by_ref(); } });
        Poll::Ready(())
    }
}

struct Locker(Mutex<u32>);

impl Future for Locker {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut guard = self.0.try_lock().unwrap();
        *guard += 1;
        let flag = *guard;
        drop(guard);
        if flag == 100000000 { Poll::Ready(()) } else { Poll::Pending }
    }
}

fn main() {
    MTRuntime::run_with(async_main(), 4);
}

async fn shutdown() {
    Sleep::new(1).await;
    MTRuntime::shutdown().await;
}

async fn async_main() {
    MTRuntime::defer(shutdown());
    Test().await;
    Locker(Mutex::new(0)).await;
}