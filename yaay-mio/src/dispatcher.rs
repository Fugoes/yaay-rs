use std::future::Future;
use std::pin::Pin;
use std::ptr::{NonNull, null_mut};
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering::{Relaxed, SeqCst};
use std::task::{Context, Poll, Waker};

use mio::Ready;

use crate::helper::{do_drop, do_new, WakerFuture};
use crate::shared::LocalData;

pub struct Dispatcher {
    inner: NonNull<DispatcherInner>,
}

unsafe impl Send for Dispatcher {}

unsafe impl Sync for Dispatcher {}

impl Dispatcher {
    const MUTED: u8 = 1;
    const NOTIFIED: u8 = Self::MUTED << 1;

    pub(crate) async unsafe fn new(local_i: usize, key: usize, ready: Ready) -> Self {
        let waker = WakerFuture().await;
        Self::new_from_waker(local_i, key, ready, waker)
    }

    pub(crate) unsafe fn del(&self, local_i: usize, key: usize) {
        let local = LocalData::get_i(local_i);
        let mut guard = local.dispatchers.lock();
        let slot = guard.get_unchecked_mut(key);
        if slot.readable_dispatcher == self.inner.as_ptr() {
            slot.readable_dispatcher = null_mut()
        } else if slot.writable_dispatcher == self.inner.as_ptr() {
            slot.writable_dispatcher = null_mut();
        } else {
            unreachable!();
        };
        drop(guard);

        do_drop(self.inner);
    }

    #[inline]
    pub(crate) fn prepare_io(&self) {
        self.status_ref().swap(Dispatcher::MUTED, SeqCst);
    }

    #[inline]
    pub(crate) fn status_ref(&self) -> &AtomicU8 {
        unsafe { &self.inner.as_ref().status }
    }

    #[inline]
    pub(crate) async fn wait_io(&self) {
        let status = self.status_ref();
        loop {
            match status.compare_exchange_weak(Dispatcher::MUTED, 0, SeqCst, Relaxed) {
                Ok(_) => {
                    PendingOnce(true).await;
                    return;
                }
                Err(val) => {
                    if val != Dispatcher::MUTED {
                        return;
                    };
                }
            };
        };
    }
}

impl Dispatcher {
    unsafe fn new_from_waker(local_i: usize, key: usize, ready: Ready, waker: Waker) -> Self {
        let inner = do_new(DispatcherInner::new(waker)).unwrap();
        let dispatcher = Self { inner };

        let local = LocalData::get_i(local_i);
        let mut guard = local.dispatchers.lock();
        let slot = guard.get_unchecked_mut(key);
        if ready.is_readable() {
            assert!(slot.readable_dispatcher.is_null());
            slot.readable_dispatcher = dispatcher.inner.as_ptr();
        } else if ready.is_writable() {
            assert!(slot.writable_dispatcher.is_null());
            slot.writable_dispatcher = dispatcher.inner.as_ptr();
        } else {
            unreachable!();
        };
        drop(guard);

        dispatcher
    }
}

pub(crate) struct DispatcherInner {
    status: AtomicU8,
    waker: Waker,
}

impl DispatcherInner {
    #[inline]
    fn new(waker: Waker) -> Self {
        Self { status: AtomicU8::new(Dispatcher::MUTED), waker }
    }

    #[inline]
    pub(crate) fn notify(&self) {
        let prev = self.status.swap(Dispatcher::NOTIFIED | Dispatcher::MUTED, SeqCst);
        if prev & (Dispatcher::MUTED | Dispatcher::NOTIFIED) == 0 {
            self.waker.wake_by_ref();
        };
    }
}

pub(crate) struct PendingOnce(bool);

impl Future for PendingOnce {
    type Output = ();

    #[inline]
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut_self = unsafe { self.get_unchecked_mut() };
        if mut_self.0 {
            mut_self.0 = false;
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}
