use std::alloc::{alloc, dealloc, Layout};
use std::ptr::{drop_in_place, NonNull};

/// Alloc a non null pointer of type `T` and write `data` to it. Should be used with `do_drop<T>`
/// in pair, or with `do_drop_in_place<T>` and then `do_dealloc<T>` in pair.
#[inline]
pub(crate) unsafe fn do_new<T>(data: T) -> Option<NonNull<T>> {
    let ptr = do_alloc::<T>();
    ptr.map(|x| x.as_ptr().write(data));
    ptr
}

/// Run `drop` on the non null pointer and dealloc its memory. Should be used with `do_new<T>` in
/// pair.
#[inline]
pub(crate) unsafe fn do_drop<T>(ptr: NonNull<T>) {
    do_drop_in_place(ptr);
    do_dealloc(ptr);
}

/// Alloc a non null pointer of type `T` without doing initialization. Should be used with
/// `do_dealloc<T>` in pair.
#[inline]
pub(crate) unsafe fn do_alloc<T>() -> Option<NonNull<T>> {
    let layout = Layout::new::<T>();
    let ptr = alloc(layout) as *mut T;
    if ptr.is_null() { None } else { Some(NonNull::new_unchecked(ptr)) }
}

/// Dealloc a non null pointer's memory. Should be used with `do_alloc<T>` in pair.
#[inline]
pub(crate) unsafe fn do_dealloc<T>(ptr: NonNull<T>) {
    let layout = Layout::new::<T>();
    dealloc(ptr.as_ptr() as *mut u8, layout);
}

/// Run `drop()` on the non null pointer without dealloc its memory.
#[inline]
pub(crate) unsafe fn do_drop_in_place<T>(ptr: NonNull<T>) {
    drop_in_place(ptr.as_ptr());
}
