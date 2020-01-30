use std::future::Future;
use std::io;
use std::net::{Shutdown, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use iovec::IoVec;
use mio::Ready;

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

pub struct TcpStreamReaderReadBufsFuture<'s, 'bufs, 'iovec: 'bufs> {
    mio_box: &'s TcpStreamReader,
    bufs: &'bufs mut [&'iovec mut IoVec],
}

impl<'s, 'bufs, 'iovec: 'bufs> Future for TcpStreamReaderReadBufsFuture<'s, 'bufs, 'iovec> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut_self = unsafe { self.get_unchecked_mut() };
        io_poll!(
            mut_self.mio_box.0.dispatcher,
            mut_self.mio_box.0.mio_data.inner.read_bufs(mut_self.bufs)
        );
    }
}

impl TcpStreamReader {
    #[inline]
    pub async fn read_bufs(&self, bufs: &mut [&mut IoVec]) -> io::Result<usize> {
        let mio_box = self;
        TcpStreamReaderReadBufsFuture { mio_box, bufs }.await
    }

    #[inline]
    pub async fn read(&self, buf: &mut IoVec) -> io::Result<usize> {
        let mut bufs = [buf];
        self.read_bufs(&mut bufs).await
    }

    #[inline]
    pub async fn read_exact(&self, buf: &mut IoVec) -> io::Result<()> {
        let mut begin = 0;
        while begin < buf.len() {
            let buf: &mut IoVec = (&mut buf[begin..]).into();
            match self.read(buf).await {
                Ok(n) => begin += n,
                Err(err) => return Err(err),
            };
        };
        Ok(())
    }
}

pub struct TcpStreamWriter(MIOBox<mio::net::TcpStream>);

pub struct TcpStreamWriterWriteBufsFuture<'s, 'bufs, 'iovec: 'bufs> {
    mio_box: &'s TcpStreamWriter,
    bufs: &'bufs [&'iovec IoVec],
}

impl<'s, 'bufs, 'iovec: 'bufs> Future for TcpStreamWriterWriteBufsFuture<'s, 'bufs, 'iovec> {
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut_self = unsafe { self.get_unchecked_mut() };
        io_poll!(
            mut_self.mio_box.0.dispatcher,
            mut_self.mio_box.0.mio_data.inner.write_bufs(mut_self.bufs)
        );
    }
}

impl TcpStreamWriter {
    #[inline]
    pub async fn write_bufs(&self, bufs: &[&IoVec]) -> io::Result<usize> {
        let mio_box = self;
        TcpStreamWriterWriteBufsFuture { mio_box, bufs }.await
    }

    #[inline]
    pub async fn write(&self, buf: &IoVec) -> io::Result<usize> {
        let bufs = [buf];
        self.write_bufs(&bufs).await
    }

    #[inline]
    pub async fn write_exact(&self, buf: &IoVec) -> io::Result<()> {
        let mut begin = 0;
        while begin < buf.len() {
            let buf: &IoVec = buf[begin..].into();
            match self.write(buf).await {
                Ok(n) => begin += n,
                Err(err) => return Err(err),
            };
        };
        Ok(())
    }
}
