//! Trap and Interrupt handling subsystem.

use core::sync::atomic::{AtomicU32, Ordering};

/// Master clock counter incremented on every timer interrupt.
pub static TICK_COUNT: AtomicU32 = AtomicU32::new(0);

/// Measured clock cycle count overhead for entry context preservation.
pub static METRIC_TRAP_LATENCY_CYCLES: AtomicU32 = AtomicU32::new(0);

/// Core trap dispatcher called from assembly when an exception or interrupt occurs.
///
/// # Safety
/// Called only from assembly (`trap_entry.s`) with a fully preserved register context.
#[no_mangle]
pub unsafe extern "C" fn trap_handler(mcause: usize, start_cycle: usize) {
    // Measure context-saving execution latency
    let end_cycle: usize;
    core::arch::asm!("csrr {}, mcycle", out(reg) end_cycle);
    let elapsed = end_cycle.wrapping_sub(start_cycle) as u32;
    METRIC_TRAP_LATENCY_CYCLES.store(elapsed, Ordering::Relaxed);

    const TIMER_INTERRUPT: usize = (1 << 31) | 7; // Machine-mode timer interrupt

    match mcause {
        TIMER_INTERRUPT => {
            let tick = TICK_COUNT.fetch_add(1, Ordering::Relaxed) + 1;

            // Re-arm timer (CLINT mtimecmp += interval)
            // 4,000,000 / 100 Hz = 40,000 cycles
            let clint_mtime = 0x0200_BFF8 as *const u64;
            let clint_mtimecmp = 0x0200_4000 as *mut u64;
            clint_mtimecmp.write_volatile(clint_mtime.read_volatile() + 40_000);

            // Log tick periodically to maintain JTAG throughput
            if tick % 10 == 0 {
                defmt::trace!("Tick: {} (Overhead: {} cycles)", tick, elapsed);
            }
        }
        cause => {
            defmt::error!("Unhandled exception. Cause register: 0x{:08X}", cause);
            let mepc: usize;
            core::arch::asm!("csrr {}, mepc", out(reg) mepc);
            defmt::error!("Instruction pointer (mepc): 0x{:08X}", mepc);
            panic!("unhandled exception");
        }
    }
}
