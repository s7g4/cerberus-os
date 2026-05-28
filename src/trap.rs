//! Trap and Interrupt handling subsystem.

use core::sync::atomic::{AtomicU32, Ordering};

/// Master clock counter incremented on every timer interrupt.
pub static TICK_COUNT: AtomicU32 = AtomicU32::new(0);

/// Measured clock cycle count overhead for entry context preservation.
pub static METRIC_TRAP_LATENCY_CYCLES: AtomicU32 = AtomicU32::new(0);

/// Core trap dispatcher called from assembly when an exception or interrupt occurs.
///
/// Returns the stack pointer of the next task to run.
#[no_mangle]
pub unsafe extern "C" fn trap_handler(mcause: usize, user_sp: usize, start_cycle: usize) -> usize {
    // Measure context-saving execution latency
    let end_cycle: usize;
    core::arch::asm!("csrr {}, mcycle", out(reg) end_cycle);
    let elapsed = end_cycle.wrapping_sub(start_cycle) as u32;
    METRIC_TRAP_LATENCY_CYCLES.store(elapsed, Ordering::Relaxed);

    const TIMER_INTERRUPT: usize = (1 << 31) | 7; // Machine-mode timer interrupt
    const ECALL_UMODE: usize = 8; // Environment call from U-mode

    let mut current_sp = user_sp;

    match mcause {
        TIMER_INTERRUPT => {
            let tick = TICK_COUNT.fetch_add(1, Ordering::Relaxed) + 1;

            // Re-arm timer (CLINT mtimecmp += interval)
            let clint_mtime = 0x0200_BFF8 as *const u64;
            let clint_mtimecmp = 0x0200_4000 as *mut u64;
            clint_mtimecmp.write_volatile(clint_mtime.read_volatile() + 40_000);

            // Log tick periodically
            if tick % 10 == 0 {
                defmt::trace!("Tick: {} (Overhead: {} cycles)", tick, elapsed);
            }

            // Perform context switch if a different task is ready
            extern "Rust" {
                static mut SCHEDULER: crate::scheduler::bitmap::BitMapScheduler;
            }
            let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
            if let Some((old_sp_ptr, new_sp)) = sched.schedule() {
                // Save the stack pointer of the outgoing task
                old_sp_ptr.write_volatile(current_sp);
                // Load the new stack pointer
                current_sp = new_sp;

                // Reprogram PMP stack sandboxing rules for the incoming task
                if let Some(prio) = sched.current_priority {
                    if let Some(new_tcb) = &sched.task_table[prio as usize] {
                        crate::memory::reprogram_pmp_stack(new_tcb.name);
                    }
                }
            }
        }
        ECALL_UMODE => {
            // Read registers from user stack frame
            // a7 (syscall ID) is at index 13 (offset 52 bytes)
            let frame = current_sp as *mut usize;
            let syscall_id = frame.add(13).read_volatile();

            // Advance PC past the ecall instruction (2 bytes in C extension, 4 bytes standard)
            // Since we target riscv32imac (compressed instructions active), ecall is 4 bytes.
            let mepc = frame.add(28).read_volatile(); // mepc is index 28 (offset 112)
            frame.add(28).write_volatile(mepc + 4);

            match syscall_id {
                1 => {
                    // Cooperative Yield Syscall
                    extern "Rust" {
                        static mut SCHEDULER: crate::scheduler::bitmap::BitMapScheduler;
                    }
                    let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
                    if let Some((old_sp_ptr, new_sp)) = sched.schedule() {
                        old_sp_ptr.write_volatile(current_sp);
                        current_sp = new_sp;

                        if let Some(prio) = sched.current_priority {
                            if let Some(new_tcb) = &sched.task_table[prio as usize] {
                                crate::memory::reprogram_pmp_stack(new_tcb.name);
                            }
                        }
                    }
                }
                _ => {
                    defmt::warn!("Unhandled syscall ID: {}", syscall_id);
                }
            }
        }
        cause => {
            if cause == 1 || cause == 5 || cause == 7 {
                crate::kernel::metrics::METRIC_PMP_VIOLATIONS.fetch_add(1, Ordering::Relaxed);
            }
            defmt::error!("Unhandled exception. Cause register: 0x{:08X}", cause);
            let frame = current_sp as *const usize;
            let mepc = frame.add(28).read_volatile();
            defmt::error!("Instruction pointer (mepc): 0x{:08X}", mepc);
            panic!("unhandled exception");
        }
    }

    current_sp
}
