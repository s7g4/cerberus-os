//! Atomic Cross-Core Spinlock for SMP synchronization.
//!
//! Reentrant per-hart: a synchronous exception can fault *while a hart is
//! already inside the trap handler holding one of these locks* (that is
//! exactly the scenario the fault-injection suite exercises deliberately).
//! Without reentrancy that nested trap would spin forever trying to
//! re-acquire a lock its own hart already holds. Tracking the owning hart
//! and a hold depth makes re-entry from the same hart succeed instead of
//! deadlocking; a different hart still spins normally.
//!
//! The caller supplies its own hart id rather than the lock reading
//! `mhartid` itself: `mhartid` is a Machine-mode-only CSR, and these locks
//! are also taken from U-mode task code (e.g. the watchdog task), where
//! reading it would fault. M-mode callers (the trap handler) already have
//! their hart id from their own CSR read on entry; U-mode callers are
//! statically pinned to a known core by the scheduler layout and can pass
//! that fixed constant.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

/// A busy-waiting, per-hart-reentrant spinlock using hardware CAS.
pub struct Spinlock {
    /// 0 = unlocked, otherwise (owning hart id + 1).
    owner: AtomicUsize,
    /// Recursive hold count. Only ever touched while `owner` identifies the
    /// current hart, so plain (non-atomic) access is sound.
    depth: UnsafeCell<usize>,
}

// SAFETY: `depth` is only mutated by the hart that currently owns the lock,
// as established via the atomic `owner` field.
unsafe impl Sync for Spinlock {}

impl Spinlock {
    /// Create a new, unlocked spinlock.
    pub const fn new() -> Self {
        Self {
            owner: AtomicUsize::new(0),
            depth: UnsafeCell::new(0),
        }
    }
}

impl Default for Spinlock {
    fn default() -> Self {
        Self::new()
    }
}

impl Spinlock {
    /// Acquire the lock, busy-waiting if another hart holds it. If the
    /// calling hart already holds it, increments the hold depth and returns
    /// immediately instead of deadlocking against itself.
    ///
    /// `hart_id` must genuinely identify the calling hart.
    pub fn lock(&self, hart_id: usize) {
        let me = hart_id + 1;

        if self.owner.load(Ordering::Relaxed) == me {
            // SAFETY: `owner == me` can only be true if this same hart set
            // it, so only this hart touches `depth` right now.
            unsafe {
                *self.depth.get() += 1;
            }
            return;
        }

        while self
            .owner
            .compare_exchange_weak(0, me, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        unsafe {
            *self.depth.get() = 1;
        }
    }

    /// Release one level of the lock. Only clears ownership once the hold
    /// depth returns to zero.
    pub fn unlock(&self) {
        unsafe {
            let depth = self.depth.get();
            *depth -= 1;
            if *depth == 0 {
                self.owner.store(0, Ordering::Release);
            }
        }
    }

    /// Busy-wait until the lock is acquired, returning a guard that releases
    /// the lock on drop. Prevents leaking the lock on an early return.
    pub fn lock_guard(&self, hart_id: usize) -> SpinlockGuard<'_> {
        self.lock(hart_id);
        SpinlockGuard { lock: self }
    }
}

/// RAII guard returned by [`Spinlock::lock_guard`]. Releases the lock when dropped.
pub struct SpinlockGuard<'a> {
    lock: &'a Spinlock,
}

impl Drop for SpinlockGuard<'_> {
    fn drop(&mut self) {
        self.lock.unlock();
    }
}
