use std::process::exit;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::SeqCst;

use yaay_mio::net::*;
use yaay_mio::prelude::*;
use yaay_mt_runtime::runtime::MTRuntime as runtime;
use yaay_runtime_api::RuntimeAPI;

static COUNT: AtomicU32 = AtomicU32::new(0);
static N: u32 = 10000;

fn main() {
    let mut fn_start = || { unsafe { mio_spawn_event_loop::<runtime>(1) } };
    let mut fn_shutdown = |r| { unsafe { mio_shutdown(r) } };
    let mut fn_exit = |_| { unsafe { mio_exit(()) } };
    runtime::run_with(ping_pong_async_main(), 7, &mut fn_start, &mut fn_shutdown, &mut fn_exit);
}

async fn ping_pong_async_main() {
    let mut listener = TcpListenerHandle::bind(&"127.0.0.1:11451".parse().unwrap()).unwrap();
    let mut acceptor = listener.acceptor().await;
    for _ in 0..N {
        runtime::spawn(pong());
    };
    for _ in 0..N {
        let (read_handle, write_handle, _) = acceptor.accept().await.unwrap();
        runtime::spawn(ping(read_handle, write_handle));
    };
}

async fn ping(mut read_handle: TcpStreamReadHandle, mut write_handle: TcpStreamWriteHandle) {
    let mut reader = read_handle.reader().await;
    let mut writer = write_handle.writer().await;

    let msg = [0 as u8; 1];
    let mut buf = [0 as u8; 1];
    loop {
        let _ = writer.write_all(&msg).await.unwrap();
        let _ = reader.read_exact(&mut buf).await.unwrap();
        if COUNT.fetch_add(1, SeqCst) == 1000 * N { exit(0) };
    };
}

async fn pong() {
    let (mut read_handle, mut write_handle) =
        TcpStream::connect(&"127.0.0.1:11451".parse().unwrap()).unwrap();
    let mut reader = read_handle.reader().await;
    let mut writer = write_handle.writer().await;

    let msg = [0 as u8; 1];
    let mut buf = [0 as u8; 1];
    loop {
        let _ = reader.read_exact(&mut buf).await.unwrap();
        let _ = writer.write_all(&msg).await.unwrap();
    };
}
