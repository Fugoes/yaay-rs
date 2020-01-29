use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use mio::Ready;

use crate::mio_box::{MIOBox, MIOData};

pub struct TcpListener(Arc<MIOData<mio::net::TcpListener>>);

unsafe impl Send for TcpListener {}

pub struct TcpListenerAcceptor(MIOBox<mio::net::TcpListener>);

impl TcpListenerAcceptor {
    #[inline]
    pub async fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let mio_data = &self.0.mio_data.inner;
        loop {
            let waiter = self.0.dispatcher.prepare_io();
            match mio_data.accept() {
                Ok(res) => {
                    let events = mio::Ready::readable(); // accept is readable event
                    return Ok((TcpStream(Arc::new(MIOData::new(res.0, events))), res.1));
                }
                Err(err) => {
                    if err.kind() == io::ErrorKind::WouldBlock {
                        waiter.await;
                    } else {
                        return Err(err);
                    };
                }
            };
        };
    }
}

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


pub struct TcpStream(Arc<MIOData<mio::net::TcpStream>>);

unsafe impl Send for TcpStream {}

