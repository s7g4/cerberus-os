//! Cerberus-OS — Kernel entry point.
//!
//! Handles low-level target initialization, registers the interrupt trap vector,
//! and configures the hardware system timer triggers.

#![no_std]
#![no_main]
#![feature(naked_functions)]

use defmt_rtt as _; // Route logs over the JTAG RTT interface

mod trap;

// Link the low-level assembly trap handler
core::arch::global_asm!(include_str!("trap_entry.s"));

extern "C" {
    // Low-level trap entry point defined in trap_entry.s
    fn _trap_entry();
}

#[riscv_rt::entry]
fn main() -> ! {
    kmain();
}

/// Core kernel entry routine.
pub fn kmain() -> ! {
    defmt::info!("Booting Cerberus-OS kernel...");

    unsafe {
        // Point the Machine Trap Vector (mtvec) register to the assembly entry.
        // Direct mode: low 2 bits are 00 (all traps jump directly to _trap_entry).
        let trap_addr = _trap_entry as usize;
        core::arch::asm!("csrw mtvec, {}", in(reg) trap_addr);

        // Arm timer tick interrupts.
        init_timer();
    }

    defmt::info!("System clock initialized. Global interrupts enabled.");

    loop {
        // Halt CPU core until next interrupt fires (power saving mode)
        unsafe { core::arch::asm!("wfi") };
    }
}

/// Configures and arms the Machine-mode Timer.
unsafe fn init_timer() {
    // Set first trigger threshold in memory-mapped mtimecmp.
    // Interval: 4MHz clock frequency / 100Hz tick rate = 40,000 cycles.
    let clint_mtime = 0x0200_BFF8 as *const u64;
    let clint_mtimecmp = 0x0200_4000 as *mut u64;
    clint_mtimecmp.write_volatile(clint_mtime.read_volatile() + 40_000);

    // Enable Machine Timer Interrupts (MTIE - bit 7) in the mie register
    let mie: usize;
    core::arch::asm!("csrr {}, mie", out(reg) mie);
    core::arch::asm!("csrw mie, {}", in(reg) mie | (1 << 7));

    // Enable Global Interrupts (MIE - bit 3) in the mstatus register
    let mstatus: usize;
    core::arch::asm!("csrr {}, mstatus", out(reg) mstatus);
    core::arch::asm!("csrw mstatus, {}", in(reg) mstatus | (1 << 3));
}

/// Global panic handler for bare-metal execution.
///
/// Log details to JTAG RTT and freeze CPU to prevent undefined operations.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    defmt::error!("CRITICAL: Kernel Panic.");
    if let Some(location) = info.location() {
        defmt::error!(
            "Source: {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
    }
    defmt::error!("Details: {}", defmt::Debug2Format(info));
    loop {
        unsafe { core::arch::asm!("wfi") };
    }
}
