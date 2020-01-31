use std::process::exit;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::SeqCst;

use yaay_mt_runtime::runtime::MTRuntime as runtime;
use yaay_runtime_api::RuntimeAPI;

static COUNT: AtomicU32 = AtomicU32::new(0);
static N: u32 = 10000000;

fn main() {
    let mut fn_start = || {};
    let mut fn_shutdown = |_| {};
    let mut fn_exit = |_| {};
    runtime::run_with(async_main(), 4, &mut fn_start, &mut fn_shutdown, &mut fn_exit);
}

async fn async_main() {
    for _ in 0..N { runtime::spawn(empty()); };
}

async fn empty() {
    if COUNT.fetch_add(1, SeqCst) == N - 1 { exit(0) };
}
