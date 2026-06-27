//! User-Mode System Call wrappers using RISC-V ECALL.

/// Syscall wrapper to trigger cooperative yields from User Mode.
#[inline(always)]
pub fn yield_now() {
    #[cfg(not(kani))]
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 1usize,
        );
    }
}

/// Syscall wrapper to sleep for a specific number of ticks.
#[inline(always)]
pub fn sleep_ticks(ticks: usize) {
    let _ = ticks;
    #[cfg(not(kani))]
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 2usize,
            in("a0") ticks,
        );
    }
}

/// Syscall wrapper to lock a Mutex.
#[inline(always)]
pub fn lock_mutex(idx: usize) {
    let _ = idx;
    #[cfg(not(kani))]
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 3usize,
            in("a0") idx,
        );
    }
}

/// Syscall wrapper to unlock a Mutex.
#[inline(always)]
pub fn unlock_mutex(idx: usize) {
    let _ = idx;
    #[cfg(not(kani))]
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 4usize,
            in("a0") idx,
        );
    }
}

/// Syscall wrapper to check in with the Watchdog.
#[inline(always)]
pub fn watchdog_checkin() {
    #[cfg(not(kani))]
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 5usize,
        );
    }
}

/// Syscall wrapper to send an IPC message (synchronous rendezvous).
#[inline(always)]
pub fn sys_send(cap_idx: usize, msg: &[u8]) -> isize {
    #[cfg(not(kani))]
    {
        let ret: isize;
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") 6usize,
                inout("a0") cap_idx => ret,
                in("a1") msg.as_ptr() as usize,
                in("a2") msg.len(),
            );
        }
        ret
    }
    #[cfg(kani)]
    {
        let _ = cap_idx;
        let _ = msg;
        0
    }
}

/// Syscall wrapper to receive an IPC message (synchronous rendezvous).
#[inline(always)]
pub fn sys_recv(cap_idx: usize, buf: &mut [u8]) -> isize {
    #[cfg(not(kani))]
    {
        let ret: isize;
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") 7usize,
                inout("a0") cap_idx => ret,
                in("a1") buf.as_mut_ptr() as usize,
                in("a2") buf.len(),
            );
        }
        ret
    }
    #[cfg(kani)]
    {
        let _ = cap_idx;
        let _ = buf;
        0
    }
}

/// Syscall wrapper to terminate a task by priority.
#[inline(always)]
pub fn sys_terminate_task(prio: usize) -> isize {
    #[cfg(not(kani))]
    {
        let ret: isize;
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") 8usize,
                inout("a0") prio => ret,
            );
        }
        ret
    }
    #[cfg(kani)]
    {
        let _ = prio;
        0
    }
}
