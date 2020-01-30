use bencher::{Bencher, black_box};

use yaay_mt_runtime::runtime::MTRuntime as runtime;
use yaay_runtime_api::RuntimeAPI;

async fn work() {
    let val = 1 + 1;
    black_box(val);
}

fn spawn_bench(bench: &mut Bencher) {
    let mut fn_start = || {};
    let mut fn_shutdown = |_| {};
    runtime::run_with(spawn_bench_async_main(bench), 8, &mut fn_start, &mut fn_shutdown);
}

async fn spawn_bench_async_main(bench: &mut Bencher) {
    bench.iter(|| {
        runtime::spawn(work());
    });
    runtime::shutdown_async();
}

fn defer_bench(bench: &mut Bencher) {
    let mut fn_start = || {};
    let mut fn_shutdown = |_| {};
    runtime::run_with(defer_bench_async_main(bench), 8, &mut fn_start, &mut fn_shutdown);
}

async fn defer_bench_async_main(bench: &mut Bencher) {
    bench.iter(|| {
        runtime::defer(work());
    });
    runtime::shutdown_async();
}

bencher::benchmark_group!(spawn_defer, spawn_bench, defer_bench);
bencher::benchmark_main!(spawn_defer);
