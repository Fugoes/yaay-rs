use std::ptr::null_mut;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering::Relaxed;

static GLOBAL: AtomicPtr<()> = AtomicPtr::new(null_mut());
thread_local! {
static LOCAL: AtomicPtr<()> = AtomicPtr::new(null_mut());
}

#[inline]
pub(crate) unsafe fn set_global<T>(ptr: *mut T) { GLOBAL.store(ptr as *mut (), Relaxed); }

#[inline]
pub(crate) unsafe fn get_global<T>() -> *mut T { GLOBAL.load(Relaxed) as *mut T }

#[inline]
pub(crate) unsafe fn set_local<T>(ptr: *mut T) { LOCAL.with(|x| x.store(ptr as *mut (), Relaxed)); }

#[inline]
pub(crate) unsafe fn get_local<T>() -> *mut T { LOCAL.with(|x| x.load(Relaxed)) as *mut T }
