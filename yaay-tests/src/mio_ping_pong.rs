use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::SeqCst;

use mio::IoVec;

use yaay_mio::net::{TcpListener, TcpStream};
use yaay_mio::prelude::*;
use yaay_mt_runtime::runtime::MTRuntime as runtime;
use yaay_runtime_api::RuntimeAPI;
use std::process::exit;

static COUNT: AtomicU32 = AtomicU32::new(0);

fn main() {
    let mut fn_start = || { unsafe { mio_spawn_event_loop::<runtime>() } };
    let mut fn_shutdown = |r| { unsafe { mio_shutdown(r) } };
    let mut fn_exit = |_| { unsafe { mio_exit(()) } };
    runtime::run_with(ping_pong_async_main(), 8, &mut fn_start, &mut fn_shutdown, &mut fn_exit);
}

async fn ping_pong_async_main() {
    let listener = TcpListener::bind(&"127.0.0.1:11451".parse().unwrap()).unwrap();
    let acceptor = listener.acceptor().await;

    for _ in 0..2000 {
        runtime::spawn(pong());
    };
    for _ in 0..2000 {
        let (stream, _) = acceptor.accept().await.unwrap();
        runtime::spawn(ping(stream));
    };
}

async fn ping(stream: TcpStream) {
    let reader = stream.reader().await;
    let writer = stream.writer().await;

    let msg: &IoVec = "ping".as_bytes().into();
    let mut buf = [0 as u8; 4];
    let buf: &mut IoVec = (&mut buf[..]).into();
    loop {
        let _ = writer.write_exact(msg.into()).await.unwrap();
        let _ = reader.read_exact(buf).await.unwrap();
        if COUNT.fetch_add(1, SeqCst) == 10000000 { exit(0) };
    };
}

async fn pong() {
    let connector = TcpStream::connect(&"127.0.0.1:11451".parse().unwrap()).unwrap();
    let reader = connector.reader().await;
    let writer = connector.writer().await;

    let msg: &IoVec = "pong".as_bytes().into();
    let mut buf = [0 as u8; 4];
    let buf: &mut IoVec = (&mut buf[..]).into();
    loop {
        let _ = reader.read_exact(buf).await.unwrap();
        let _ = writer.write_exact(msg.into()).await.unwrap();
    };
}
