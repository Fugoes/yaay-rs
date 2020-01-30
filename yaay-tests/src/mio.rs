use mio::IoVec;

use yaay_mio::net::{TcpListener, TcpStream};
use yaay_mio::prelude::*;
use yaay_mt_runtime::runtime::MTRuntime as runtime;
use yaay_runtime_api::RuntimeAPI;

fn main() {
    let mut fn_start = || { unsafe { mio_spawn_event_loop::<runtime>() } };
    let mut fn_shutdown = |r| { unsafe { mio_shutdown(r) } };
    let mut fn_exit = |_| { unsafe { mio_exit(()) } };
    runtime::run_with(ping_pong_async_main(), 4, &mut fn_start, &mut fn_shutdown, &mut fn_exit);
}

async fn ping_pong_async_main() {
    runtime::spawn(ping());
    runtime::spawn(pong());
}

async fn ping() {
    let listener = TcpListener::bind(&"127.0.0.1:23333".parse().unwrap()).unwrap();
    let acceptor = listener.acceptor().await;
    let (stream, _addr) = acceptor.accept().await.unwrap();
    let reader = stream.reader().await;
    let writer = stream.writer().await;

    let msg: &IoVec = "ping".as_bytes().into();
    let mut buf = [0 as u8; 4];
    let buf: &mut IoVec = (&mut buf[..]).into();
    for _i in 0..10 {
        let _ = writer.write_exact(msg.into()).await.unwrap();
        let _ = reader.read_exact(buf).await.unwrap();
        println!("{}", std::str::from_utf8(&buf).unwrap());
    };
    runtime::shutdown_async();
}

async fn pong() {
    let connector = TcpStream::connect(&"127.0.0.1:23333".parse().unwrap()).unwrap();
    let reader = connector.reader().await;
    let writer = connector.writer().await;

    let msg: &IoVec = "pong".as_bytes().into();
    let mut buf = [0 as u8; 4];
    let buf: &mut IoVec = (&mut buf[..]).into();
    loop {
        let _ = reader.read_exact(buf).await.unwrap();
        println!("{}", std::str::from_utf8(&buf).unwrap());
        let _ = writer.write_exact(msg.into()).await.unwrap();
    };
}
