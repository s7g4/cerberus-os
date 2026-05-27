//! Cerberus-OS — Kernel entry point.

#![no_std]
#![no_main]
#![feature(naked_functions)]

use defmt_rtt as _;

mod scheduler;
mod trap;
mod memory;
mod network;

use scheduler::{BitMapScheduler, TaskControlBlock, TaskState};

// Link the low-level assembly trap handler
core::arch::global_asm!(include_str!("trap_entry.s"));

extern "C" {
    fn _trap_entry();
}

// Global scheduler instance
#[no_mangle]
pub static mut SCHEDULER: BitMapScheduler = BitMapScheduler::new();

// Static buffers for task stacks (allocated statically, no heap)
static mut TASK_A_STACK: [u8; 1024] = [0; 1024];
static mut TASK_B_STACK: [u8; 1024] = [0; 1024];

#[riscv_rt::entry]
fn main() -> ! {
    kmain();
}

/// Core kernel entry routine.
pub fn kmain() -> ! {
    defmt::info!("Booting Cerberus-OS kernel...");

    unsafe {
        // Point the Machine Trap Vector (mtvec) register to the assembly entry.
        let trap_addr = _trap_entry as usize;
        core::arch::asm!("csrw mtvec, {}", in(reg) trap_addr);

        // Initialize PMP boundaries (W^X rules)
        init_memory_protection();

        // Initialize task contexts on their respective stacks
        let sp_a = TaskControlBlock::initialize_stack(&mut *core::ptr::addr_of_mut!(TASK_A_STACK), task_a);
        let sp_b = TaskControlBlock::initialize_stack(&mut *core::ptr::addr_of_mut!(TASK_B_STACK), task_b);

        let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);

        // Register tasks inside the scheduler
        sched.register_task(TaskControlBlock {
            saved_sp: sp_a,
            priority: 1, // Higher priority (lower numerical value)
            state: TaskState::Ready,
            name: "Task A",
        });

        sched.register_task(TaskControlBlock {
            saved_sp: sp_b,
            priority: 2,
            state: TaskState::Ready,
            name: "Task B",
        });

        // Configure timer tick interrupts
        init_timer();

        defmt::info!("Heartbeat timer armed. Launching first task...");

        // Start execution of the highest priority task
        sched.start_first_task();
    }
}

/// Configures hardware-level memory boundaries.
unsafe fn init_memory_protection() {
    use memory::{configure_pmp, PmpAddressMode, PmpConfig};
    // Region 0: Flash/Code execution boundary (Read + Execute only, Locked)
    // Base: 0x4200_0000, Size: 4MB (using NAPOT address mode)
    configure_pmp(
        0,
        0x4200_0000,
        4 * 1024 * 1024,
        PmpConfig {
            read: true,
            write: false,
            execute: true,
            mode: PmpAddressMode::Napot,
            locked: true,
        },
    );
    // Region 1: SRAM/Data RAM boundary (Read + Write only, Locked - Prevents execution from RAM)
    // Base: 0x3FC8_0000, Size: 512KB
    configure_pmp(
        1,
        0x3FC8_0000,
        512 * 1024,
        PmpConfig {
            read: true,
            write: true,
            execute: false,
            mode: PmpAddressMode::Napot,
            locked: true,
        },
    );
}

/// Task A Entry point
extern "C" fn task_a() -> ! {
    loop {
        defmt::info!("Task A is active");
        // Busy loop representing operational work
        for _ in 0..50_000 {
            unsafe { core::arch::asm!("nop") };
        }
    }
}

/// Task B Entry point
extern "C" fn task_b() -> ! {
    loop {
        defmt::info!("Task B is active");
        // Busy loop representing operational work
        for _ in 0..50_000 {
            unsafe { core::arch::asm!("nop") };
        }
    }
}

/// Configure the Machine-mode Timer.
unsafe fn init_timer() {
    let clint_mtime    = 0x0200_BFF8 as *const u64;
    let clint_mtimecmp = 0x0200_4000 as *mut u64;
    clint_mtimecmp.write_volatile(clint_mtime.read_volatile() + 40_000);

    let mie: usize;
    core::arch::asm!("csrr {}, mie", out(reg) mie);
    core::arch::asm!("csrw mie, {}", in(reg) mie | (1 << 7));

    let mstatus: usize;
    core::arch::asm!("csrr {}, mstatus", out(reg) mstatus);
    core::arch::asm!("csrw mstatus, {}", in(reg) mstatus | (1 << 3));
}

/// Global panic handler for bare-metal execution.
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
