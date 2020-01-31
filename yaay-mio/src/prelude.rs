use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use parking_lot::{Condvar, Mutex};

use yaay_runtime_api::RuntimeAPI;

use crate::shared::SharedData;

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

#[allow(non_camel_case_types)]
pub unsafe fn mio_spawn_event_loop<runtime: RuntimeAPI>() -> JoinHandle<()> {
    let init = Arc::new(Mutex::new(false));
    let init_cond = Arc::new(Condvar::new());

    let handle = {
        let init = init.clone();
        let init_cond = init_cond.clone();
        thread::Builder::new()
            .name("yaay-mio-event-loop".to_string())
            .spawn(move || {
                let ptr = Box::into_raw(Box::new(SharedData::new()));
                SharedData::set_ptr(ptr);

                let mut guard = init.lock();
                *guard = true;
                drop(guard);
                init_cond.notify_all();
                drop(init);
                drop(init_cond);

                let batch_guard = runtime::batch_guard();
                let duration = Some(Duration::from_micros(500));
                let shared = SharedData::get();
                let mut poll_events = mio::Events::with_capacity(128);
                while !SHUTDOWN.load(Acquire) {
                    let n = shared.poll.poll(&mut poll_events, duration).unwrap();
                    for poll_event in poll_events.iter() {
                        let key = poll_event.token().0;
                        let event = poll_event.readiness();
                        let guard = shared.dispatchers.lock();
                        guard.get(key).map(|dispatcher| {
                            if event.is_readable() { dispatcher.dispatch_readable(); };
                            if event.is_writable() { dispatcher.dispatch_writable(); };
                        });
                    };
                    let mut dispatchers_guard = shared.dispatchers.lock();
                    let mut guard = shared.deferred_remove.lock();
                    for key in guard.iter() { dispatchers_guard.remove(*key); };
                    guard.clear();
                    drop(guard);
                    drop(dispatchers_guard);
                    if n > 0 { runtime::push_batch(&batch_guard) };
                };
            })
            .unwrap()
    };

    let mut guard = init.lock();
    while !*guard { init_cond.wait(&mut guard); };
    drop(guard);

    handle
}

pub unsafe fn mio_shutdown(handle: JoinHandle<()>) {
    SHUTDOWN.store(true, Release);
    let _ = handle.join();
}

pub unsafe fn mio_exit(_: ()) {
    let shared = SharedData::get();
    let mut dispatcher_guard = shared.dispatchers.lock();
    let mut guard = shared.deferred_remove.lock();
    for key in guard.iter() { dispatcher_guard.remove(*key); };
    guard.clear();
    drop(guard);
    drop(dispatcher_guard);

    Box::from_raw(SharedData::get_ptr());
}