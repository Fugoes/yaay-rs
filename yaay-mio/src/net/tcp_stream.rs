use std::io::{Read, Result, Write};
use std::net::SocketAddr;

use crate::container::{MIO, MIOHandle};

pub struct TcpStreamReadHandle(pub(super) MIOHandle<mio::net::TcpStream>);

pub struct TcpStreamReader<'handle>(pub(super) MIO<'handle, mio::net::TcpStream>);

pub struct TcpStreamWriteHandle(pub(super) MIOHandle<mio::net::TcpStream>);

pub struct TcpStreamWriter<'handle>(pub(super) MIO<'handle, mio::net::TcpStream>);

pub struct TcpStream();

impl TcpStream {
    pub fn connect(addr: &SocketAddr) -> Result<(TcpStreamReadHandle, TcpStreamWriteHandle)> {
        mio::net::TcpStream::connect(addr).map(|mio_data| {
            let (read_handle, write_handle) = MIOHandle::new_rw(mio_data);
            let read_handle = TcpStreamReadHandle(read_handle);
            let write_handle = TcpStreamWriteHandle(write_handle);
            (read_handle, write_handle)
        })
    }
}

impl TcpStreamReadHandle {
    #[inline]
    pub async fn reader(&mut self) -> TcpStreamReader<'_> {
        TcpStreamReader(MIO::new_r(&self.0).await)
    }
}

impl TcpStreamWriteHandle {
    #[inline]
    pub async fn writer(&mut self) -> TcpStreamWriter<'_> {
        TcpStreamWriter(MIO::new_w(&self.0).await)
    }
}

impl<'handle> TcpStreamReader<'handle> {
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        io_poll!(self.0.dispatcher, self.0.inner_mut().read(buf));
    }

    pub async fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        io_poll!(self.0.dispatcher, self.0.inner_mut().read_exact(buf));
    }
}

impl<'handle> TcpStreamWriter<'handle> {
    pub async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        io_poll!(self.0.dispatcher, self.0.inner_mut().write(buf));
    }

    pub async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        io_poll!(self.0.dispatcher, self.0.inner_mut().write_all(buf));
    }
}
