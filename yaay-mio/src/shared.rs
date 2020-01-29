use std::ptr::null_mut;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering::Relaxed;

use parking_lot::Mutex;
use slab::Slab;

use crate::dispatcher::RWDispatcher;

pub(crate) struct SharedData {
    pub(crate) poll: mio::Poll,
    pub(crate) dispatchers: Mutex<Slab<RWDispatcher>>,
    pub(crate) deferred_remove: Mutex<Vec<usize>>,
}

impl SharedData {
    pub(crate) fn new() -> Self {
        Self {
            poll: mio::Poll::new().unwrap(),
            dispatchers: Mutex::new(Slab::new()),
            deferred_remove: Mutex::new(Vec::new()),
        }
    }

    #[inline]
    pub(crate) fn get<'a>() -> &'a mut SharedData { unsafe { &mut *SHARED.load(Relaxed) } }

    #[inline]
    pub(crate) unsafe fn set_ptr<'a>(ptr: *mut SharedData) { SHARED.store(ptr, Relaxed) }
}

static SHARED: AtomicPtr<SharedData> = AtomicPtr::new(null_mut());
