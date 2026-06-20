//! Trap and Interrupt handling subsystem.

use core::sync::atomic::{AtomicU32, Ordering};

/// Master clock counter incremented on every timer interrupt.
pub static TICK_COUNT: AtomicU32 = AtomicU32::new(0);

/// Measured clock cycle count overhead for entry context preservation.
pub static METRIC_TRAP_LATENCY_CYCLES: AtomicU32 = AtomicU32::new(0);

/// Array tracking the tick count of the last checkin for each task.
#[no_mangle]
pub static mut LAST_CHECKIN_TICK: [u32; 32] = [0; 32];

/// Core trap dispatcher called from assembly when an exception or interrupt occurs.
///
/// Returns the stack pointer of the next task to run.
#[no_mangle]
pub unsafe extern "C" fn trap_handler(mcause: usize, user_sp: usize, start_cycle: usize) -> usize {
    // Measure context-saving execution latency
    let elapsed = {
        #[cfg(not(kani))]
        {
            let end_cycle: usize;
            core::arch::asm!("csrr {}, mcycle", out(reg) end_cycle);
            end_cycle.wrapping_sub(start_cycle) as u32
        }
        #[cfg(kani)]
        {
            0
        }
    };
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

            // Scan and wake tasks blocked on ticks
            for i in 0..crate::scheduler::bitmap::MAX_PARTITIONS {
                if let Some(tcb) = &mut sched.task_table[i] {
                    if let crate::scheduler::TaskState::Blocked { wake_tick } = tcb.state {
                        if tick >= wake_tick {
                            tcb.state = crate::scheduler::TaskState::Ready;
                        }
                    }
                }
            }

            if let Some((old_sp_ptr, new_sp)) = sched.schedule(true) {
                // Save the stack pointer of the outgoing task
                old_sp_ptr.write_volatile(current_sp);
                // Load the new stack pointer
                current_sp = new_sp;

                // Reprogram PMP stack sandboxing rules for the incoming task
                let active_idx = sched.current_partition_idx;
                if let Some(new_tcb) = &sched.task_table[active_idx] {
                    crate::memory::reprogram_pmp_stack(new_tcb.name);
                }
            }
        }
        ECALL_UMODE => {
            // Read registers from user stack frame
            let frame = current_sp as *mut usize;
            let syscall_id = frame.add(13).read_volatile(); // a7 is index 13 (offset 52)

            // Advance PC past the 4-byte ecall instruction
            let mepc = frame.add(28).read_volatile(); // mepc is index 28 (offset 112)
            frame.add(28).write_volatile(mepc + 4);

            match syscall_id {
                1 => {
                    // Syscall 1: Cooperative Yield
                    extern "Rust" {
                        static mut SCHEDULER: crate::scheduler::bitmap::BitMapScheduler;
                    }
                    let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
                    if let Some((old_sp_ptr, new_sp)) = sched.schedule(false) {
                        old_sp_ptr.write_volatile(current_sp);
                        current_sp = new_sp;

                        let active_idx = sched.current_partition_idx;
                        if let Some(new_tcb) = &sched.task_table[active_idx] {
                            crate::memory::reprogram_pmp_stack(new_tcb.name);
                        }
                    }
                }
                2 => {
                    // Syscall 2: Sleep Ticks
                    let ticks = frame.add(6).read_volatile(); // a0
                    extern "Rust" {
                        static mut SCHEDULER: crate::scheduler::bitmap::BitMapScheduler;
                    }
                    let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
                    let running_idx = sched.current_partition_idx;

                    let current_tick = TICK_COUNT.load(Ordering::Relaxed);
                    let tcb = sched.task_table[running_idx].as_mut().unwrap();
                    tcb.state = crate::scheduler::TaskState::Blocked {
                        wake_tick: current_tick + ticks as u32,
                    };

                    // Reschedule because the current task is now blocked
                    if let Some((old_sp_ptr, new_sp)) = sched.schedule(false) {
                        old_sp_ptr.write_volatile(current_sp);
                        current_sp = new_sp;
                        let active_idx = sched.current_partition_idx;
                        if let Some(new_tcb) = &sched.task_table[active_idx] {
                            crate::memory::reprogram_pmp_stack(new_tcb.name);
                        }
                    }
                }
                3 => {
                    // Syscall 3: Lock Mutex
                    let cap_idx = frame.add(6).read_volatile(); // a0
                    extern "Rust" {
                        static mut SCHEDULER: crate::scheduler::bitmap::BitMapScheduler;
                        static mut MUTEXES: [Option<crate::KernelMutex>; 8];
                    }
                    let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
                    let running_idx = sched.current_partition_idx;

                    let mut lock_granted = false;
                    let mut permission_denied = false;

                    if cap_idx < 8 {
                        let tcb = sched.task_table[running_idx].as_mut().unwrap();
                        if let crate::scheduler::Capability::Mutex {
                            mutex_idx,
                            can_lock: true,
                            ..
                        } = tcb.capabilities[cap_idx]
                        {
                            let mutex_idx = mutex_idx as usize;
                            if let Some(mutex) = &mut (*core::ptr::addr_of_mut!(MUTEXES))[mutex_idx]
                            {
                                if !mutex.locked {
                                    mutex.locked = true;
                                    mutex.owner_task_idx = Some(running_idx as u8);
                                    lock_granted = true;
                                } else {
                                    // Mutex is locked: block the current task on the mutex
                                    sched.task_table[running_idx].as_mut().unwrap().state =
                                        crate::scheduler::TaskState::BlockedOnMutex {
                                            mutex_idx: mutex_idx as u8,
                                        };

                                    mutex.waiters_bitmap |= 1 << running_idx;

                                    // Reschedule because the current task is blocked
                                    if let Some((old_sp_ptr, new_sp)) = sched.schedule(false) {
                                        old_sp_ptr.write_volatile(current_sp);
                                        current_sp = new_sp;
                                        let active_idx = sched.current_partition_idx;
                                        if let Some(new_tcb) = &sched.task_table[active_idx] {
                                            crate::memory::reprogram_pmp_stack(new_tcb.name);
                                        }
                                    }
                                    lock_granted = true; // Handled scheduling, don't write success immediately
                                }
                            } else {
                                permission_denied = true;
                            }
                        } else {
                            permission_denied = true;
                        }
                    } else {
                        permission_denied = true;
                    }

                    if permission_denied {
                        frame.add(6).write_volatile(-1isize as usize);
                    } else if lock_granted
                        && !matches!(
                            sched.task_table[running_idx].as_ref().unwrap().state,
                            crate::scheduler::TaskState::BlockedOnMutex { .. }
                        )
                    {
                        frame.add(6).write_volatile(0); // Return success
                    }
                }
                4 => {
                    // Syscall 4: Unlock Mutex
                    let cap_idx = frame.add(6).read_volatile(); // a0
                    extern "Rust" {
                        static mut SCHEDULER: crate::scheduler::bitmap::BitMapScheduler;
                        static mut MUTEXES: [Option<crate::KernelMutex>; 8];
                    }
                    let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
                    let running_idx = sched.current_partition_idx;

                    let mut permission_denied = false;
                    let mut unlock_success = false;

                    if cap_idx < 8 {
                        let tcb = sched.task_table[running_idx].as_mut().unwrap();
                        if let crate::scheduler::Capability::Mutex {
                            mutex_idx,
                            can_unlock: true,
                            ..
                        } = tcb.capabilities[cap_idx]
                        {
                            let mutex_idx = mutex_idx as usize;
                            if let Some(mutex) = &mut (*core::ptr::addr_of_mut!(MUTEXES))[mutex_idx]
                            {
                                assert_eq!(mutex.owner_task_idx, Some(running_idx as u8));

                                // Find the highest priority waiter in the bitmap
                                let mut highest_waiter: Option<usize> = None;
                                for i in 0..32 {
                                    if (mutex.waiters_bitmap & (1 << i)) != 0 {
                                        if let Some(w) = highest_waiter {
                                            let w_prio =
                                                sched.task_table[w].as_ref().unwrap().priority;
                                            let i_prio =
                                                sched.task_table[i].as_ref().unwrap().priority;
                                            if i_prio < w_prio {
                                                highest_waiter = Some(i);
                                            }
                                        } else {
                                            highest_waiter = Some(i);
                                        }
                                    }
                                }

                                if let Some(waiter_idx) = highest_waiter {
                                    // Transfer ownership to the waiter
                                    mutex.owner_task_idx = Some(waiter_idx as u8);
                                    mutex.waiters_bitmap &= !(1 << waiter_idx);

                                    // Unblock the waiter
                                    let waiter_tcb = sched.task_table[waiter_idx].as_mut().unwrap();
                                    waiter_tcb.state = crate::scheduler::TaskState::Ready;

                                    // Write 0 to waiter's saved frame a0 to indicate success
                                    let waiter_frame = waiter_tcb.saved_sp as *mut usize;
                                    waiter_frame.add(6).write_volatile(0);
                                } else {
                                    mutex.locked = false;
                                    mutex.owner_task_idx = None;
                                }
                                unlock_success = true;

                                // Reschedule to run unblocked waiter if a swap is triggered
                                if let Some((old_sp_ptr, new_sp)) = sched.schedule(false) {
                                    old_sp_ptr.write_volatile(current_sp);
                                    current_sp = new_sp;
                                    let active_idx = sched.current_partition_idx;
                                    if let Some(new_tcb) = &sched.task_table[active_idx] {
                                        crate::memory::reprogram_pmp_stack(new_tcb.name);
                                    }
                                }
                            } else {
                                permission_denied = true;
                            }
                        } else {
                            permission_denied = true;
                        }
                    } else {
                        permission_denied = true;
                    }

                    if permission_denied {
                        frame.add(6).write_volatile(-1isize as usize);
                    } else if unlock_success {
                        frame.add(6).write_volatile(0);
                    }
                }

                5 => {
                    // Syscall 5: Watchdog Check-in
                    extern "Rust" {
                        static mut SCHEDULER: crate::scheduler::bitmap::BitMapScheduler;
                    }
                    let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
                    let running_idx = sched.current_partition_idx;
                    let current_tick = TICK_COUNT.load(Ordering::Relaxed);
                    LAST_CHECKIN_TICK[running_idx] = current_tick;
                }

                6 => {
                    // Syscall 6: Send IPC (Synchronous Rendezvous)
                    let cap_idx = frame.add(6).read_volatile(); // a0
                    let msg_addr = frame.add(7).read_volatile(); // a1
                    let msg_len = frame.add(8).read_volatile(); // a2

                    extern "Rust" {
                        static mut SCHEDULER: crate::scheduler::bitmap::BitMapScheduler;
                    }
                    let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
                    let running_idx = sched.current_partition_idx;

                    let mut permission_denied = false;

                    if cap_idx < 8 {
                        let tcb = sched.task_table[running_idx].as_mut().unwrap();
                        if let crate::scheduler::Capability::Ipc {
                            endpoint_idx,
                            can_send: true,
                            ..
                        } = tcb.capabilities[cap_idx]
                        {
                            // Search for a waiting receiver on this endpoint
                            let mut found_receiver: Option<usize> = None;
                            for i in 0..crate::scheduler::bitmap::MAX_PARTITIONS {
                                if let Some(other_tcb) = &sched.task_table[i] {
                                    if let crate::scheduler::TaskState::BlockedOnIpcRecv {
                                        endpoint_idx: rx_ep,
                                        ..
                                    } = other_tcb.state
                                    {
                                        if rx_ep == endpoint_idx {
                                            found_receiver = Some(i);
                                            break;
                                        }
                                    }
                                }
                            }

                            if let Some(rx_idx) = found_receiver {
                                // Rendezvous Met! Transfer data directly.
                                let receiver_tcb = sched.task_table[rx_idx].as_mut().unwrap();

                                if let crate::scheduler::TaskState::BlockedOnIpcRecv {
                                    buf_addr,
                                    max_len,
                                    ..
                                } = receiver_tcb.state
                                {
                                    let transfer_len = core::cmp::min(msg_len, max_len);

                                    // Perform direct stack-to-stack zero-copy copy
                                    core::ptr::copy_nonoverlapping(
                                        msg_addr as *const u8,
                                        buf_addr as *mut u8,
                                        transfer_len,
                                    );

                                    // Set receiver's state to Ready
                                    receiver_tcb.state = crate::scheduler::TaskState::Ready;

                                    // Write transfer_len (number of bytes received) to receiver's saved frame a0
                                    let rx_frame = receiver_tcb.saved_sp as *mut usize;
                                    rx_frame.add(6).write_volatile(transfer_len);

                                    // Write 0 (success) to sender's frame a0
                                    frame.add(6).write_volatile(0);
                                }
                            } else {
                                // No receiver waiting: block the sender
                                let sender_tcb = sched.task_table[running_idx].as_mut().unwrap();
                                sender_tcb.state = crate::scheduler::TaskState::BlockedOnIpcSend {
                                    endpoint_idx,
                                    msg_addr,
                                    msg_len,
                                };

                                // Reschedule since current task is now blocked
                                if let Some((old_sp_ptr, new_sp)) = sched.schedule(false) {
                                    old_sp_ptr.write_volatile(current_sp);
                                    current_sp = new_sp;
                                    let active_idx = sched.current_partition_idx;
                                    if let Some(new_tcb) = &sched.task_table[active_idx] {
                                        crate::memory::reprogram_pmp_stack(new_tcb.name);
                                    }
                                }
                            }
                        } else {
                            permission_denied = true;
                        }
                    } else {
                        permission_denied = true;
                    }

                    if permission_denied {
                        frame.add(6).write_volatile(-1isize as usize);
                    }
                }
                7 => {
                    // Syscall 7: Receive IPC (Synchronous Rendezvous)
                    let cap_idx = frame.add(6).read_volatile(); // a0
                    let buf_addr = frame.add(7).read_volatile(); // a1
                    let max_len = frame.add(8).read_volatile(); // a2

                    extern "Rust" {
                        static mut SCHEDULER: crate::scheduler::bitmap::BitMapScheduler;
                    }
                    let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
                    let running_idx = sched.current_partition_idx;

                    let mut permission_denied = false;

                    if cap_idx < 8 {
                        let tcb = sched.task_table[running_idx].as_mut().unwrap();
                        if let crate::scheduler::Capability::Ipc {
                            endpoint_idx,
                            can_recv: true,
                            ..
                        } = tcb.capabilities[cap_idx]
                        {
                            // Search for a waiting sender on this endpoint
                            let mut found_sender: Option<usize> = None;
                            for i in 0..crate::scheduler::bitmap::MAX_PARTITIONS {
                                if let Some(other_tcb) = &sched.task_table[i] {
                                    if let crate::scheduler::TaskState::BlockedOnIpcSend {
                                        endpoint_idx: tx_ep,
                                        ..
                                    } = other_tcb.state
                                    {
                                        if tx_ep == endpoint_idx {
                                            found_sender = Some(i);
                                            break;
                                        }
                                    }
                                }
                            }

                            if let Some(tx_idx) = found_sender {
                                // Rendezvous Met! Transfer data directly.
                                let sender_tcb = sched.task_table[tx_idx].as_mut().unwrap();

                                if let crate::scheduler::TaskState::BlockedOnIpcSend {
                                    msg_addr,
                                    msg_len,
                                    ..
                                } = sender_tcb.state
                                {
                                    let transfer_len = core::cmp::min(msg_len, max_len);

                                    // Perform direct stack-to-stack zero-copy copy
                                    core::ptr::copy_nonoverlapping(
                                        msg_addr as *const u8,
                                        buf_addr as *mut u8,
                                        transfer_len,
                                    );

                                    // Set sender's state to Ready
                                    sender_tcb.state = crate::scheduler::TaskState::Ready;

                                    // Write 0 (success) to sender's saved frame a0
                                    let tx_frame = sender_tcb.saved_sp as *mut usize;
                                    tx_frame.add(6).write_volatile(0);

                                    // Write transfer_len (number of bytes received) to receiver's frame a0
                                    frame.add(6).write_volatile(transfer_len);
                                }
                            } else {
                                // No sender waiting: block the receiver
                                let receiver_tcb = sched.task_table[running_idx].as_mut().unwrap();
                                receiver_tcb.state =
                                    crate::scheduler::TaskState::BlockedOnIpcRecv {
                                        endpoint_idx,
                                        buf_addr,
                                        max_len,
                                    };

                                // Reschedule since current task is now blocked
                                if let Some((old_sp_ptr, new_sp)) = sched.schedule(false) {
                                    old_sp_ptr.write_volatile(current_sp);
                                    current_sp = new_sp;
                                    let active_idx = sched.current_partition_idx;
                                    if let Some(new_tcb) = &sched.task_table[active_idx] {
                                        crate::memory::reprogram_pmp_stack(new_tcb.name);
                                    }
                                }
                            }
                        } else {
                            permission_denied = true;
                        }
                    } else {
                        permission_denied = true;
                    }

                    if permission_denied {
                        frame.add(6).write_volatile(-1isize as usize);
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

                // Identify and terminate the faulty U-mode task
                extern "Rust" {
                    static mut SCHEDULER: crate::scheduler::bitmap::BitMapScheduler;
                }
                let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
                let running_idx = sched.current_partition_idx;

                if let Some(tcb) = &mut sched.task_table[running_idx] {
                    defmt::error!(
                        "SECURITY FAULT: Task '{}' triggered memory access violation (cause: {}). Terminating task.",
                        tcb.name,
                        cause
                    );
                    tcb.state = crate::scheduler::TaskState::Terminated;
                }

                // Reschedule to run a healthy task
                if let Some((old_sp_ptr, new_sp)) = sched.schedule(false) {
                    old_sp_ptr.write_volatile(current_sp);
                    current_sp = new_sp;
                    let active_idx = sched.current_partition_idx;
                    if let Some(new_tcb) = &sched.task_table[active_idx] {
                        crate::memory::reprogram_pmp_stack(new_tcb.name);
                    }
                }
            } else {
                defmt::error!("Unhandled exception. Cause register: 0x{:08X}", cause);
                let frame = current_sp as *const usize;
                let mepc = frame.add(28).read_volatile();
                defmt::error!("Instruction pointer (mepc): 0x{:08X}", mepc);
                panic!("unhandled exception");
            }
        }
    }

    current_sp
}
