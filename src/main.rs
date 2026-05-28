//! Cerberus-OS — Kernel entry point.

#![no_std]
#![no_main]
#![feature(naked_functions)]

use defmt_rtt as _;

mod kernel;
mod memory;
mod network;
mod scheduler;
mod security;
mod trap;

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
pub static mut TASK_A_STACK: [u8; 1024] = [0; 1024];
pub static mut TASK_B_STACK: [u8; 1024] = [0; 1024];
pub static mut TASK_C_STACK: [u8; 1024] = [0; 1024];

// Dedicated Kernel Interrupt Stack (M-mode execution)
pub static mut KERNEL_STACK: [u8; 1024] = [0; 1024];

// Kernel Mutex representation
pub struct KernelMutex {
    pub locked: bool,
    pub owner_task_idx: Option<u8>,
    pub waiters_bitmap: u32,
}

// Static array of Mutex locks
#[no_mangle]
pub static mut MUTEXES: [Option<KernelMutex>; 8] = [
    Some(KernelMutex {
        locked: false,
        owner_task_idx: None,
        waiters_bitmap: 0,
    }),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
];

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

        // Initialize PMP boundaries (Priority Masking sandboxing rules)
        init_memory_protection();

        // Run cryptographic CAN stack self-test on boot
        verify_network_and_security();

        // Initialize task contexts on their respective stacks
        let sp_a =
            TaskControlBlock::initialize_stack(&mut *core::ptr::addr_of_mut!(TASK_A_STACK), task_a);
        let sp_b =
            TaskControlBlock::initialize_stack(&mut *core::ptr::addr_of_mut!(TASK_B_STACK), task_b);
        let sp_c =
            TaskControlBlock::initialize_stack(&mut *core::ptr::addr_of_mut!(TASK_C_STACK), task_c);

        let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);

        // Register tasks inside the scheduler
        sched.register_task(TaskControlBlock {
            saved_sp: sp_a,
            priority: 1, // High priority
            active_priority: 1,
            state: TaskState::Ready,
            name: "Task A",
        });

        sched.register_task(TaskControlBlock {
            saved_sp: sp_b,
            priority: 2, // Medium priority
            active_priority: 2,
            state: TaskState::Ready,
            name: "Task B",
        });

        sched.register_task(TaskControlBlock {
            saved_sp: sp_c,
            priority: 3, // Low priority
            active_priority: 3,
            state: TaskState::Ready,
            name: "Task C",
        });

        // Configure timer tick interrupts
        init_timer();

        defmt::info!("Heartbeat timer armed. Launching first task...");

        // Start execution of the highest priority task
        sched.start_first_task();
    }
}

/// Verification routine to test the CAN parser, ring buffer, and HMAC authentication on boot.
fn verify_network_and_security() {
    use core::sync::atomic::Ordering;
    use kernel::metrics::{
        METRIC_CAN_PARSE_CYCLES, METRIC_CAN_QUEUE_CYCLES, METRIC_FRAMES_DROPPED, METRIC_FRAMES_RX,
        METRIC_HMAC_FAILURES, METRIC_HMAC_VERIFY_CYCLES,
    };
    use network::{CanError, CanFrame, CanRingBuffer};
    use security::{compute_hmac, verify_frame, AuthFrame};

    defmt::info!("Executing network & cryptographic self-test...");

    // 1. Simulate receiving a raw 13-byte transceiver frame
    let mut raw_frame = [0u8; 13];
    raw_frame[0] = 0x3E;
    raw_frame[1] = 0x00;
    raw_frame[2] = 4; // DLC
    raw_frame[3] = 0xAA;
    raw_frame[4] = 0xBB;
    raw_frame[5] = 0xCC;
    raw_frame[6] = 0xDD;

    // Parse raw buffer and measure cycle latency
    let start_parse = read_cycles();
    let parsed = CanFrame::parse(&raw_frame);
    let end_parse = read_cycles();
    METRIC_CAN_PARSE_CYCLES.store(end_parse.wrapping_sub(start_parse), Ordering::Relaxed);

    match parsed {
        Ok(frame) => {
            defmt::info!(
                "CAN Parser: Valid frame parsed. ID=0x{:03X}, DLC={}",
                frame.id,
                frame.dlc
            );
            METRIC_FRAMES_RX.fetch_add(1, Ordering::Relaxed);

            // 2. Cryptographic signature generation and verification latency measurement
            let start_crypto = read_cycles();
            let tag = compute_hmac(&frame);
            let auth = AuthFrame { frame, tag };
            let verified = verify_frame(&auth);
            let end_crypto = read_cycles();
            METRIC_HMAC_VERIFY_CYCLES
                .store(end_crypto.wrapping_sub(start_crypto), Ordering::Relaxed);

            defmt::info!("HMAC Sign: Generated tag = {:?}", tag);
            if verified {
                defmt::info!("HMAC Verify: Signature verification passed.");
            } else {
                METRIC_HMAC_FAILURES.fetch_add(1, Ordering::Relaxed);
                defmt::error!("HMAC Verify: Verification failed!");
            }

            // 3. Queue frame into lock-free ring buffer and measure SPSC queue operations latency
            let mut can_queue = CanRingBuffer::new();
            let start_queue = read_cycles();
            let push_ok = can_queue.push(frame).is_ok();
            let popped = if push_ok { can_queue.pop() } else { None };
            let end_queue = read_cycles();
            METRIC_CAN_QUEUE_CYCLES.store(end_queue.wrapping_sub(start_queue), Ordering::Relaxed);

            if push_ok {
                defmt::info!("Ring Buffer: Successfully pushed frame to SPSC queue.");
                if let Some(p) = popped {
                    defmt::info!(
                        "Ring Buffer: Popped frame from SPSC queue. ID=0x{:03X}",
                        p.id
                    );
                }
            }
        }
        Err(CanError::BlockedId(id)) => {
            METRIC_FRAMES_DROPPED.fetch_add(1, Ordering::Relaxed);
            defmt::warn!("CAN Parser: Blocked malicious frame ID=0x{:03X}", id);
        }
        Err(CanError::InvalidDlc(dlc)) => {
            defmt::error!("CAN Parser: Frame rejected. Invalid DLC={}", dlc);
        }
        Err(CanError::BufferFull) => {
            defmt::error!("CAN Buffer: Push failed. Queue full.");
        }
    }
}

/// Configures hardware-level memory boundaries using PMP priority masking.
unsafe fn init_memory_protection() {
    use memory::{configure_pmp, PmpAddressMode, PmpConfig};

    // Entry 0: Flash/Code execution boundary (Read + Execute only, Locked)
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

    // Entry 1: Inactive Stack 1 (No Access, Unlocked)
    configure_pmp(
        1,
        core::ptr::addr_of!(TASK_B_STACK) as usize,
        1024,
        PmpConfig {
            read: false,
            write: false,
            execute: false,
            mode: PmpAddressMode::Napot,
            locked: false,
        },
    );

    // Entry 2: Inactive Stack 2 (No Access, Unlocked)
    configure_pmp(
        2,
        core::ptr::addr_of!(TASK_C_STACK) as usize,
        1024,
        PmpConfig {
            read: false,
            write: false,
            execute: false,
            mode: PmpAddressMode::Napot,
            locked: false,
        },
    );

    // Entry 3: SRAM/Data RAM boundary (Read + Write only, Unlocked)
    configure_pmp(
        3,
        0x3FC8_0000,
        512 * 1024,
        PmpConfig {
            read: true,
            write: true,
            execute: false,
            mode: PmpAddressMode::Napot,
            locked: false,
        },
    );
}

/// Assembly cycle probe helper.
#[inline(always)]
fn read_cycles() -> u32 {
    let cycles: u32;
    unsafe {
        core::arch::asm!("csrr {0}, mcycle", out(reg) cycles);
    }
    cycles
}

/// Syscall wrapper to trigger cooperative yields from User Mode.
fn yield_now() {
    unsafe {
        core::arch::asm!("li a7, 1", "ecall");
    }
}

/// Syscall wrapper to lock a Mutex.
fn lock_mutex(idx: usize) {
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
fn unlock_mutex(idx: usize) {
    unsafe {
        core::arch::asm!(
            "li a7, 4",
            "mv a0, {}",
            "ecall",
            in(reg) idx
        );
    }
}

/// Task A Entry point (High Priority)
extern "C" fn task_a() -> ! {
    let mut loop_count = 0u32;
    loop {
        defmt::info!("Task A (High) loop starting. Trying to lock Mutex 0...");
        lock_mutex(0);
        defmt::info!("Task A (High) locked Mutex 0 successfully.");

        // Small busy work
        for _ in 0..10_000 {
            unsafe { core::arch::asm!("nop") };
        }

        defmt::info!("Task A (High) releasing Mutex 0...");
        unlock_mutex(0);
        defmt::info!("Task A (High) released Mutex 0.");

        loop_count = loop_count.wrapping_add(1);
        if loop_count % 10 == 0 {
            kernel::dump_metrics();
        }

        yield_now();
    }
}

/// Task B Entry point (Medium Priority)
extern "C" fn task_b() -> ! {
    loop {
        defmt::info!("Task B (Medium) is active. Running heavy loop...");
        // Large compute loop. Without PIP, this would starve Task C and block Task A forever.
        for _ in 0..80_000 {
            unsafe { core::arch::asm!("nop") };
        }
        defmt::info!("Task B (Medium) yielding.");
        yield_now();
    }
}

/// Task C Entry point (Low Priority)
extern "C" fn task_c() -> ! {
    loop {
        defmt::info!("Task C (Low) starting. Locking Mutex 0...");
        lock_mutex(0);
        defmt::info!("Task C (Low) acquired Mutex 0. Yielding to let high priority run...");

        yield_now(); // Yield to let Task A preempt us and block on Mutex 0

        defmt::info!("Task C (Low) resumed. Releasing Mutex 0...");
        unlock_mutex(0);
        defmt::info!("Task C (Low) released Mutex 0. Yielding.");

        yield_now();
    }
}

/// Configure the Machine-mode Timer.
unsafe fn init_timer() {
    let clint_mtime = 0x0200_BFF8 as *const u64;
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
