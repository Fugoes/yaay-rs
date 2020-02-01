use std::net::SocketAddr;
use std::process::exit;

use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::SeqCst;
use std::task::Context;

use tokio::macros::support::{Future, Pin, Poll};

use yaay_mio::net::*;
use yaay_mio::prelude::*;
use yaay_mt_runtime::runtime::MTRuntime as runtime;
use yaay_runtime_api::RuntimeAPI;

static COUNT: AtomicU32 = AtomicU32::new(0);
static N: u32 = 1000;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let addr: Arc<SocketAddr> = Arc::new(args[1].as_str().clone().parse().unwrap());
    let n = args[2].parse::<u32>().unwrap();
    let mio_n = args[3].parse::<u32>().unwrap();
    let mut fn_start = || { unsafe { mio_spawn_event_loop::<runtime>(mio_n as usize) } };
    let mut fn_shutdown = |r| { unsafe { mio_shutdown(r) } };
    let mut fn_exit = |_| { unsafe { mio_exit(()) } };
    runtime::run_with(ping_pong_async_main(addr.clone()), n, &mut fn_start, &mut fn_shutdown, &mut
        fn_exit);
}

async fn ping_pong_async_main(addr: Arc<SocketAddr>) {
    let mut listener = TcpListenerHandle::bind(addr.as_ref()).unwrap();
    let mut acceptor = listener.acceptor().await;
    for _i in 0..N {
        runtime::spawn(pong(addr.clone()));
    };
    for _i in 0..N {
        let (read_handle, write_handle, _) = acceptor.accept().await.unwrap();
        runtime::spawn(ping(read_handle, write_handle));
    };
    println!("all accept");
    Block().await;
}

async fn ping(mut read_handle: TcpStreamReadHandle, mut write_handle: TcpStreamWriteHandle) {
    let mut reader = read_handle.reader().await;
    let mut writer = write_handle.writer().await;

    let msg = [0 as u8; 1];
    let mut buf = [0 as u8; 1];
    for _ in 0..10000 {
        let _ = writer.write_all(&msg).await.unwrap();
        let _ = reader.read_exact(&mut buf).await.unwrap();
    };
    if COUNT.fetch_add(1, SeqCst) == N - 1 { exit(0); };
}

struct Block();

impl Future for Block {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}

async fn pong(addr: Arc<SocketAddr>) {
    let (mut read_handle, mut write_handle) = TcpStream::connect(addr.as_ref()).unwrap();
    let mut reader = read_handle.reader().await;
    let mut writer = write_handle.writer().await;

    let msg = [0 as u8; 1];
    let mut buf = [0 as u8; 1];
    for _ in 0..10000 {
        let _ = reader.read_exact(&mut buf).await.unwrap();
        let _ = writer.write_all(&msg).await.unwrap();
    };
    Block().await;
    panic!()
}
