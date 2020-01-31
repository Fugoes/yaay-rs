use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::atomic::Ordering::{Acquire, Release, SeqCst};
use std::task::{Context, Poll};
use std::thread::spawn;

use yaay_mt_runtime::runtime::MTRuntime as runtime;
use yaay_runtime_api::RuntimeAPI;

fn main() {
    let mut fn_start = || {};
    let mut fn_shutdown = |_| {};
    let mut fn_exit = |_| {};
    runtime::run_with(async_main(), 8,
                      &mut fn_start, &mut fn_shutdown, &mut fn_exit);
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
                for _ in 0..100000000 { waker.wake_by_ref(); };
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
    runtime::shutdown_async();
    println!("async main exit");
}
