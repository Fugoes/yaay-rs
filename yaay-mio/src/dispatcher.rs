use std::future::Future;
use std::pin::Pin;
use std::ptr::NonNull;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering::{Relaxed, SeqCst};
use std::task::{Context, Poll, Waker};

use crate::mem::{do_drop, do_new};
use crate::shared::SharedData;

pub(crate) struct RWDispatcher {
    readable_dispatchers: Vec<*mut AtomicDispatcherInner>,
    writable_dispatchers: Vec<*mut AtomicDispatcherInner>,
}

impl RWDispatcher {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            readable_dispatchers: Vec::new(),
            writable_dispatchers: Vec::new(),
        }
    }

    #[inline]
    pub(crate) fn dispatch_readable(&self) {
        self.readable_dispatchers.iter().for_each(|dispatcher| unsafe {
            (**dispatcher).notify();
        });
    }

    #[inline]
    pub(crate) fn dispatch_writable(&self) {
        self.writable_dispatchers.iter().for_each(|dispatcher| unsafe {
            (**dispatcher).notify();
        });
    }
}

pub(crate) struct AtomicDispatcher {
    inner: *mut AtomicDispatcherInner,
}

unsafe impl Send for AtomicDispatcher {}

unsafe impl Sync for AtomicDispatcher {}

impl AtomicDispatcher {
    pub(crate) async unsafe fn new(key: usize, is_readable: bool) -> Self {
        struct RFuture();

        impl Future for RFuture {
            type Output = Waker;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                Poll::Ready(cx.waker().clone())
            }
        }

        let inner = do_new(AtomicDispatcherInner::new(RFuture().await)).unwrap().as_ptr();

        let shared = SharedData::get();
        if is_readable {
            let mut guard = shared.dispatchers.lock();
            guard.get_unchecked_mut(key).readable_dispatchers.push(inner);
        } else {
            let mut guard = shared.dispatchers.lock();
            guard.get_unchecked_mut(key).writable_dispatchers.push(inner);
        };

        Self { inner }
    }

    pub(crate) unsafe fn del(&self, key: usize) {
        let shared = SharedData::get();
        let mut guard = shared.dispatchers.lock();
        let slot = guard.get_unchecked_mut(key);
        let len = slot.readable_dispatchers.len();
        slot.readable_dispatchers.retain(|x| *x != self.inner);
        if slot.readable_dispatchers.len() == len {
            slot.writable_dispatchers.retain(|x| *x != self.inner);
        };
        drop(guard);
        do_drop(NonNull::new_unchecked(self.inner));
    }

    #[inline]
    pub(crate) fn prepare_io(&self) {
        unsafe { (*self.inner).status.swap(AtomicDispatcherInner::MUTED, SeqCst) };
    }

    #[inline]
    pub(crate) fn try_wait_io(&self) -> bool {
        let status = unsafe { &(*self.inner).status };
        loop {
            match status.compare_exchange_weak(AtomicDispatcherInner::MUTED, 0, SeqCst, Relaxed) {
                Ok(_) => {
                    return true;
                }
                Err(val) => {
                    if val != AtomicDispatcherInner::MUTED {
                        return false;
                    };
                }
            }
        }
    }
}

struct AtomicDispatcherInner {
    status: AtomicU8,
    waker: Waker,
}

impl AtomicDispatcherInner {
    const MUTED: u8 = 1;
    const NOTIFIED: u8 = Self::MUTED << 1;

    #[inline]
    fn new(waker: Waker) -> Self { Self { status: AtomicU8::new(Self::MUTED), waker } }

    #[inline]
    fn notify(&self) {
        let prev = self.status.swap(Self::NOTIFIED | Self::MUTED, SeqCst);
        if prev & Self::MUTED == 0 && prev & Self::NOTIFIED == 0 { self.waker.wake_by_ref(); };
    }
}
