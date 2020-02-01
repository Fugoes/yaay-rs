use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::process::exit;
use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::SeqCst;
use std::task::{Context, Poll};

use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

static COUNT: AtomicU32 = AtomicU32::new(0);
static N: u32 = 1000;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let addr: Arc<SocketAddr> = Arc::new(args[1].as_str().clone().parse().unwrap());
    let n = args[2].parse::<u32>().unwrap();
    let mut rt = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .core_threads(n as usize)
        .enable_all()
        .build()
        .unwrap();
    let _ = rt.block_on(async_main(addr.clone()));
}

async fn async_main(addr: Arc<SocketAddr>) -> Result<(), Box<dyn std::error::Error>> {
    let mut listener = TcpListener::bind(addr.as_ref()).await.unwrap();
    for _ in 0..N {
        tokio::spawn(pong(addr.clone()));
    };
    for _i in 0..N {
        let (socket, _) = listener.accept().await.unwrap();
        tokio::spawn(ping(socket));
    };
    println!("all accept");
    Block().await;
    Ok(())
}

async fn ping(mut socket: TcpStream) {
    let msg = [0 as u8; 1];
    let mut buf = [0 as u8; 1];
    for _ in 0..10000 {
        let _ = socket.write_all(&msg).await.unwrap();
        let _ = socket.read_exact(&mut buf).await.unwrap();
    };
    if COUNT.fetch_add(1, SeqCst) == N - 1 { exit(0); };
}

async fn pong(addr: Arc<SocketAddr>) {
    let mut stream = TcpStream::connect(addr.as_ref()).await.unwrap();
    let msg = [0 as u8; 1];
    let mut buf = [0 as u8; 1];
    for _ in 0..10000 {
        let _ = stream.read_exact(&mut buf).await.unwrap();
        let _ = stream.write_all(&msg).await.unwrap();
    };
    Block().await;
    panic!();
}

struct Block();

impl Future for Block {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}

