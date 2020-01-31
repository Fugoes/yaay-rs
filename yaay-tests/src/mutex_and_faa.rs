use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::SeqCst;
use std::time::Instant;

use parking_lot::Mutex;

fn main() {
    let mutex = Mutex::new(());
    let counter = AtomicU32::new(1);
    let begin = Instant::now();
    loop {
        let mut guard = mutex.lock();
        drop(guard);
        if counter.fetch_add(1, SeqCst) == 0 { break; };
    };
    println!("{:?}", begin.elapsed() / u32::max_value());
}