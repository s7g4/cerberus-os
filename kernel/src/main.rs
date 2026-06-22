//! Cerberus-OS — Kernel entry point.

#![no_std]
#![no_main]
#![feature(naked_functions)]
#![allow(unused_variables, dead_code, stable_features)]

use defmt_rtt as _;

mod kernel;
mod memory;
mod trap;

use scheduler::{BitMapScheduler, Capability, TaskControlBlock, TaskState};

// Link the low-level assembly trap handler
#[cfg(not(kani))]
core::arch::global_asm!(include_str!("trap_entry.s"));

extern "C" {
    fn _trap_entry();
}

use core::sync::atomic::{AtomicBool, Ordering};

// Global schedulers instance (per core)
#[no_mangle]
pub static mut SCHEDULERS: [BitMapScheduler; 2] = [BitMapScheduler::new(), BitMapScheduler::new()];

// Static buffers for task stacks (allocated statically, no heap)
pub static mut TASK_A_STACK: [u8; 1024] = [0; 1024];
pub static mut TASK_B_STACK: [u8; 1024] = [0; 1024];
pub static mut TASK_C_STACK: [u8; 1024] = [0; 1024];
pub static mut TASK_WD_STACK: [u8; 1024] = [0; 1024];

// Dedicated Kernel Interrupt Stacks (M-mode execution per core)
pub static mut KERNEL_STACK_0: [u8; 1024] = [0; 1024];
pub static mut KERNEL_STACK_1: [u8; 1024] = [0; 1024];
pub static mut HSM_STACK: [u8; 1024] = [0; 1024];

// Synchronization primitives for SMP execution
pub static BOOT_BARRIER: AtomicBool = AtomicBool::new(false);
#[no_mangle]
pub static MUTEX_LOCK: kernel::Spinlock = kernel::Spinlock::new();

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

pub trait StartFirstTaskExt {
    fn start_first_task(&mut self, hart_id: usize) -> !;
}

impl StartFirstTaskExt for BitMapScheduler {
    fn start_first_task(&mut self, hart_id: usize) -> ! {
        let mut first_idx = 0;
        let mut found = false;
        for i in 0..scheduler::bitmap::MAX_PARTITIONS {
            if let Some(tcb) = &self.task_table[i] {
                if tcb.state == TaskState::Ready {
                    first_idx = i;
                    found = true;
                    break;
                }
            }
        }
        assert!(found, "No ready tasks registered at boot");

        self.current_partition_idx = first_idx;
        self.remaining_mif_ticks = self.partition_durations[first_idx];

        let new_tcb = self.task_table[first_idx].as_mut().unwrap();
        new_tcb.state = TaskState::Running;

        let user_sp = new_tcb.saved_sp;
        let task_name = new_tcb.name;

        #[cfg(not(kani))]
        unsafe {
            // 1. Set PMP isolation to block the inactive task stacks
            crate::memory::reprogram_pmp_stack(task_name);

            // 2. Point mscratch to the top of our dedicated Kernel Stack
            let kernel_stack_top = if hart_id == 0 {
                core::ptr::addr_of_mut!(crate::KERNEL_STACK_0) as usize + 1024
            } else {
                core::ptr::addr_of_mut!(crate::KERNEL_STACK_1) as usize + 1024
            };
            core::arch::asm!("csrw mscratch, {}", in(reg) kernel_stack_top);

            // 3. Load user stack pointer, restore U-mode registers, and execute mret
            core::arch::asm!(
                "mv sp, {0}",
                "lw t0, 112(sp)",
                "csrw mepc, t0",
                "lw t1, 116(sp)",
                "csrw mstatus, t1",
                "lw ra, 0(sp)",
                "lw t0, 4(sp)",
                "lw t1, 8(sp)",
                "lw t2, 12(sp)",
                "lw s0, 16(sp)",
                "lw s1, 20(sp)",
                "lw a0, 24(sp)",
                "lw a1, 28(sp)",
                "lw a2, 32(sp)",
                "lw a3, 36(sp)",
                "lw a4, 40(sp)",
                "lw a5, 44(sp)",
                "lw a6, 48(sp)",
                "lw a7, 52(sp)",
                "lw s2, 56(sp)",
                "lw s3, 60(sp)",
                "lw s4, 64(sp)",
                "lw s5, 68(sp)",
                "lw s6, 72(sp)",
                "lw s7, 76(sp)",
                "lw s8, 80(sp)",
                "lw s9, 84(sp)",
                "lw s10, 88(sp)",
                "lw s11, 92(sp)",
                "lw t3, 96(sp)",
                "lw t4, 100(sp)",
                "lw t5, 104(sp)",
                "lw t6, 108(sp)",
                "addi sp, sp, 128",
                "mret",
                in(reg) user_sp,
                options(noreturn)
            );
        }
        #[cfg(kani)]
        {
            loop {}
        }
    }
}

#[riscv_rt::entry]
fn main() -> ! {
    kmain();
}

/// Core kernel entry routine.
pub fn kmain() -> ! {
    let hart_id: usize;
    unsafe {
        core::arch::asm!("csrr {}, mhartid", out(reg) hart_id);
    }

    if hart_id == 0 {
        defmt::info!("Booting Cerberus-OS kernel on Core 0...");

        unsafe {
            // Point the Machine Trap Vector (mtvec) register to the assembly entry.
            let trap_addr = _trap_entry as *const () as usize;
            core::arch::asm!("csrw mtvec, {}", in(reg) trap_addr);

            // Initialize PMP boundaries (Priority Masking sandboxing rules)
            init_memory_protection();

            // Run Secure Boot Loader (SBL) verification
            defmt::info!("SBL: Starting Secure Boot verification...");
            if security::verify_secure_boot() {
                defmt::info!("SBL: Secure Boot Verification SUCCESSFUL.");
            } else {
                defmt::error!("SBL: Secure Boot Verification FAILED! Halting system.");
                loop {
                    core::arch::asm!("wfi");
                }
            }

            // Verify SBL tampered detection
            defmt::info!("SBL: Testing tampered image detection...");
            if !security::verify_tampered_secure_boot() {
                defmt::info!("SBL: Tampered image successfully detected and rejected.");
            } else {
                defmt::error!(
                    "SBL: WARNING - Tampered image verification bypassed! Security failure!"
                );
            }

            // Run cryptographic CAN stack self-test on boot
            verify_network_and_security();

            // Initialize task contexts on their respective stacks
            let sp_wd = TaskControlBlock::initialize_stack(
                &mut *core::ptr::addr_of_mut!(TASK_WD_STACK),
                watchdog_task,
            );
            let sp_a = TaskControlBlock::initialize_stack(
                &mut *core::ptr::addr_of_mut!(TASK_A_STACK),
                task_a,
            );
            let sp_b = TaskControlBlock::initialize_stack(
                &mut *core::ptr::addr_of_mut!(TASK_B_STACK),
                task_b,
            );
            let sp_c = TaskControlBlock::initialize_stack(
                &mut *core::ptr::addr_of_mut!(TASK_C_STACK),
                task_c,
            );
            let sp_hsm = TaskControlBlock::initialize_stack(
                &mut *core::ptr::addr_of_mut!(HSM_STACK),
                security::hsm_task,
            );

            let scheds = &mut *core::ptr::addr_of_mut!(SCHEDULERS);

            // Core 0 Scheduler registration (Watchdog + Task A + HSM Task)
            scheds[0].register_task(TaskControlBlock {
                saved_sp: sp_wd,
                priority: 0,
                active_priority: 0,
                state: TaskState::Ready,
                name: "Watchdog",
                capabilities: [Capability::None; 8],
            });

            scheds[0].register_task(TaskControlBlock {
                saved_sp: sp_a,
                priority: 1,
                active_priority: 1,
                state: TaskState::Ready,
                name: "Task A",
                capabilities: [
                    Capability::Mutex {
                        mutex_idx: 0,
                        can_lock: true,
                        can_unlock: true,
                    },
                    Capability::Ipc {
                        endpoint_idx: 0,
                        can_send: true,
                        can_recv: false,
                    },
                    Capability::Ipc {
                        endpoint_idx: 2,
                        can_send: true,
                        can_recv: false,
                    },
                    Capability::Ipc {
                        endpoint_idx: 3,
                        can_send: false,
                        can_recv: true,
                    },
                    Capability::None,
                    Capability::None,
                    Capability::None,
                    Capability::None,
                ],
            });

            scheds[0].register_task(TaskControlBlock {
                saved_sp: sp_hsm,
                priority: 4,
                active_priority: 4,
                state: TaskState::Ready,
                name: "HSM Task",
                capabilities: [
                    Capability::Ipc {
                        endpoint_idx: 2,
                        can_send: false,
                        can_recv: true,
                    },
                    Capability::Ipc {
                        endpoint_idx: 3,
                        can_send: true,
                        can_recv: false,
                    },
                    Capability::None,
                    Capability::None,
                    Capability::None,
                    Capability::None,
                    Capability::None,
                    Capability::None,
                ],
            });

            // Core 1 Scheduler registration (Task B + Task C)
            scheds[1].register_task(TaskControlBlock {
                saved_sp: sp_b,
                priority: 2,
                active_priority: 2,
                state: TaskState::Ready,
                name: "Task B",
                capabilities: [
                    Capability::Ipc {
                        endpoint_idx: 0,
                        can_send: false,
                        can_recv: true,
                    },
                    Capability::None,
                    Capability::None,
                    Capability::None,
                    Capability::None,
                    Capability::None,
                    Capability::None,
                    Capability::None,
                ],
            });

            scheds[1].register_task(TaskControlBlock {
                saved_sp: sp_c,
                priority: 3,
                active_priority: 3,
                state: TaskState::Ready,
                name: "Task C",
                capabilities: [
                    Capability::Mutex {
                        mutex_idx: 0,
                        can_lock: true,
                        can_unlock: true,
                    },
                    Capability::None,
                    Capability::None,
                    Capability::None,
                    Capability::None,
                    Capability::None,
                    Capability::None,
                    Capability::None,
                ],
            });

            // Configure timer tick interrupts
            init_timer();

            defmt::info!("Core 0: Heartbeat timer armed. Releasing Core 1...");

            // Release Core 1 boot barrier
            BOOT_BARRIER.store(true, Ordering::Release);

            // Start Core 0 scheduler execution
            scheds[0].start_first_task(0);
        }
    } else {
        // Core 1 (Secondary Hart) boot path
        // Spin-wait until Core 0 completes basic initialization
        while !BOOT_BARRIER.load(Ordering::Acquire) {
            core::hint::spin_loop();
        }

        unsafe {
            // Set Core 1 trap handler
            let trap_addr = _trap_entry as *const () as usize;
            core::arch::asm!("csrw mtvec, {}", in(reg) trap_addr);

            // Initialize local Core 1 memory protection
            init_memory_protection();

            // Initialize local Core 1 timer
            init_timer();

            defmt::info!("Core 1: Booted and timer armed. Launching scheduler...");

            // Start Core 1 scheduler execution
            let scheds = &mut *core::ptr::addr_of_mut!(SCHEDULERS);
            scheds[1].start_first_task(1);
        }
    }
}

/// Verification routine to test the CAN parser and ring buffer on boot.
fn verify_network_and_security() {
    use core::sync::atomic::Ordering;
    use kernel::metrics::{
        METRIC_CAN_PARSE_CYCLES, METRIC_CAN_QUEUE_CYCLES, METRIC_FRAMES_DROPPED, METRIC_FRAMES_RX,
    };
    use network::{CanError, CanFrame, CanRingBuffer};

    defmt::info!("Executing network self-test...");

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

            // 2. Queue frame into lock-free ring buffer and measure SPSC queue operations latency
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

    // Entry 4: Inactive Stack 3 (No Access, Unlocked)
    configure_pmp(
        4,
        core::ptr::addr_of!(TASK_WD_STACK) as usize,
        1024,
        PmpConfig {
            read: false,
            write: false,
            execute: false,
            mode: PmpAddressMode::Napot,
            locked: false,
        },
    );

    // Entry 5: HSM Stack (No Access, Unlocked)
    configure_pmp(
        5,
        core::ptr::addr_of!(HSM_STACK) as usize,
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
    #[cfg(not(kani))]
    {
        let cycles: u32;
        unsafe {
            core::arch::asm!("csrr {0}, mcycle", out(reg) cycles);
        }
        cycles
    }
    #[cfg(kani)]
    {
        0
    }
}

/// Syscall wrapper to trigger cooperative yields from User Mode.
fn yield_now() {
    #[cfg(not(kani))]
    unsafe {
        core::arch::asm!("li a7, 1", "ecall");
    }
}

/// Syscall wrapper to sleep for a specific number of ticks.
fn sleep_ticks(ticks: usize) {
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
fn lock_mutex(idx: usize) {
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
fn unlock_mutex(idx: usize) {
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
fn watchdog_checkin() {
    #[cfg(not(kani))]
    unsafe {
        core::arch::asm!("li a7, 5", "ecall");
    }
}

/// Syscall wrapper to send an IPC message (synchronous rendezvous).
pub(crate) fn sys_send(cap_idx: usize, msg: &[u8]) -> isize {
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
        0
    }
}

/// Syscall wrapper to receive an IPC message (synchronous rendezvous).
pub(crate) fn sys_recv(cap_idx: usize, buf: &mut [u8]) -> isize {
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
        0
    }
}

/// Dedicated Watchdog Task (Highest Priority, Priority 0)
extern "C" fn watchdog_task() -> ! {
    defmt::info!("Watchdog Task (Priority 0) started.");

    // Initialize check-in ticks to the current system tick to avoid false positives at boot
    let current_tick = crate::trap::TICK_COUNT.load(core::sync::atomic::Ordering::Relaxed);
    unsafe {
        let last_checkins = &mut *core::ptr::addr_of_mut!(crate::trap::LAST_CHECKIN_TICK);
        last_checkins[1] = current_tick; // Task A
        last_checkins[2] = current_tick; // Task B
    }

    let mut loop_count = 0u32;

    loop {
        // Sleep for 100 ticks
        sleep_ticks(100);

        let current_tick = crate::trap::TICK_COUNT.load(core::sync::atomic::Ordering::Relaxed);

        // Check health of monitored ready/running tasks
        extern "Rust" {
            static mut SCHEDULERS: [scheduler::bitmap::BitMapScheduler; 2];
        }

        let scheds = unsafe { &mut *core::ptr::addr_of_mut!(SCHEDULERS) };

        let mut check_failed = false;
        let mut failed_task = "";

        // Check Task A (prio 1)
        if let Some(tcb) = &scheds[0].task_table[1] {
            if tcb.state != TaskState::Terminated {
                let last_checkin =
                    unsafe { (*core::ptr::addr_of_mut!(crate::trap::LAST_CHECKIN_TICK))[1] };
                let elapsed = current_tick.wrapping_sub(last_checkin);
                if elapsed > 200 {
                    check_failed = true;
                    failed_task = tcb.name;
                }
            }
        }

        // Check Task B (prio 2)
        if let Some(tcb) = &scheds[1].task_table[2] {
            if tcb.state != TaskState::Terminated {
                let last_checkin =
                    unsafe { (*core::ptr::addr_of_mut!(crate::trap::LAST_CHECKIN_TICK))[2] };
                let elapsed = current_tick.wrapping_sub(last_checkin);
                if elapsed > 200 {
                    check_failed = true;
                    failed_task = tcb.name;
                }
            }
        }

        if check_failed {
            defmt::error!(
                "WATCHDOG FAILURE: Task '{}' failed to check in! (Current Tick: {}, Last Check-in: {})",
                failed_task,
                current_tick,
                unsafe { (*core::ptr::addr_of_mut!(crate::trap::LAST_CHECKIN_TICK))[if failed_task == "Task A" { 1 } else { 2 }] }
            );

            // Print final telemetry dashboard
            crate::kernel::dump_metrics();

            defmt::error!("Safe-parking CPU. Disabling interrupts.");
            unsafe {
                // Disable interrupts (clear MIE bit in mstatus)
                core::arch::asm!("csrci mstatus, 8");
                loop {
                    core::arch::asm!("wfi");
                }
            }
        }

        loop_count = loop_count.wrapping_add(1);
        if loop_count % 5 == 0 {
            defmt::info!("Watchdog: All active monitored tasks are healthy.");
        }
    }
}

/// Task A Entry point (High Priority)
extern "C" fn task_a() -> ! {
    let mut loop_count = 0u32;
    loop {
        defmt::info!("Task A (High) loop starting. Trying to lock Mutex 0...");
        watchdog_checkin();
        lock_mutex(0);
        defmt::info!("Task A (High) locked Mutex 0 successfully.");

        // Small busy work
        for _ in 0..10_000 {
            unsafe { core::arch::asm!("nop") };
        }

        defmt::info!("Task A (High) releasing Mutex 0...");
        unlock_mutex(0);
        defmt::info!("Task A (High) released Mutex 0.");

        // --- HSM cryptographic signing demo ---
        let test_frame = network::can::CanFrame {
            id: 0x3E,
            dlc: 4,
            payload: [0xAA, 0xBB, 0xCC, 0xDD, 0, 0, 0, 0],
        };
        let frame_bytes = unsafe {
            core::slice::from_raw_parts(
                &test_frame as *const network::can::CanFrame as *const u8,
                core::mem::size_of::<network::can::CanFrame>(),
            )
        };
        let mut computed_tag = [0u8; 8];
        defmt::info!("Task A: Requesting HSM to sign CAN frame...");
        let start_cycles = read_cycles();

        // Send the frame to the HSM on capability index 2 (endpoint 2)
        let send_res = sys_send(2, frame_bytes);

        // Receive the signature back on capability index 3 (endpoint 3)
        let recv_res = sys_recv(3, &mut computed_tag);
        let end_cycles = read_cycles();

        if send_res >= 0 && recv_res >= 0 {
            defmt::info!(
                "Task A: Received signature from HSM: {:X} (Overhead: {} cycles)",
                computed_tag,
                end_cycles.wrapping_sub(start_cycles)
            );
            // Verify signature using the helper function (uses endpoints 2 and 3)
            let auth = security::AuthFrame {
                frame: test_frame,
                tag: computed_tag,
            };
            let hsm_verified = security::verify_frame_secure(&auth, 2, 3);
            if hsm_verified {
                defmt::info!("Task A: HSM-calculated signature verified successfully.");
            } else {
                defmt::error!("Task A: HSM signature verification failed!");
            }
        } else {
            defmt::error!(
                "Task A: HSM IPC communication failed! Send: {}, Recv: {}",
                send_res,
                recv_res
            );
        }

        // Send a synchronous IPC message to Task B
        let msg = [0xDE, 0xAD, 0xBE, 0xEF, 0x12, 0x34, 0x56, 0x78];
        defmt::info!("Task A sending IPC telemetry...");
        let res = sys_send(1, &msg);
        defmt::info!("Task A IPC send completed with status: {}", res);

        loop_count = loop_count.wrapping_add(1);
        if loop_count % 10 == 0 {
            kernel::dump_metrics();
        }

        yield_now();
    }
}

/// Task B Entry point (Medium Priority)
extern "C" fn task_b() -> ! {
    let mut loops = 0u32;
    let mut rx_buf = [0u8; 8];
    loop {
        defmt::info!("Task B (Medium) is active. Waiting for IPC telemetry...");

        // Receive synchronous IPC message from Task A
        let res = sys_recv(0, &mut rx_buf);
        if res >= 0 {
            defmt::info!("Task B received IPC payload: {:X}", rx_buf);
        } else {
            defmt::error!("Task B IPC receive failed: {}", res);
        }

        // Simulating a hang after 5 successful loops
        if loops < 5 {
            watchdog_checkin();
        } else {
            defmt::warn!("Task B (Medium) simulating software hang: stopping check-ins!");
        }

        // Large compute loop
        for _ in 0..80_000 {
            unsafe { core::arch::asm!("nop") };
        }

        defmt::info!("Task B (Medium) yielding.");
        loops = loops.wrapping_add(1);
        yield_now();
    }
}

/// Task C Entry point (Low Priority & Fault Injection target)
extern "C" fn task_c() -> ! {
    defmt::info!("Task C (Low) starting. Locking Mutex 0...");
    lock_mutex(0);
    defmt::info!("Task C (Low) acquired Mutex 0. Yielding...");
    yield_now(); // Yield to let Task A preempt us and block on Mutex 0

    defmt::info!("Task C (Low) resumed. Releasing Mutex 0...");
    unlock_mutex(0);
    defmt::info!("Task C (Low) released Mutex 0. Yielding...");
    yield_now();

    // --- FAULT INJECTION ---
    defmt::info!("Task C (Low) injecting fault: attempting illegal read of Task A's stack...");

    // This read should instantly trigger a PMP Load Access Fault (cause 5)
    let illegal_ptr = core::ptr::addr_of!(TASK_A_STACK) as *const u8;
    let _val = unsafe { illegal_ptr.read_volatile() };

    // We should never reach this line because the kernel terminates the task
    defmt::error!("Task C (Low) failed to isolate! Accessed forbidden memory.");
    loop {
        yield_now();
    }
}

/// Configure the Machine-mode Timer.
unsafe fn init_timer() {
    #[cfg(not(kani))]
    {
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
}

/// Global panic handler for bare-metal execution.
#[cfg(not(kani))]
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
    // details printing is omitted to save code space and keep within the 32 KB limit.
    loop {
        unsafe { core::arch::asm!("wfi") };
    }
}
