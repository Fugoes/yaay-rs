use std::sync::Arc;

use mio::{PollOpt, Ready};

use crate::dispatcher::{AtomicDispatcher, RWDispatcher};
use crate::shared::SharedData;

pub(crate) struct MIOData<T> where T: mio::Evented {
    pub(crate) key: usize,
    pub(crate) inner: T,
}

impl<T> MIOData<T> where T: mio::Evented {
    pub(crate) fn new(inner: T, ready: Ready) -> Self {
        let shared = SharedData::get();
        let key = shared.dispatchers.lock().insert(RWDispatcher::new());
        shared.poll.register(&inner, mio::Token(key), ready, PollOpt::edge()).unwrap();
        Self { key, inner }
    }
}

impl<T> Drop for MIOData<T> where T: mio::Evented {
    fn drop(&mut self) {
        let shared = SharedData::get();
        shared.poll.deregister(&self.inner).unwrap();
        shared.dispatchers.lock().remove(self.key);
    }
}

pub(crate) struct MIOBox<T> where T: mio::Evented {
    pub(crate) dispatcher: AtomicDispatcher,
    pub(crate) mio_data: Arc<MIOData<T>>,
}

impl<T> MIOBox<T> where T: mio::Evented {
    pub(crate) async fn new(mio_data: Arc<MIOData<T>>, is_reader: bool) -> Self {
        let dispatcher = unsafe { AtomicDispatcher::new(mio_data.key, is_reader) }.await;
        Self { dispatcher, mio_data }
    }
}

impl<T> Drop for MIOBox<T> where T: mio::Evented {
    fn drop(&mut self) {
        unsafe { self.dispatcher.del(self.mio_data.key) };
    }
}
