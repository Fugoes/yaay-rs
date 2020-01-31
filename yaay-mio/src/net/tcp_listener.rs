use std::io::Result;
use std::net::SocketAddr;

use crate::container::{MIO, MIOHandle};
use crate::net::tcp_stream::*;

pub struct TcpListenerHandle(pub(super) MIOHandle<mio::net::TcpListener>);

pub struct TcpListenerAcceptor<'handle>(pub(super) MIO<'handle, mio::net::TcpListener>);

impl TcpListenerHandle {
    pub fn bind(addr: &SocketAddr) -> Result<Self> {
        mio::net::TcpListener::bind(addr).map(|mio_data| { Self(MIOHandle::new_r(mio_data)) })
    }

    pub async fn acceptor(&mut self) -> TcpListenerAcceptor<'_> {
        TcpListenerAcceptor(MIO::new_r(&self.0).await)
    }
}

impl<'handle> TcpListenerAcceptor<'handle> {
    pub async fn accept(&mut self)
                        -> Result<(TcpStreamReadHandle, TcpStreamWriteHandle, SocketAddr)> {
        let map = |res: (mio::net::TcpStream, SocketAddr)| {
            let mio_data = res.0;
            let addr = res.1;
            let (read_handle, write_handle) = MIOHandle::new_rw(mio_data);
            let read_handle = TcpStreamReadHandle(read_handle);
            let write_handle = TcpStreamWriteHandle(write_handle);
            (read_handle, write_handle, addr)
        };
        io_poll!(self.0.dispatcher, self.0.inner_mut().accept(), map);
    }
}
