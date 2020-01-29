use mio::IoVec;

use yaay_mio::net::{TcpListener, TcpStream};
use yaay_mio::prelude::*;
use yaay_mt_runtime::runtime::MTRuntime as runtime;
use yaay_runtime_api::RuntimeAPI;

fn main() {
    let mut fn_start = || { unsafe { mio_spawn_event_loop::<runtime>() } };
    let mut fn_shutdown = |r| { unsafe { mio_shutdown(r) } };
    runtime::run_with(async_main(), 4, &mut fn_start, &mut fn_shutdown);
}

async fn async_main() {
    let listener = TcpListener::bind(&"127.0.0.1:23333".parse().unwrap()).unwrap();
    let acceptor = listener.acceptor().await;
    loop {
        match acceptor.accept().await {
            Ok((client, addr)) => {
                println!("accept {}", addr);
                runtime::defer(handle_client(client));
            }
            Err(_err) => {
                runtime::shutdown_async();
                return;
            }
        }
    }
}

async fn handle_client(client: TcpStream) {
    let writer = client.writer().await;
    let data = vec![IoVec::from_bytes("hello world\n".as_bytes()).unwrap()];
    writer.write_bufs(data.as_slice()).await.unwrap();
}
