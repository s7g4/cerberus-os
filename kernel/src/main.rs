//! Cerberus-OS — Kernel entry point.

#![no_std]
#![no_main]
#![feature(naked_functions)]
#![allow(unused_variables, dead_code, stable_features)]

mod logger_setup {
    extern crate defmt as real_defmt;

    static LOG_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

    #[real_defmt::global_logger]
    struct MyLogger;

    unsafe impl real_defmt::Logger for MyLogger {
        fn acquire() {
            let _ = LOG_LOCK.compare_exchange(
                false,
                true,
                core::sync::atomic::Ordering::Acquire,
                core::sync::atomic::Ordering::Relaxed,
            );
        }

        unsafe fn release() {
            LOG_LOCK.store(false, core::sync::atomic::Ordering::Release);
        }

        unsafe fn write(bytes: &[u8]) {
            unsafe {
                crate::SEGGER_RTT_WriteNoLock(0, bytes.as_ptr(), bytes.len());
            }
        }

        unsafe fn flush() {}
    }

    real_defmt::timestamp!("{=u32}", 0);
}

pub struct HexSlice<'a>(pub &'a [u8]);

impl core::fmt::UpperHex for HexSlice<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for &byte in self.0 {
            write!(f, "{:02X}", byte)?;
        }
        Ok(())
    }
}

#[macro_export]
macro_rules! rtt_info {
    ($($arg:tt)*) => {
        $crate::rtt_println!($($arg)*);
    };
}

#[macro_export]
macro_rules! rtt_warn {
    ($($arg:tt)*) => {
        $crate::rtt_println!($($arg)*);
    };
}

#[macro_export]
macro_rules! rtt_error {
    ($($arg:tt)*) => {
        $crate::rtt_println!($($arg)*);
    };
}

#[macro_export]
macro_rules! rtt_trace {
    ($($arg:tt)*) => {
        $crate::rtt_println!($($arg)*);
    };
}

pub mod defmt {
    pub struct RttWriter;

    impl core::fmt::Write for RttWriter {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            unsafe {
                crate::SEGGER_RTT_WriteNoLock(0, s.as_ptr(), s.len());
            }
            Ok(())
        }
    }

    #[macro_export]
    macro_rules! rtt_println {
        ($($arg:tt)*) => {
            {
                use core::fmt::Write;
                let mut writer = $crate::defmt::RttWriter;
                let _ = writeln!(&mut writer, $($arg)*);
            }
        };
    }

    #[macro_export]
    macro_rules! rtt_print {
        ($($arg:tt)*) => {
            {
                use core::fmt::Write;
                let mut writer = $crate::defmt::RttWriter;
                let _ = write!(&mut writer, $($arg)*);
            }
        };
    }

    pub use crate::{
        rtt_info as info,
        rtt_warn as warn,
        rtt_error as error,
        rtt_trace as trace,
    };
}


#[no_mangle]
#[inline(never)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn SEGGER_RTT_WriteNoLock(
    channel: usize,
    ptr: *const u8,
    len: usize,
) -> usize {
    core::hint::black_box(channel);
    core::hint::black_box(ptr);
    core::hint::black_box(len);
    len
}

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

#[repr(align(1024))]
pub struct Stack(pub [u8; 1024]);

// Static buffers for task stacks (allocated statically, no heap)
pub static mut TASK_A_STACK: Stack = Stack([0; 1024]);
pub static mut TASK_B_STACK: Stack = Stack([0; 1024]);
pub static mut TASK_C_STACK: Stack = Stack([0; 1024]);
pub static mut TASK_WD_STACK: Stack = Stack([0; 1024]);
pub static mut IDLE_STACK_0: Stack = Stack([0; 1024]);
pub static mut IDLE_STACK_1: Stack = Stack([0; 1024]);

// Dedicated Kernel Interrupt Stacks (M-mode execution per core)
pub static mut KERNEL_STACK_0: Stack = Stack([0; 1024]);
pub static mut KERNEL_STACK_1: Stack = Stack([0; 1024]);
pub static mut HSM_STACK: Stack = Stack([0; 1024]);

// Synchronization primitives for SMP execution
pub static BOOT_BARRIER: AtomicBool = AtomicBool::new(false);
pub static CORE_1_READY: AtomicBool = AtomicBool::new(false);
pub static mut TEST_COUNTER: u32 = 0;
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
                &mut (*core::ptr::addr_of_mut!(TASK_WD_STACK)).0,
                watchdog_task,
            );
            let sp_a = TaskControlBlock::initialize_stack(
                &mut (*core::ptr::addr_of_mut!(TASK_A_STACK)).0,
                task_a,
            );
            let sp_b = TaskControlBlock::initialize_stack(
                &mut (*core::ptr::addr_of_mut!(TASK_B_STACK)).0,
                task_b,
            );
            let sp_c = TaskControlBlock::initialize_stack(
                &mut (*core::ptr::addr_of_mut!(TASK_C_STACK)).0,
                task_c,
            );
            let sp_hsm = TaskControlBlock::initialize_stack(
                &mut (*core::ptr::addr_of_mut!(HSM_STACK)).0,
                security::hsm_task,
            );
            let sp_idle0 = TaskControlBlock::initialize_stack(
                &mut (*core::ptr::addr_of_mut!(IDLE_STACK_0)).0,
                idle_task,
            );
            let sp_idle1 = TaskControlBlock::initialize_stack(
                &mut (*core::ptr::addr_of_mut!(IDLE_STACK_1)).0,
                idle_task,
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

            scheds[0].register_task(TaskControlBlock {
                saved_sp: sp_idle0,
                priority: 31,
                active_priority: 31,
                state: TaskState::Ready,
                name: "Idle 0",
                capabilities: [Capability::None; 8],
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

            scheds[1].register_task(TaskControlBlock {
                saved_sp: sp_idle1,
                priority: 31,
                active_priority: 31,
                state: TaskState::Ready,
                name: "Idle 1",
                capabilities: [Capability::None; 8],
            });

            // Configure timer tick interrupts
            init_timer(hart_id);

            defmt::info!("Core 0: Heartbeat timer armed. Releasing Core 1...");

            // Release Core 1 boot barrier
            BOOT_BARRIER.store(true, Ordering::Release);

            // Spin-wait until Core 1 is ready before starting Core 0 scheduler execution
            while !CORE_1_READY.load(Ordering::Acquire) {
                core::hint::spin_loop();
            }

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
            init_timer(hart_id);

            defmt::info!("Core 1: Booted and timer armed. Launching scheduler...");

            // Signal that Core 1 is ready
            CORE_1_READY.store(true, Ordering::Release);

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
            core::arch::asm!("csrr {0}, cycle", out(reg) cycles);
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
        core::arch::asm!(
            "ecall",
            in("a7") 1usize,
        );
    }
}

/// Syscall wrapper to sleep for a specific number of ticks.
fn sleep_ticks(ticks: usize) {
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
fn lock_mutex(idx: usize) {
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
fn unlock_mutex(idx: usize) {
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
fn watchdog_checkin() {
    #[cfg(not(kani))]
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") 5usize,
        );
    }
}

/// Syscall wrapper to send an IPC message (synchronous rendezvous).
pub(crate) fn sys_send(cap_idx: usize, msg: &[u8]) -> isize {
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
        0
    }
}

/// Syscall wrapper to terminate a task by priority.
fn sys_terminate_task(prio: usize) -> isize {
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

/// Dedicated Watchdog Task (Highest Priority, Priority 0)
extern "C" fn watchdog_task() -> ! {
    defmt::info!("Watchdog Task (Priority 0) started.");

    // Let tasks initialize and perform their first check-in within a boot grace period.
    // We leave LAST_CHECKIN_TICK initialized to 0, which signifies "not checked in yet".

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
                let elapsed = if last_checkin == 0 {
                    current_tick
                } else {
                    current_tick.wrapping_sub(last_checkin)
                };
                let timeout = if last_checkin == 0 { 1000 } else { 200 };
                if elapsed > timeout {
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
                let elapsed = if last_checkin == 0 {
                    current_tick
                } else {
                    current_tick.wrapping_sub(last_checkin)
                };
                let timeout = if last_checkin == 0 { 1000 } else { 200 };
                if elapsed > timeout {
                    check_failed = true;
                    failed_task = tcb.name;
                }
            }
        }

        // Check Task C (prio 3 - Test Runner)
        if let Some(tcb) = &scheds[1].task_table[3] {
            if tcb.state != TaskState::Terminated {
                let last_checkin =
                    unsafe { (*core::ptr::addr_of_mut!(crate::trap::LAST_CHECKIN_TICK))[3] };
                let elapsed = if last_checkin == 0 {
                    current_tick
                } else {
                    current_tick.wrapping_sub(last_checkin)
                };
                let timeout = if last_checkin == 0 { 1000 } else { 200 };
                if elapsed > timeout {
                    check_failed = true;
                    failed_task = tcb.name;
                }
            }
        }

        if check_failed {
            let failed_prio = if failed_task == "Task A" { 1 } else if failed_task == "Task B" { 2 } else { 3 };
            defmt::error!(
                "WATCHDOG FAILURE: Task '{}' failed to check in! (Current Tick: {}, Last Check-in: {})",
                failed_task,
                current_tick,
                unsafe { (*core::ptr::addr_of_mut!(crate::trap::LAST_CHECKIN_TICK))[failed_prio] }
            );

            defmt::warn!("WATCHDOG: Terminating task '{}' to restore availability.", failed_task);
            sys_terminate_task(failed_prio);
        }

        // Check if Task C needs to be restarted for the next test
        unsafe {
            let scheds = &mut *core::ptr::addr_of_mut!(SCHEDULERS);
            if let Some(ref tcb) = scheds[1].task_table[3] {
                if tcb.state == TaskState::Terminated && TEST_COUNTER < 2 {
                    TEST_COUNTER += 1;
                    let sp_c = TaskControlBlock::initialize_stack(
                        &mut (*core::ptr::addr_of_mut!(TASK_C_STACK)).0,
                        task_c,
                    );
                    if let Some(ref mut tcb_mut) = scheds[1].task_table[3] {
                        tcb_mut.saved_sp = sp_c;
                        tcb_mut.state = TaskState::Ready;
                        defmt::info!("[WATCHDOG] Resetting Task C stack and restarting to run Test {}", TEST_COUNTER + 1);
                    }
                    (*core::ptr::addr_of_mut!(crate::trap::LAST_CHECKIN_TICK))[3] = 0;
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
                HexSlice(&computed_tag),
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
            defmt::info!("Task B received IPC payload: {:X}", HexSlice(&rx_buf));
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
    let tc = unsafe { TEST_COUNTER };
    
    if tc == 0 {
        defmt::info!("Task C (Low) starting. Locking Mutex 0...");
        lock_mutex(0);
        defmt::info!("Task C (Low) acquired Mutex 0. Yielding...");
        yield_now(); // Yield to let Task A preempt us and block on Mutex 0

        defmt::info!("Task C (Low) resumed. Releasing Mutex 0...");
        unlock_mutex(0);
        defmt::info!("Task C (Low) released Mutex 0. Yielding...");
        yield_now();

        // --- FAULT INJECTION TEST 1: PMP Stack Violation ---
        defmt::info!("[TEST RUNNER] Running Test 1: PMP Stack Violation (Load Access Fault)...");
        let illegal_ptr = core::ptr::addr_of!(TASK_A_STACK) as *const u8;
        let _val = unsafe { illegal_ptr.read_volatile() };
        
        defmt::error!("[TEST RUNNER] Test 1 failed to contain fault!");
    } else if tc == 1 {
        // --- FAULT INJECTION TEST 2: Privilege Violation (Illegal Instruction) ---
        defmt::info!("[TEST RUNNER] Running Test 2: Privilege Violation (Illegal Instruction)...");
        unsafe {
            core::arch::asm!("csrw mstatus, zero");
        }
        
        defmt::error!("[TEST RUNNER] Test 2 failed to contain fault!");
    } else {
        // --- FAULT INJECTION TEST 3: Watchdog Timeout (Temporal Isolation) ---
        defmt::info!("[TEST RUNNER] Running Test 3: Watchdog Timeout (Temporal Isolation)...");
        watchdog_checkin();
        loop {
            yield_now();
        }
    }

    loop {
        yield_now();
    }
}

/// Configure the Machine-mode Timer.
unsafe fn init_timer(hart_id: usize) {
    #[cfg(not(kani))]
    {
        let clint_mtime = 0x0200_BFF8 as *const u64;
        let clint_mtimecmp = (0x0200_4000 + hart_id * 8) as *mut u64;
        clint_mtimecmp.write_volatile(clint_mtime.read_volatile() + 40_000);

        let mie: usize;
        core::arch::asm!("csrr {}, mie", out(reg) mie);
        core::arch::asm!("csrw mie, {}", in(reg) mie | (1 << 7));

        let mstatus: usize;
        core::arch::asm!("csrr {}, mstatus", out(reg) mstatus);
        core::arch::asm!("csrw mstatus, {}", in(reg) mstatus | (1 << 3));

        // Enable User-mode access to the cycle counter (CY bit)
        core::arch::asm!("csrw mcounteren, {}", in(reg) 1usize);
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

#[no_mangle]
pub extern "C" fn idle_task() -> ! {
    loop {
        unsafe {
            core::arch::asm!("nop");
        }
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "Rust" fn _mp_hook(hartid: usize) -> bool {
    hartid == 0
}
