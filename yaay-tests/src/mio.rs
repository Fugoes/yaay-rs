use mio::IoVec;

use yaay_mio::net::{TcpListener, TcpStream};
use yaay_mio::prelude::*;
use yaay_mt_runtime::runtime::MTRuntime as runtime;
use yaay_runtime_api::RuntimeAPI;

fn main() {
    let mut fn_start = || { unsafe { mio_spawn_event_loop::<runtime>() } };
    let mut fn_shutdown = |r| { unsafe { mio_shutdown(r) } };
    runtime::run_with(ping_pong_async_main(), 4, &mut fn_start, &mut fn_shutdown);
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
    let msg = vec![IoVec::from_bytes("ping".as_bytes()).unwrap()];
    let mut vec_buf = vec![0; 4].into_boxed_slice();
    let mut buf = vec![IoVec::from_bytes_mut(vec_buf.as_mut()).unwrap()];
    let _ = writer.write_bufs(msg.as_slice()).await;
    let _ = reader.read_bufs(&mut buf).await;
    let _ = reader.read_bufs(&mut buf).await;
    runtime::shutdown_async();
}

async fn pong() {
    let connector = TcpStream::connect(&"127.0.0.1:23333".parse().unwrap()).unwrap();
    let reader = connector.reader().await;
    let writer = connector.writer().await;
    let msg = vec![IoVec::from_bytes("pong".as_bytes()).unwrap()];
    let mut vec_buf = vec![0; 4].into_boxed_slice();
    let mut buf = vec![IoVec::from_bytes_mut(vec_buf.as_mut()).unwrap()];
    loop {
        let _ = reader.read_bufs(buf.as_mut_slice()).await;
        let _ = writer.write_bufs(msg.as_slice()).await;
    };
}
