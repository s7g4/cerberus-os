//! User-Mode System Call wrappers using RISC-V ECALL.

/// Syscall wrapper to trigger cooperative yields from User Mode.
#[inline(always)]
pub fn yield_now() {
    #[cfg(not(kani))]
    unsafe {
        core::arch::asm!("li a7, 1", "ecall");
    }
}

/// Syscall wrapper to sleep for a specific number of ticks.
#[inline(always)]
pub fn sleep_ticks(ticks: usize) {
    let _ = ticks;
    #[cfg(not(kani))]
    unsafe {
        core::arch::asm!(
            "li a7, 2",
            "mv a0, {}",
            "ecall",
            in(reg) ticks
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
            "li a7, 3",
            "mv a0, {}",
            "ecall",
            in(reg) idx
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
            "li a7, 4",
            "mv a0, {}",
            "ecall",
            in(reg) idx
        );
    }
}

/// Syscall wrapper to check in with the Watchdog.
#[inline(always)]
pub fn watchdog_checkin() {
    #[cfg(not(kani))]
    unsafe {
        core::arch::asm!("li a7, 5", "ecall");
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
                "li a7, 6",
                "mv a0, {0}",
                "mv a1, {1}",
                "mv a2, {2}",
                "ecall",
                in(reg) cap_idx,
                in(reg) msg.as_ptr() as usize,
                in(reg) msg.len(),
                lateout("a0") ret
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
                "li a7, 7",
                "mv a0, {0}",
                "mv a1, {1}",
                "mv a2, {2}",
                "ecall",
                in(reg) cap_idx,
                in(reg) buf.as_mut_ptr() as usize,
                in(reg) buf.len(),
                lateout("a0") ret
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
