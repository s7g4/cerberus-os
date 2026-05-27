//! Cerberus-OS — kernel entry point.
//!
//! ## Execution model
//!
//! On reset, the CPU begins executing at the address specified by the
//! linker script's ENTRY symbol (here: `_start` in `link.x`). Control
//! flows to `_start` (assembly stub) which initialises the stack pointer
//! and calls `main` (which we map via `#[riscv_rt::entry]`).
//! There is no OS loader, no ELF interpreter — the binary image IS the running program.

#![no_std]
#![no_main]
#![feature(naked_functions)] // Required for Phase 2 context switcher
#![feature(asm_const)] // Required for inline assembly constants

use defmt_rtt as _; // RTT transport — routes defmt logs over JTAG

#[riscv_rt::entry]
fn main() -> ! {
    kmain();
}

pub fn kmain() -> ! {
    // Phase 1 completion proof: system is alive and observable
    defmt::info!("Cerberus-OS kernel booting — phase 1 alive");
    defmt::info!(
        "Build: {} {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    // This halt loop is the placeholder for the scheduler (Phase 3)
    loop {
        // RISC-V WFI: Wait For Interrupt — CPU halts until next IRQ
        unsafe { core::arch::asm!("wfi") };
    }
}

/// Custom bare-metal panic handler.
///
/// On panic, we print the file, line, and payload details over RTT,
/// and then put the processor into an infinite Wait-For-Interrupt low power sleep.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    defmt::error!("!!! KERNEL PANIC !!!");

    // Print location if available
    if let Some(location) = info.location() {
        defmt::error!(
            "Location: {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
    }

    // Print panic payload/message
    defmt::error!("Payload: {}", defmt::Debug2Format(info));

    // Safely freeze the CPU
    loop {
        unsafe { core::arch::asm!("wfi") };
    }
}
