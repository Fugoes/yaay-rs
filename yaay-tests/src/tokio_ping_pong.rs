use std::process::exit;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::SeqCst;

use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

static COUNT: AtomicU32 = AtomicU32::new(0);
static N: u32 = 10000;

fn main() {
    let mut rt = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .core_threads(8)
        .enable_all()
        .build()
        .unwrap();
    let _ = rt.block_on(async_main());
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    let mut listener = TcpListener::bind("127.0.0.1:11451").await.unwrap();
    for _ in 0..N {
        tokio::spawn(pong());
    };
    for _ in 0..N {
        let (socket, _) = listener.accept().await.unwrap();
        tokio::spawn(ping(socket));
    };
    let _ = listener.accept().await;
    Ok(())
}

async fn ping(mut socket: TcpStream) {
    let msg = [0 as u8; 1];
    let mut buf = [0 as u8; 1];
    loop {
        let _ = socket.write_all(&msg).await.unwrap();
        let _ = socket.read_exact(&mut buf).await.unwrap();
        if COUNT.fetch_add(1, SeqCst) == 1000 * N { exit(0) };
    };
}

async fn pong() {
    let mut stream = TcpStream::connect("127.0.0.1:11451").await.unwrap();
    let msg = [0 as u8; 1];
    let mut buf = [0 as u8; 1];
    loop {
        let _ = stream.read_exact(&mut buf).await.unwrap();
        let _ = stream.write_all(&msg).await.unwrap();
    };
}
