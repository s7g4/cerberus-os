//! Atomic Cross-Core Spinlock for SMP synchronization.

use core::sync::atomic::{AtomicBool, Ordering};

/// A basic busy-waiting spinlock using hardware-supported atomic compare-and-swap.
pub struct Spinlock {
    locked: AtomicBool,
}

impl Spinlock {
    /// Create a new, unlocked spinlock.
    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
        }
    }
}

impl Default for Spinlock {
    fn default() -> Self {
        Self::new()
    }
}

impl Spinlock {
    /// Busy-wait until the lock is acquired.
    pub fn lock(&self) {
        // Attempt to atomically swap false to true. If it fails, spin-wait.
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }

    /// Release the lock.
    pub fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}
