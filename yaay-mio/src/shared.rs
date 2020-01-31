use std::ptr::{NonNull, null_mut};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize};
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::thread::JoinHandle;
use std::time::Duration;

use parking_lot::{Condvar, Mutex};
use slab::Slab;

use yaay_runtime_api::RuntimeAPI;

use crate::dispatcher::Dispatcher;

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub(crate) struct LocalData {
    pub(crate) poll: mio::Poll,
    pub(crate) dispatchers: Mutex<Slab<Slot>>,
    pub(crate) deferred_remove: Mutex<Vec<usize>>,
}

unsafe impl Send for LocalData {}

unsafe impl Sync for LocalData {}

pub(crate) struct Slot {
    pub(crate) readable_dispatcher: Dispatcher,
    pub(crate) writable_dispatcher: Dispatcher,
}

pub(crate) struct GlobalData {
    pub(crate) locals: Box<[NonNull<LocalData>]>,
    pub(crate) counter: AtomicUsize,
}

impl LocalData {
    #[inline]
    pub(crate) fn get<'a>() -> &'a LocalData {
        unsafe { &*LOCAL.with(|x| x.load(Relaxed)) }
    }

    #[inline]
    pub(crate) fn get_i<'a>(i: usize) -> &'a LocalData {
        unsafe { (*GLOBAL.load(Relaxed)).locals.get_unchecked(i).as_ref() }
    }

    unsafe fn build() -> NonNull<LocalData> {
        let poll = mio::Poll::new().unwrap();
        let dispatchers = Mutex::new(Slab::new());
        let deferred_remove = Mutex::new(Vec::new());
        let ptr = Box::into_raw(Box::new(LocalData { poll, dispatchers, deferred_remove }));
        NonNull::new(ptr).unwrap()
    }
}

impl Slot {
    pub(crate) fn new() -> Self {
        Self {
            readable_dispatcher: unsafe { Dispatcher::null() },
            writable_dispatcher: unsafe { Dispatcher::null() },
        }
    }
}

impl GlobalData {
    #[inline]
    pub(crate) fn get<'a>() -> &'a GlobalData {
        unsafe { &*GLOBAL.load(Relaxed) }
    }

    pub(crate) fn build<RT: RuntimeAPI>(n_threads: usize) -> Box<[JoinHandle<()>]> {
        let mut handles = Vec::with_capacity(n_threads);
        let ptrs = Arc::new(Mutex::new(vec![0 as usize; n_threads]
            .into_boxed_slice()));
        let cond = Arc::new(Condvar::new());

        for tid in 0..n_threads {
            let ptrs = ptrs.clone();
            let cond = cond.clone();
            let handle =
                std::thread::Builder::new()
                    .name(format!("yaay-mio-{}", tid))
                    .spawn(move || unsafe {
                        let ptr = LocalData::build();
                        let mut guard = ptrs.lock();
                        guard[tid] = ptr.as_ptr() as usize;
                        while !guard.iter().all(|x| !(*x as *const ()).is_null()) {
                            cond.wait(&mut guard);
                        };
                        drop(guard);
                        cond.notify_all();
                        drop(ptrs);
                        drop(cond);

                        let batch = RT::batch_guard();
                        let local = LocalData::get();
                        let mut poll_events = mio::Events::with_capacity(1024);
                        let timeout = Some(Duration::from_millis(1000));
                        while !SHUTDOWN.load(Acquire) {
                            let n = local.poll.poll(&mut poll_events, timeout);
                            let guard = local.dispatchers.lock();
                            for poll_event in poll_events.iter() {
                                let key = poll_event.token().0;
                                let ready = poll_event.readiness();
                                let slot = guard.get_unchecked(key);
                                if ready.is_readable() && !slot.readable_dispatcher.is_null() {
                                    slot.readable_dispatcher.notify();
                                };
                                if ready.is_writable() && !slot.writable_dispatcher.is_null() {
                                    slot.writable_dispatcher.notify();
                                };
                            };
                            drop(guard);
                            n.map(|x| if x > 0 { RT::push_batch(&batch) }).unwrap();
                            'outer: loop {
                                let key = local.deferred_remove.lock().pop();
                                match key {
                                    Some(key) => local.dispatchers.lock().remove(key),
                                    None => break 'outer,
                                };
                            };
                        };
                        'outer0: loop {
                            let key = local.deferred_remove.lock().pop();
                            match key {
                                Some(key) => local.dispatchers.lock().remove(key),
                                None => break 'outer0,
                            };
                        };
                    })
                    .unwrap();
            handles.push(handle);
        };

        let mut guard = ptrs.lock();
        while !guard.iter().all(|x| !(*x as *const ()).is_null()) { cond.wait(&mut guard); };
        drop(guard);

        let locals = ptrs.lock().iter().map(|x| unsafe { NonNull::new_unchecked(*x as *mut _) })
            .collect();
        let counter = AtomicUsize::new(0);

        let global = GlobalData { locals, counter };
        drop(ptrs);
        drop(cond);

        let ptr = Box::into_raw(Box::new(global));
        GLOBAL.store(ptr, Release);

        handles.into_boxed_slice()
    }
}

thread_local! {
static LOCAL: AtomicPtr<LocalData> = AtomicPtr::new(null_mut());
}
static GLOBAL: AtomicPtr<GlobalData> = AtomicPtr::new(null_mut());

pub(crate) fn set_shutdown() {
    SHUTDOWN.store(true, Release);
}
