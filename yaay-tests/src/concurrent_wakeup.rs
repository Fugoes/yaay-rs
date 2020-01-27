use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::atomic::Ordering::{Acquire, Release, SeqCst};
use std::task::{Context, Poll};
use std::thread::spawn;

use yaay_mt_runtime::runtime::MTRuntime;

fn main() {
    MTRuntime::run_with(async_main(), 4);
}

static BOOL: AtomicBool = AtomicBool::new(true);
static N: AtomicU32 = AtomicU32::new(4);

struct Dummy();

impl Future for Dummy {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        for _ in 0..4 {
            let waker = cx.waker().clone();
            spawn(move || {
                for _ in 0..10000000 {
                    waker.wake_by_ref();
                };
                N.fetch_sub(1, SeqCst);
            });
        };
        Poll::Ready(())
    }
}

struct Test();

impl Future for Test {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        assert!(BOOL.load(Acquire));
        BOOL.store(false, Release);
        assert!(!BOOL.load(Acquire));
        BOOL.store(true, Release);
        if N.load(Acquire) == 0 {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

async fn async_main() {
    Dummy().await;
    Test().await;
    MTRuntime::shutdown().await;
}
