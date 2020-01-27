use std::ptr::{NonNull, null_mut};

use crate::mem::{do_drop, do_new};
use crate::static_var::{get_local, set_local};
use crate::task::RuntimeLocalData;
use crate::worker_manager::RuntimeSharedData;

pub struct BatchGuard();

impl BatchGuard {
    pub fn new() -> Self {
        let local = unsafe { do_new(RuntimeLocalData::new()) }.unwrap();
        assert!(unsafe { get_local::<()>().is_null() });
        unsafe { set_local(local.as_ptr()) };
        Self()
    }

    pub fn next_batch(&self) {
        let shared = RuntimeSharedData::get();
        let local = unsafe { RuntimeLocalData::get_unchecked() };
        let mut all_empty = true;
        local.task_lists.get_mut().iter_mut()
            .enumerate()
            .for_each(|(worker_id, tasks)| {
                if !tasks.is_empty() {
                    all_empty = false;
                    let tasks = tasks.pop_all();
                    let worker = unsafe { shared.worker_ptrs.get_unchecked(worker_id) };
                    unsafe { worker.as_ref().task_list.lock().push_back_batch(tasks) };
                };
            });
        if !all_empty { unsafe { shared.epoch.as_ref().next_epoch() }; };
    }
}

impl Drop for BatchGuard {
    fn drop(&mut self) {
        self.next_batch();
        let local = unsafe { get_local() } as *mut RuntimeLocalData;
        unsafe { set_local(null_mut() as *mut ()) };
        if !local.is_null() { unsafe { do_drop(NonNull::new_unchecked(local)) }; };
    }
}
