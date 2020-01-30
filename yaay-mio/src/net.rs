use std::future::Future;
use std::io;
use std::net::{Shutdown, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use iovec::IoVec;
use mio::Ready;

use crate::io_traits::{AsyncVectoredRead, AsyncVectoredWrite};
use crate::mio_box::{MIOBox, MIOData};

pub struct TcpListener(Arc<MIOData<mio::net::TcpListener>>);

impl TcpListener {
    #[inline]
    pub async fn acceptor(&self) -> TcpListenerAcceptor {
        TcpListenerAcceptor(MIOBox::new(self.0.clone(), true).await)
    }

    #[inline]
    pub fn bind(addr: &SocketAddr) -> io::Result<TcpListener> {
        mio::net::TcpListener::bind(addr).map(|mio_data| {
            Self(Arc::new(MIOData::new(mio_data, Ready::readable())))
        })
    }

    #[inline]
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.inner.local_addr()
    }

    #[inline]
    pub fn try_clone(&self) -> io::Result<TcpListener> {
        self.0.inner.try_clone().map(|mio_data| {
            Self(Arc::new(MIOData::new(mio_data, Ready::readable())))
        })
    }

    #[inline]
    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.0.inner.set_ttl(ttl)
    }

    #[inline]
    pub fn ttl(&self) -> io::Result<u32> {
        self.0.inner.ttl()
    }

    #[inline]
    pub fn set_only_v6(&self, only_v6: bool) -> io::Result<()> {
        self.0.inner.set_only_v6(only_v6)
    }

    #[inline]
    pub fn only_v6(&self) -> io::Result<bool> {
        self.0.inner.only_v6()
    }

    #[inline]
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.0.inner.take_error()
    }
}

pub struct TcpListenerAcceptor(MIOBox<mio::net::TcpListener>);

pub struct TcpListenerAcceptorAcceptFuture<'a>(&'a TcpListenerAcceptor);

impl<'a> Future for TcpListenerAcceptorAcceptFuture<'a> {
    type Output = io::Result<(TcpStream, SocketAddr)>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let map = |res: (mio::net::TcpStream, SocketAddr)| {
            let events = mio::Ready::readable(); // accept is readable event
            (TcpStream(Arc::new(MIOData::new(res.0, events))), res.1)
        };
        io_poll!((self.0).0.dispatcher, (self.0).0.mio_data.inner.accept(), map);
    }
}

impl TcpListenerAcceptor {
    #[inline]
    pub fn accept(&self) -> TcpListenerAcceptorAcceptFuture {
        TcpListenerAcceptorAcceptFuture(self)
    }
}


pub struct TcpStream(Arc<MIOData<mio::net::TcpStream>>);

impl TcpStream {
    #[inline]
    pub async fn reader(&self) -> TcpStreamReader {
        TcpStreamReader(MIOBox::new(self.0.clone(), true).await)
    }

    #[inline]
    pub async fn writer(&self) -> TcpStreamWriter {
        TcpStreamWriter(MIOBox::new(self.0.clone(), false).await)
    }

    #[inline]
    pub fn connect(addr: &SocketAddr) -> io::Result<Self> {
        mio::net::TcpStream::connect(addr).map(|mio_data| {
            Self(Arc::new(MIOData::new(mio_data, Ready::readable() | Ready::writable())))
        })
    }

    #[inline]
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.0.inner.peer_addr()
    }

    #[inline]
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.inner.local_addr()
    }

    #[inline]
    pub fn try_clone(&self) -> io::Result<Self> {
        self.0.inner.try_clone().map(|mio_data| {
            Self(Arc::new(MIOData::new(mio_data, Ready::readable() | Ready::writable())))
        })
    }

    #[inline]
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.0.inner.shutdown(how)
    }

    #[inline]
    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.0.inner.set_nodelay(nodelay)
    }

    #[inline]
    pub fn nodelay(&self) -> io::Result<bool> {
        self.0.inner.nodelay()
    }

    #[inline]
    pub fn set_recv_buffer_size(&self, size: usize) -> io::Result<()> {
        self.0.inner.set_recv_buffer_size(size)
    }

    #[inline]
    pub fn recv_buffer_size(&self) -> io::Result<usize> {
        self.0.inner.recv_buffer_size()
    }

    #[inline]
    pub fn set_send_buffer_size(&self, size: usize) -> io::Result<()> {
        self.0.inner.set_send_buffer_size(size)
    }

    #[inline]
    pub fn send_buffer_size(&self) -> io::Result<usize> {
        self.0.inner.send_buffer_size()
    }

    #[inline]
    pub fn set_keepalive(&self, keepalive: Option<Duration>) -> io::Result<()> {
        self.0.inner.set_keepalive(keepalive)
    }

    #[inline]
    pub fn keepalive(&self) -> io::Result<Option<Duration>> {
        self.0.inner.keepalive()
    }

    #[inline]
    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.0.inner.set_ttl(ttl)
    }

    #[inline]
    pub fn ttl(&self) -> io::Result<u32> {
        self.0.inner.ttl()
    }

    #[inline]
    pub fn set_only_v6(&self, only_v6: bool) -> io::Result<()> {
        self.0.inner.set_only_v6(only_v6)
    }

    #[inline]
    pub fn only_v6(&self) -> io::Result<bool> {
        self.0.inner.only_v6()
    }

    #[inline]
    pub fn set_linger(&self, dur: Option<Duration>) -> io::Result<()> {
        self.0.inner.set_linger(dur)
    }

    #[inline]
    pub fn linger(&self) -> io::Result<Option<Duration>> {
        self.0.inner.linger()
    }

    #[inline]
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.0.inner.take_error()
    }
}

pub struct TcpStreamReader(MIOBox<mio::net::TcpStream>);

pub struct TcpStreamReaderReadBufsFuture<'a, 'b>(&'a TcpStreamReader, &'a mut [&'b mut IoVec]);

impl<'a, 'b> Future for TcpStreamReaderReadBufsFuture<'a, 'b> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut_self = unsafe { self.get_unchecked_mut() };
        io_poll!((mut_self.0).0.dispatcher, (mut_self.0).0.mio_data.inner.read_bufs(mut_self.1));
    }
}

impl<'a, 'b: 'a> AsyncVectoredRead<'a, 'b> for TcpStreamReader {
    type R = TcpStreamReaderReadBufsFuture<'a, 'b>;

    fn read_bufs(&'a self, bufs: &'a mut [&'b mut IoVec]) -> Self::R {
        TcpStreamReaderReadBufsFuture(self, bufs)
    }
}

pub struct TcpStreamWriter(MIOBox<mio::net::TcpStream>);

pub struct TcpStreamWriterWriteBufsFuture<'a, 'b>(&'a TcpStreamWriter, &'a [&'b IoVec]);

impl<'a, 'b> Future for TcpStreamWriterWriteBufsFuture<'a, 'b> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut_self = unsafe { self.get_unchecked_mut() };
        io_poll!((mut_self.0).0.dispatcher, (mut_self.0).0.mio_data.inner.write_bufs(mut_self.1));
    }
}

impl<'a, 'b: 'a> AsyncVectoredWrite<'a, 'b> for TcpStreamWriter {
    type R = TcpStreamWriterWriteBufsFuture<'a, 'b>;

    fn write_bufs(&'a self, bufs: &'a [&'b IoVec]) -> Self::R {
        TcpStreamWriterWriteBufsFuture(self, bufs)
    }
}
