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
    inner: *mut DispatcherInner,
}

unsafe impl Send for Dispatcher {}

unsafe impl Sync for Dispatcher {}

impl Dispatcher {
    const MUTED: u8 = 1;
    const NOTIFIED: u8 = Self::MUTED << 1;

    pub(crate) unsafe fn set(&mut self, that: &Dispatcher) {
        self.inner = that.inner;
    }

    pub(crate) unsafe fn null() -> Self { Self { inner: null_mut() } }

    pub(crate) fn is_null(&self) -> bool { self.inner.is_null() }

    unsafe fn new_from_waker(local_i: usize, key: usize, ready: Ready, waker: Waker) -> Self {
        let inner = do_new(DispatcherInner::new(waker)).unwrap().as_ptr();
        let dispatcher = Self { inner };

        let local = LocalData::get_i(local_i);
        let mut guard = local.dispatchers.lock();
        let slot = guard.get_unchecked_mut(key);
        if ready.is_readable() {
            assert!(slot.readable_dispatcher.is_null());
            slot.readable_dispatcher.set(&dispatcher);
        } else if ready.is_writable() {
            assert!(slot.writable_dispatcher.is_null());
            slot.writable_dispatcher.set(&dispatcher);
        } else {
            unreachable!();
        };
        drop(guard);

        dispatcher
    }

    pub(crate) async unsafe fn new(local_i: usize, key: usize, ready: Ready) -> Self {
        let waker = WakerFuture().await;
        Self::new_from_waker(local_i, key, ready, waker)
    }

    pub(crate) unsafe fn del(&self, local_i: usize, key: usize) {
        let local = LocalData::get_i(local_i);
        let mut guard = local.dispatchers.lock();
        let slot = guard.get_unchecked_mut(key);
        if slot.readable_dispatcher.inner == self.inner {
            slot.readable_dispatcher.set(&Dispatcher::null());
        } else if slot.writable_dispatcher.inner == self.inner {
            slot.writable_dispatcher.set(&Dispatcher::null());
        } else {
            unreachable!();
        };
        drop(guard);

        do_drop(NonNull::new_unchecked(self.inner));
    }

    #[inline]
    pub(crate) fn prepare_io(&self) {
        unsafe { (*self.inner).status.swap(Dispatcher::MUTED, SeqCst) };
    }

    #[inline]
    pub(crate) fn status_ref(&self) -> &AtomicU8 {
        unsafe { &(*self.inner).status }
    }

    #[inline]
    pub(crate) async fn wait_io(&self) {
        let status = self.status_ref();
        loop {
            match status.compare_exchange_weak(Dispatcher::MUTED, 0, SeqCst, Relaxed) {
                Ok(_) => return,
                Err(val) => {
                    if val != Dispatcher::MUTED {
                        PendingOnce(true).await;
                    };
                }
            };
        };
    }

    #[inline]
    pub(crate) fn notify(&self) {
        unsafe {
            let prev = (*self.inner).status.swap(Dispatcher::NOTIFIED | Dispatcher::MUTED, SeqCst);
            if prev & Dispatcher::MUTED == 0 && prev & Dispatcher::NOTIFIED == 0 {
                (*self.inner).waker.wake_by_ref();
            };
        };
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
}

pub(crate) struct PendingOnce(bool);

impl Future for PendingOnce {
    type Output = ();

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
