use std::process::exit;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::SeqCst;

use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

static COUNT: AtomicU32 = AtomicU32::new(0);

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
    for _ in 0..2000 {
        tokio::spawn(pong());
    };
    for _ in 0..2000 {
        let (socket, _) = listener.accept().await.unwrap();
        tokio::spawn(ping(socket));
    };
    let _ = listener.accept().await;
    Ok(())
}

async fn ping(mut socket: TcpStream) {
    let msg = "ping".as_bytes();
    let mut buf = [0; 4];
    loop {
        let _ = socket.write(msg).await.unwrap();
        let _ = socket.read_exact(&mut buf).await.unwrap();
        if COUNT.fetch_add(1, SeqCst) == 10000000 { exit(0) };
    };
}

async fn pong() {
    let mut stream = TcpStream::connect("127.0.0.1:11451").await.unwrap();
    let msg = "pong".as_bytes();
    let mut buf = [0; 4];
    loop {
        let _ = stream.read_exact(&mut buf).await.unwrap();
        let _ = stream.write(msg).await.unwrap();
    };
}
