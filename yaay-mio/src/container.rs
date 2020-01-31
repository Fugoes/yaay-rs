use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::Ordering::SeqCst;

use mio::{PollOpt, Ready, Token};

use crate::dispatcher::Dispatcher;
use crate::shared::{GlobalData, LocalData, Slot};

/// A mio cell holds a mio evented instance. It include drop implementation.
pub(crate) struct MIOCell<T> where T: mio::Evented + Send {
    local_i: usize,
    key: usize,
    pub(crate) inner: UnsafeCell<T>,
}

/// A mio handle represents a mio evented instance which accept one kind of event (either
/// readable or writable). Safe to transfer between top level tasks. An `Arc` is needed here because
/// multiple handle might be created from same mio evented instance. For example, a stream instance
/// would leads to one write handle and one read handle. This is for convenience of doing concurrent
/// read and write on same mio evented instance.
pub(crate) struct MIOHandle<T> where T: mio::Evented + Send {
    pub(crate) inner: Arc<MIOCell<T>>,
}

unsafe impl<T> Send for MIOHandle<T> where T: mio::Evented + Send {}

unsafe impl<T> Sync for MIOHandle<T> where T: mio::Evented + Send {}

/// A mio wraps up a mio handle, it is bind to one top level task. On creation, it would store the
/// task's waker inside the dispatcher. Since waker clone is expensive operation, we use
/// the 'task-local' `MIO<T>` to avoid waker clone. Only one `MIO<T>` is created from one
/// `MIOHandle<T>` instance.
pub(crate) struct MIO<'handle, T> where T: mio::Evented + Send {
    pub(crate) dispatcher: Dispatcher,
    pub(crate) handle: &'handle MIOHandle<T>,
}

impl<T> Drop for MIOCell<T> where T: mio::Evented + Send {
    fn drop(&mut self) {
        let local = LocalData::get_i(self.local_i);

        let _ = local.poll.deregister(unsafe { &*self.inner.get() });

        let mut guard = local.dispatchers.lock();
        let slot = unsafe { guard.get_unchecked_mut(self.key) };
        assert!(slot.readable_dispatcher.is_null());
        assert!(slot.writable_dispatcher.is_null());
        let mut deferred = local.deferred_remove.lock();
        deferred.push(self.key);
        drop(deferred);
        drop(guard);
    }
}

impl<T> MIOHandle<T> where T: mio::Evented + Send {
    pub(crate) fn new_r(t: T) -> Self {
        let global = GlobalData::get();
        let local_i = global.counter.fetch_add(1, SeqCst) % global.locals.len();
        let local = LocalData::get_i(local_i);
        let key = local.dispatchers.lock().insert(Slot::new());
        local.poll.register(&t, Token(key), Ready::readable(), PollOpt::edge()).unwrap();

        Self { inner: Arc::new(MIOCell { local_i, key, inner: UnsafeCell::new(t) }) }
    }

    #[allow(dead_code)]
    pub(crate) fn new_w(t: T) -> Self {
        let global = GlobalData::get();
        let local_i = global.counter.fetch_add(1, SeqCst) % global.locals.len();
        let local = LocalData::get_i(local_i);
        let key = local.dispatchers.lock().insert(Slot::new());
        local.poll.register(&t, Token(key), Ready::writable(), PollOpt::edge()).unwrap();

        Self { inner: Arc::new(MIOCell { local_i, key, inner: UnsafeCell::new(t) }) }
    }

    pub(crate) fn new_rw(t: T) -> (Self, Self) {
        let global = GlobalData::get();
        let local_i = global.counter.fetch_add(1, SeqCst) % global.locals.len();
        let local = LocalData::get_i(local_i);
        let key = local.dispatchers.lock().insert(Slot::new());
        local.poll.register(&t, Token(key), Ready::readable() | Ready::writable(), PollOpt::edge())
            .unwrap();

        let inner = Arc::new(MIOCell { local_i, key, inner: UnsafeCell::new(t) });

        (Self { inner: inner.clone() }, Self { inner: inner.clone() })
    }
}

impl<'handle, T> MIO<'handle, T> where T: mio::Evented + Send {
    pub(crate) async fn new_r(handle: &'handle MIOHandle<T>) -> MIO<'handle, T> {
        let local_i = handle.inner.local_i;
        let key = handle.inner.key;
        let dispatcher = unsafe { Dispatcher::new(local_i, key, Ready::readable()) }.await;
        Self { dispatcher, handle }
    }

    pub(crate) async fn new_w(handle: &'handle MIOHandle<T>) -> MIO<'handle, T> {
        let local_i = handle.inner.local_i;
        let key = handle.inner.key;
        let dispatcher = unsafe { Dispatcher::new(local_i, key, Ready::writable()) }.await;
        Self { dispatcher, handle }
    }

    #[inline]
    pub(crate) fn inner_mut(&mut self) -> &mut T {
        unsafe { &mut *self.handle.inner.inner.get() }
    }
}

impl<'handle, T> Drop for MIO<'handle, T> where T: mio::Evented + Send {
    fn drop(&mut self) {
        let local_i = self.handle.inner.local_i;
        let key = self.handle.inner.key;
        unsafe { self.dispatcher.del(local_i, key); }
    }
}
