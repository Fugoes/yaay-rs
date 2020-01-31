use std::process::exit;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::SeqCst;
use std::time::Duration;

use tokio::time::delay_for;

static COUNT: AtomicU32 = AtomicU32::new(0);
static N: u32 = 10000000;

fn main() {
    let mut rt = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .core_threads(4)
        .build()
        .unwrap();
    let _ = rt.block_on(async_main());
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..N { tokio::spawn(empty()); };
    delay_for(Duration::from_secs(10000));
    Ok(())
}

async fn empty() {
    if COUNT.fetch_add(1, SeqCst) == N - 1 { exit(0) };
}
