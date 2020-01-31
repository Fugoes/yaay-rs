use std::thread::JoinHandle;

use yaay_runtime_api::RuntimeAPI;

use crate::shared::{GlobalData, set_shutdown};

#[allow(non_camel_case_types)]
pub unsafe fn mio_spawn_event_loop<runtime: RuntimeAPI>(n_threads: usize) -> Box<[JoinHandle<()>]> {
    GlobalData::build::<runtime>(n_threads)
}

pub unsafe fn mio_shutdown(handles: Box<[JoinHandle<()>]>) {
    set_shutdown();
    for handle in handles.into_vec().into_iter() {
        let _ = handle.join();
    };
}

pub unsafe fn mio_exit(_: ()) {}