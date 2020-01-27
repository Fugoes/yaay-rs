use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::{Relaxed, SeqCst};
use std::time::{Duration, Instant};

use parking_lot_core::{park, ParkToken, unpark_all, UnparkToken};

/// Provides blocking for work stealing. Below provides a detailed explanation of how it works.
///
/// # Producer-consumer problem
/// The core functionality of a future runtime is polling tasks to make progress.
///
/// The naive approach would be just keeping polling all tasks in a round-robin fashion. This
/// approach is inefficient, especially when there are only few tasks could make progress.
///
/// The approach adapted by the rust `Future` design is using `Waker`s: when a task could make no
/// more progress, it registers its waker in some event notification mechanics (e.g. `epoll` under
/// Linux) to wait for some events, then returns `Poll::Pending` to the runtime. The waker is
/// provided by the runtime. When the event notification mechanics found the events the task is
/// waiting happens, it invokes the `Waker::wake()` method of the waker. The `Waker::wake()` method
/// would schedule the task so that the runtime would poll the task again later. This is a
/// producer-consumer problem: the event notification mechanics produce tasks to be `poll`ed
/// further, and the runtime consumes these `poll`able tasks by polling them.
///
/// A naive approach to solve the producer-consumer problem is using a task queue: the producer
/// keeps pushing new task to the queue, while the consumer keeps popping tasks from the queue and
/// polls them.
///
/// Usually the runtime is multi-threaded to maximize performance. So the task queue needs to be
/// thread safe. If we protected the single global task queue with a mutex, the thread safety is
/// guaranteed. However, this leads to a bottle neck: the throughput (lock/unlock operations per
/// seconds) has an upper bound, we cannot improve the upper bound by adding new threads to the
/// runtime. This method does not scale.
///
/// # Work-Stealing Dequeues
/// A good reference would be _The Art of Multiprocessor Programming. Chapter 16, Futures,
/// Scheduling, and Work Distribution. Section 16.5, Work-Stealing Dequeues_.
///
/// To improve the scalability of a single mutex-protected queue, we could assign each thread with
/// a local mutex-protected queue. When there is new task come in, the producer push the task to
/// a randomly selected local queue. The maximum throughput would be proportional to the number of
/// local queues. This method scales well.
///
/// This method still has problems: work distribution might not be uniform enough, so that some
/// threads' queues might have more time consuming tasks than others. Some threads might be very
/// busy while other threads are idling.
///
/// To solve this problem, when a thread found its local queue empty, it would try to steal tasks
/// from other threads' queues, a.k.a work-stealing. Work-stealing dequeues won't waste CPU time
/// to steal tasks when their local queue is not empty, and when some threads are not busy,
/// work-stealing happens so that the tail latency of the system would be improved. These are great!
///
/// # Termination Detecting Barriers
/// A good reference would be _The Art of Multiprocessor Programming. Chapter 17, Barriers. Section
/// 17.6, Termination Detecting Barriers_.
///
/// But what if all threads' local queues are empty? All worker threads in a work-stealing runtime
/// would try to steal tasks from other queues forever. We need to detect the termination.
///
/// Termination detecting barriers solve this problem partially by keeping an active count: This
/// active count is set to number of threads initially. When a thread found its local queue empty,
/// the thread would atomically decrease the active count by 1 to mark itself as an inactive thread,
/// and: (1) if the return value of the atomic operation indicates that all threads are inactive,
/// the thread exit; (2) if the return value of the atomic operation indicates that not all threads
/// are inactive, the thread try to do work-stealing until the condition in (1) holds. With the
/// termination detecting barriers, if no more tasks are pushed to these local queues, all threads
/// would exit finally, so that the whole runtime shutdown on termination.
///
/// However, under the use case of future runtime, there would always be new tasks pushed to these
/// local queues, so we cannot directly adopt the original termination detecting barriers.
///
/// # Our Approach: `Epoch`
/// Now we presents our approach: `Epoch`: it includes two counters, the `active_count` counter, and
/// the `epoch` counter. The `active_count` counter is 9-bits unsigned integer. The `epoch` counter
/// is 23-bits unsigned integer. They are stored in an `AtomicU32`, the `active_count` counter in
/// the lower 9-bits, and the `epoch` counter in the higher 23-bits. This layout ensures `epoch`
/// counter won't overflow to the `active_count` counter.
///
/// The worker thread's algorithm:
/// ```
/// loop {
///     // drain the local queue until it is empty               // L0
///     let mut old_instant = epoch.get_instant();               // L1
///     let mut old_status = epoch.set_inactive();               // L2
///     loop {
///         if Epoch::active_count(old_status) == 0 {            // L3
///             // recheck the local queue is empty              // L4
///             // if not empty, break                           // L5
///             epoch.wait_next_epoch(old_status, old_instant);  // L6
///         } else {
///             // try stealing task                             // L7
///             // if succeed, break                             // L8
///         }
///         // try pop from local queue                          // L9
///         // if succeed, break                                 // L10
///         old_instant = epoch.get_instant();                   // L11
///         old_status = epoch.get_status();                     // L12
///     }
///     epoch.set_active();                                      // L13
/// }
/// ```
/// When done push tasks, the producer needs to call `epoch.next_epoch()`.
///
/// The `active_count` counter generally acts like a termination detecting barrier. And when
/// termination is detected, instead of exiting, the worker would try to `wait_next_epoch()`.
/// Rechecking local queue before actually invoking `wait_next_epoch()` guarantees no deadlock would
/// happen (a.k.a when all worker threads are blocked on `L6`, there should be no task in all
/// local queues). Here is a not-so-strict proof for this no deadlock property.
///
/// _Lemma A_: If some threads is blocked on `L6`, and current `active_count` is not 0, these
/// threads would be unblocked eventually.
///
/// Proof: Assume some threads is blocked on `L6` and `active_count` is not 0 at time t_0. According
/// to `L3`, at some time t < t_0, the `active_count` is 0. Assume t_1 is the largest t at when the
/// `active_count` is 0, so that between t_1 and t_0, one process would call `set_active()` to set
/// `active_count` from 0 to 1. According to the implementation of `set_active()`, it would invoke
/// `unpark_all()` when set `active_count` from 0 to 1. So eventually, this `unblock_all()` would
/// unblock these threads.
///
/// _Lemma B_: If some threads is blocked on `L6` when invoking `epoch.next_epoch()`, these threads
/// would be unblocked eventually.
///
/// Proof: (1) If `epoch.next_epoch()` found `active_count` is not 0, according to Lemma A, all
/// worker threads would be unblocked eventually. (2) If `epoch.next_epoch()` found `active_count`
/// is 0, it would invoke `unpark_all()` so that these threads would be unblocked eventually.
///
/// _Lemma C_: After invoking `epoch.next_epoch()`, all worker threads would eventually check its
/// local queue.
///
/// Proof: According to Lemma B, we only need to consider those running worker threads when
/// invoking `epoch.next_epoch()`. (1) If a thread is between `L5` and `L6`, it would failed to
/// enter blocking state, so that it would eventually check its local queue. (2) If a thread is not
/// between `L5` and `L6`, it would eventually check its local queue.
///
/// _Lemma D (no deadlock)_: When all threads are blocked on `L6`, all local queues is empty.
///
/// Proof: Assume when all threads are blocked on `L6` at t_0, and some local queues is not empty
/// at t_0, some task `task` is in some queue `q`. When `push(task)`, `epoch_next_epoch()` is
/// invoked at time t_1. So that after t_1, all worker threads would eventually check its local
/// queue. t_0 > t_1. When `q`'s owner thread checking its local queue, it would find `task` and
/// pop it. Contradiction!
///
/// Lemma D tells us when some local queues is not empty, some threads are running, so the system
/// could make progress (a.k.a no deadlock).
///
/// Please note that Lemma C is wrong when the `epoch` overflows to exactly same number before
/// `L6` (the proof's (1) would be wrong). This is avoided by checking time elapsed. If the
/// `old_instant` is `MAX_DURATION` earlier, the `wait_next_epoch()` method won't block. For a 23
/// bits unsigned integer `epoch` to overflow to same number, the `next_epoch()` needs to be invoked
/// 8388608 times, which at least requires 8388608 atomic `fetch_add(EPOCH)` operations.
/// Hopefully it should take longer than 1000us.
///
/// The key for this proof is that, the worker needs to guarantee rechecking local queue before try
/// to `wait_next_epoch()`. Optimization keeping this property could be applied without breaking the
/// proof.
#[repr(align(64))]
pub(crate) struct Epoch(AtomicU32);

unsafe impl Sync for Epoch {}

/// 9 bits unsigned integer for active count, Support up to (2^9 - 1) threads.
const ACTIVE_COUNT_BITS: u32 = 9;
/// The mask to get the active count.
const ACTIVE_COUNT_MASK: u32 = !(((!(0 as u32)) >> ACTIVE_COUNT_BITS) << ACTIVE_COUNT_BITS);
/// The '1' for epoch
const EPOCH: u32 = 1 << ACTIVE_COUNT_BITS;
/// Max safe duration without epoch counter overflow. Since the duration is measured using
/// `Instant`, which should be able to measure sub microsecond duration, it is set to 1000
/// microseconds here.
const MAX_DURATION: Duration = Duration::from_micros(1_000);

impl Epoch {
    /// Init with `n_threads` in active state. If `n_threads` is too large, panic.
    #[inline]
    pub(crate) fn new(n_threads: u32) -> Self {
        assert!(n_threads < EPOCH);
        Self(AtomicU32::new(n_threads))
    }

    /// Return `Instant::now()`. Should be called before `get_status()` to measure how old is the
    /// status when checking status in `wait_next_epoch()`.
    #[inline]
    pub(crate) fn get_instant(&self) -> Instant {
        Instant::now()
    }

    /// Return a might-not-so-up-to-date status. Should be used in combination with `get_instant()`.
    #[inline]
    pub(crate) fn get_status(&self) -> u32 { self.0.load(SeqCst) }

    /// Set current worker thread to active state, and return the status after this operation.
    /// Should only be called from a currently inactive worker thread.
    #[inline]
    pub(crate) fn set_active(&self) -> u32 {
        let status = self.0.fetch_add(1, SeqCst);
        if Epoch::active_count(status) == 0 { self.wake_all_slow(); };
        status + 1
    }

    /// Set current worker thread to inactive state, and return the status after this operation.
    /// Should only be called from a currently active worker thread.
    #[inline]
    pub(crate) fn set_inactive(&self) -> u32 {
        self.0.fetch_sub(1, SeqCst) - 1
    }

    /// Wait for next epoch. It would return immediately when the `old_instant` is older than
    /// `MAX_DURATION` before. Should only be called when `active_count(old_status) == 0`.
    #[inline]
    pub(crate) fn wait_next_epoch(&self, old_status: u32, old_instant: Instant) {
        if self.0.load(SeqCst) == old_status { self.wait_slow(old_status, old_instant); };
    }

    /// Notify all workers that next epoch has arrived. Should be called whenever new tasks are
    /// pushed to local queues.
    #[inline]
    pub(crate) fn next_epoch(&self) {
        let status = self.0.fetch_add(EPOCH, SeqCst);
        if Epoch::active_count(status) == 0 { self.wake_all_slow(); };
    }

    /// Get number of active worker threads from `status` returned by `get_status()`,
    /// `set_active()`, and `set_inactive()`.
    #[inline]
    pub(crate) fn active_count(status: u32) -> u32 {
        status & ACTIVE_COUNT_MASK
    }

    /// Get the epoch number from `status` returned by `get_status()`, `set_active()`, and
    /// `set_inactive()`.
    #[inline]
    pub(crate) fn epoch(status: u32) -> u32 {
        status >> ACTIVE_COUNT_BITS
    }
}

impl Epoch {
    /// Using `parking_lot::park()` to park current worker thread. When doing validation inside the
    /// parking lot, if both (1) Time elapsed since `old_instant` does not exceed `MAX_DURATION` and
    /// (2) Current status is still `old_status`, park current worker thread, if not, return
    /// immediately. Since this is the slow path, no need to inline.
    fn wait_slow(&self, old_status: u32, old_instant: Instant) {
        let key = &self.0 as *const AtomicU32 as usize;
        let validate = move || {
            unsafe {
                let ptr = key as *const AtomicU32;
                let result = (*ptr).compare_exchange_weak(old_status, old_status, SeqCst, Relaxed);
                if result.is_err() { return false; };
                if old_instant.elapsed() > MAX_DURATION { return false; };
                return true;
            }
        };
        unsafe { park(key, validate, || {}, |_, _| {}, ParkToken(0), None) };
    }

    /// Wake up all worker threads parked in the parking lot by calling `parking_lot::unpark_all()`.
    /// Since this is the slow path, no need to inline.
    fn wake_all_slow(&self) {
        let key = &self.0 as *const AtomicU32 as usize;
        unsafe { unpark_all(key, UnparkToken(0)) };
    }
}
