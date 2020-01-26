use std::alloc::{alloc, dealloc, Layout};
use std::ptr::NonNull;

#[inline]
pub(crate) unsafe fn do_alloc<T>() -> Option<NonNull<T>> {
    let layout = Layout::new::<T>();
    let ptr = alloc(layout) as *mut T;
    if ptr.is_null() { None } else { Some(NonNull::new_unchecked(ptr)) }
}

#[inline]
pub(crate) unsafe fn do_new<T>(data: T) -> Option<NonNull<T>> {
    let ptr = do_alloc::<T>();
    ptr.map(|x| x.as_ptr().write(data));
    ptr
}

#[inline]
pub(crate) unsafe fn do_dealloc<T>(ptr: NonNull<T>) {
    let layout = Layout::new::<T>();
    dealloc(ptr.as_ptr() as *mut u8, layout);
}
