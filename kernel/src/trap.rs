//! Trap and Interrupt handling subsystem.

#![allow(unused_variables)]

use crate::defmt;
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
    const SOFTWARE_INTERRUPT: usize = (1 << 31) | 3; // Machine-mode software interrupt (IPI)
    const ECALL_UMODE: usize = 8; // Environment call from U-mode

    let hart_id: usize = {
        let id: usize;
        #[cfg(not(kani))]
        unsafe {
            core::arch::asm!("csrr {}, mhartid", out(reg) id);
        }
        #[cfg(kani)]
        {
            id = 0;
        }
        id
    };

    let mut current_sp = user_sp;

    match mcause {
        TIMER_INTERRUPT => {
            let tick = TICK_COUNT.fetch_add(1, Ordering::Relaxed) + 1;

            // Re-arm local timer (CLINT mtimecmp += interval)
            let clint_mtime = 0x0200_BFF8 as *const u64;
            let clint_mtimecmp = (0x0200_4000 + hart_id * 8) as *mut u64;
            clint_mtimecmp.write_volatile(clint_mtime.read_volatile() + 40_000);

            // Log tick periodically
            if tick % 10 == 0 && hart_id == 0 {
                defmt::trace!("Tick: {} (Overhead: {} cycles)", tick, elapsed);
            }

            // Perform context switch if a different task is ready
            extern "Rust" {
                static mut SCHEDULERS: [scheduler::bitmap::BitMapScheduler; 2];
                static SCHED_LOCK: crate::kernel::spinlock::Spinlock;
            }
            let _sched_guard = SCHED_LOCK.lock_guard(hart_id);
            let sched = &mut (*core::ptr::addr_of_mut!(SCHEDULERS))[hart_id];

            // Scan and wake tasks blocked on ticks
            for i in 0..scheduler::bitmap::MAX_PARTITIONS {
                if let Some(tcb) = &mut sched.task_table[i] {
                    if let scheduler::TaskState::Blocked { wake_tick } = tcb.state {
                        if tick >= wake_tick {
                            tcb.state = scheduler::TaskState::Ready;
                        }
                    }
                }
            }

            switch_task(sched, &mut current_sp, true);
        }
        SOFTWARE_INTERRUPT => {
            // Clear Core Local Software Interrupt pending bit
            let msip = (0x0200_0000 + hart_id * 4) as *mut u32;
            msip.write_volatile(0);

            // Force a reschedule check on Software Interrupt
            extern "Rust" {
                static mut SCHEDULERS: [scheduler::bitmap::BitMapScheduler; 2];
                static SCHED_LOCK: crate::kernel::spinlock::Spinlock;
            }
            let _sched_guard = SCHED_LOCK.lock_guard(hart_id);
            let sched = &mut (*core::ptr::addr_of_mut!(SCHEDULERS))[hart_id];
            switch_task(sched, &mut current_sp, false);
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
                        static mut SCHEDULERS: [scheduler::bitmap::BitMapScheduler; 2];
                        static SCHED_LOCK: crate::kernel::spinlock::Spinlock;
                    }
                    let _sched_guard = SCHED_LOCK.lock_guard(hart_id);
                    let sched = &mut (*core::ptr::addr_of_mut!(SCHEDULERS))[hart_id];
                    switch_task(sched, &mut current_sp, false);
                }
                2 => {
                    // Syscall 2: Sleep Ticks
                    let ticks = frame.add(6).read_volatile(); // a0
                    extern "Rust" {
                        static mut SCHEDULERS: [scheduler::bitmap::BitMapScheduler; 2];
                        static SCHED_LOCK: crate::kernel::spinlock::Spinlock;
                    }
                    let _sched_guard = SCHED_LOCK.lock_guard(hart_id);
                    let sched = &mut (*core::ptr::addr_of_mut!(SCHEDULERS))[hart_id];
                    let running_idx = sched.current_partition_idx;

                    let current_tick = TICK_COUNT.load(Ordering::Relaxed);
                    let tcb = sched.task_table[running_idx].as_mut().unwrap();
                    tcb.state = scheduler::TaskState::Blocked {
                        wake_tick: current_tick + ticks as u32,
                    };

                    // Reschedule because the current task is now blocked
                    switch_task(sched, &mut current_sp, false);
                }
                3 => {
                    // Syscall 3: Lock Mutex
                    let cap_idx = frame.add(6).read_volatile(); // a0
                    extern "Rust" {
                        static mut SCHEDULERS: [scheduler::bitmap::BitMapScheduler; 2];
                        static mut MUTEXES: [Option<crate::KernelMutex>; 8];
                        static MUTEX_LOCK: crate::kernel::spinlock::Spinlock;
                        static SCHED_LOCK: crate::kernel::spinlock::Spinlock;
                    }
                    let _sched_guard = SCHED_LOCK.lock_guard(hart_id);
                    let scheds = &mut *core::ptr::addr_of_mut!(SCHEDULERS);
                    let sched = &mut scheds[hart_id];
                    let running_idx = sched.current_partition_idx;

                    let mut lock_granted = false;
                    let mut permission_denied = false;

                    MUTEX_LOCK.lock(hart_id);

                    if cap_idx < 8 {
                        let tcb = sched.task_table[running_idx].as_mut().unwrap();
                        if let scheduler::Capability::Mutex {
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
                                    MUTEX_LOCK.unlock();
                                } else {
                                    // Mutex is locked: block the current task on the mutex
                                    sched.task_table[running_idx].as_mut().unwrap().state =
                                        scheduler::TaskState::BlockedOnMutex {
                                            mutex_idx: mutex_idx as u8,
                                        };

                                    mutex.waiters_bitmap |= 1 << running_idx;
                                    MUTEX_LOCK.unlock();

                                    // Reschedule because the current task is blocked
                                    switch_task(sched, &mut current_sp, false);
                                    lock_granted = true; // Handled scheduling, don't write success immediately
                                }
                            } else {
                                MUTEX_LOCK.unlock();
                                permission_denied = true;
                            }
                        } else {
                            MUTEX_LOCK.unlock();
                            permission_denied = true;
                        }
                    } else {
                        MUTEX_LOCK.unlock();
                        permission_denied = true;
                    }

                    if permission_denied {
                        frame.add(6).write_volatile(-1isize as usize);
                    } else if lock_granted
                        && !matches!(
                            sched.task_table[running_idx].as_ref().unwrap().state,
                            scheduler::TaskState::BlockedOnMutex { .. }
                        )
                    {
                        frame.add(6).write_volatile(0); // Return success
                    }
                }
                4 => {
                    // Syscall 4: Unlock Mutex
                    let cap_idx = frame.add(6).read_volatile(); // a0
                    extern "Rust" {
                        static mut SCHEDULERS: [scheduler::bitmap::BitMapScheduler; 2];
                        static mut MUTEXES: [Option<crate::KernelMutex>; 8];
                        static MUTEX_LOCK: crate::kernel::spinlock::Spinlock;
                        static SCHED_LOCK: crate::kernel::spinlock::Spinlock;
                    }
                    let _sched_guard = SCHED_LOCK.lock_guard(hart_id);
                    let scheds = &mut *core::ptr::addr_of_mut!(SCHEDULERS);
                    let [ref mut sched0, ref mut sched1] = *scheds;
                    let running_idx = if hart_id == 0 {
                        sched0.current_partition_idx
                    } else {
                        sched1.current_partition_idx
                    };

                    let mut permission_denied = false;
                    let mut unlock_success = false;

                    MUTEX_LOCK.lock(hart_id);

                    if cap_idx < 8 {
                        let cap = if hart_id == 0 {
                            sched0.task_table[running_idx]
                                .as_ref()
                                .unwrap()
                                .capabilities[cap_idx]
                        } else {
                            sched1.task_table[running_idx]
                                .as_ref()
                                .unwrap()
                                .capabilities[cap_idx]
                        };
                        if let scheduler::Capability::Mutex {
                            mutex_idx,
                            can_unlock: true,
                            ..
                        } = cap
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
                                            // Look up waiter on its registered core
                                            let w_prio = if w < 2 {
                                                sched0.task_table[w].as_ref().unwrap().priority
                                            } else {
                                                sched1.task_table[w].as_ref().unwrap().priority
                                            };
                                            let i_prio = if i < 2 {
                                                sched0.task_table[i].as_ref().unwrap().priority
                                            } else {
                                                sched1.task_table[i].as_ref().unwrap().priority
                                            };
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

                                    // Identify which core the waiter resides on
                                    // Watchdog (0) & Task A (1) are on Core 0. Task B (2) & Task C (3) on Core 1.
                                    let waiter_hart = if waiter_idx < 2 { 0 } else { 1 };

                                    // Unblock the waiter in its core-local scheduler
                                    let waiter_tcb = if waiter_hart == 0 {
                                        sched0.task_table[waiter_idx].as_mut().unwrap()
                                    } else {
                                        sched1.task_table[waiter_idx].as_mut().unwrap()
                                    };
                                    waiter_tcb.state = scheduler::TaskState::Ready;

                                    // Write 0 to waiter's saved frame a0 to indicate success
                                    let waiter_frame = waiter_tcb.saved_sp as *mut usize;
                                    waiter_frame.add(6).write_volatile(0);

                                    MUTEX_LOCK.unlock();

                                    // Signal other core via CLINT Software Interrupt if the waiter is cross-core
                                    if waiter_hart != hart_id {
                                        let msip = (0x0200_0000 + waiter_hart * 4) as *mut u32;
                                        msip.write_volatile(1);
                                    }
                                } else {
                                    mutex.locked = false;
                                    mutex.owner_task_idx = None;
                                    MUTEX_LOCK.unlock();
                                }
                                unlock_success = true;

                                let sched = if hart_id == 0 { sched0 } else { sched1 };
                                switch_task(sched, &mut current_sp, false);
                            } else {
                                MUTEX_LOCK.unlock();
                                permission_denied = true;
                            }
                        } else {
                            MUTEX_LOCK.unlock();
                            permission_denied = true;
                        }
                    } else {
                        MUTEX_LOCK.unlock();
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
                        static mut SCHEDULERS: [scheduler::bitmap::BitMapScheduler; 2];
                        static SCHED_LOCK: crate::kernel::spinlock::Spinlock;
                    }
                    let _sched_guard = SCHED_LOCK.lock_guard(hart_id);
                    let scheds = &mut *core::ptr::addr_of_mut!(SCHEDULERS);
                    let running_idx = scheds[hart_id].current_partition_idx;
                    let current_tick = TICK_COUNT.load(Ordering::Relaxed);
                    let checkin_slot = running_idx;
                    LAST_CHECKIN_TICK[checkin_slot] = current_tick;
                }
                6 => {
                    // Syscall 6: Send IPC (Synchronous Rendezvous)
                    let cap_idx = frame.add(6).read_volatile(); // a0
                    let msg_addr = frame.add(7).read_volatile(); // a1
                    let msg_len = frame.add(8).read_volatile(); // a2

                    extern "Rust" {
                        static mut SCHEDULERS: [scheduler::bitmap::BitMapScheduler; 2];
                        static SCHED_LOCK: crate::kernel::spinlock::Spinlock;
                    }
                    let _sched_guard = SCHED_LOCK.lock_guard(hart_id);
                    let scheds = &mut *core::ptr::addr_of_mut!(SCHEDULERS);
                    let [ref mut sched0, ref mut sched1] = *scheds;
                    let running_idx = if hart_id == 0 {
                        sched0.current_partition_idx
                    } else {
                        sched1.current_partition_idx
                    };

                    let mut permission_denied = false;

                    if cap_idx < 8 {
                        let cap = if hart_id == 0 {
                            sched0.task_table[running_idx]
                                .as_ref()
                                .unwrap()
                                .capabilities[cap_idx]
                        } else {
                            sched1.task_table[running_idx]
                                .as_ref()
                                .unwrap()
                                .capabilities[cap_idx]
                        };
                        if let scheduler::Capability::Ipc {
                            endpoint_idx,
                            can_send: true,
                            ..
                        } = cap
                        {
                            // Search across BOTH cores' schedulers for a waiting receiver on this endpoint
                            let mut found_receiver: Option<(usize, usize)> = None; // (hart_id, task_idx)
                            for h in 0..2 {
                                for i in 0..scheduler::bitmap::MAX_PARTITIONS {
                                    let other_tcb = if h == 0 {
                                        &sched0.task_table[i]
                                    } else {
                                        &sched1.task_table[i]
                                    };
                                    if let Some(other_tcb) = other_tcb {
                                        if let scheduler::TaskState::BlockedOnIpcRecv {
                                            endpoint_idx: rx_ep,
                                            ..
                                        } = other_tcb.state
                                        {
                                            if rx_ep == endpoint_idx {
                                                found_receiver = Some((h, i));
                                                break;
                                            }
                                        }
                                    }
                                }
                                if found_receiver.is_some() {
                                    break;
                                }
                            }

                            if let Some((rx_hart, rx_idx)) = found_receiver {
                                // Rendezvous Met! Transfer data directly.
                                let receiver_tcb = if rx_hart == 0 {
                                    sched0.task_table[rx_idx].as_mut().unwrap()
                                } else {
                                    sched1.task_table[rx_idx].as_mut().unwrap()
                                };

                                if let scheduler::TaskState::BlockedOnIpcRecv {
                                    buf_addr,
                                    max_len,
                                    ..
                                } = receiver_tcb.state
                                {
                                    let transfer_len = core::cmp::min(msg_len, max_len);

                                    core::ptr::copy_nonoverlapping(
                                        msg_addr as *const u8,
                                        buf_addr as *mut u8,
                                        transfer_len,
                                    );

                                    // Set receiver's state to Ready
                                    receiver_tcb.state = scheduler::TaskState::Ready;

                                    // Write transfer_len (number of bytes received) to receiver's saved frame a0
                                    let rx_frame = receiver_tcb.saved_sp as *mut usize;
                                    rx_frame.add(6).write_volatile(transfer_len);

                                    // Write 0 (success) to sender's frame a0
                                    frame.add(6).write_volatile(0);

                                    telemetry::log_telemetry(&telemetry::TraceEvent::IpcTransfer {
                                        endpoint: endpoint_idx,
                                        bytes: transfer_len as u8,
                                    });

                                    // Trigger software interrupt if receiver is on the other core
                                    if rx_hart != hart_id {
                                        let msip = (0x0200_0000 + rx_hart * 4) as *mut u32;
                                        msip.write_volatile(1);
                                    }
                                }
                            } else {
                                // No receiver waiting: block the sender
                                let sender_tcb = if hart_id == 0 {
                                    sched0.task_table[running_idx].as_mut().unwrap()
                                } else {
                                    sched1.task_table[running_idx].as_mut().unwrap()
                                };
                                sender_tcb.state = scheduler::TaskState::BlockedOnIpcSend {
                                    endpoint_idx,
                                    msg_addr,
                                    msg_len,
                                };

                                // Reschedule since current task is now blocked
                                let sched = if hart_id == 0 { sched0 } else { sched1 };
                                switch_task(sched, &mut current_sp, false);
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
                        static mut SCHEDULERS: [scheduler::bitmap::BitMapScheduler; 2];
                        static SCHED_LOCK: crate::kernel::spinlock::Spinlock;
                    }
                    let _sched_guard = SCHED_LOCK.lock_guard(hart_id);
                    let scheds = &mut *core::ptr::addr_of_mut!(SCHEDULERS);
                    let [ref mut sched0, ref mut sched1] = *scheds;
                    let running_idx = if hart_id == 0 {
                        sched0.current_partition_idx
                    } else {
                        sched1.current_partition_idx
                    };

                    let mut permission_denied = false;

                    if cap_idx < 8 {
                        let cap = if hart_id == 0 {
                            sched0.task_table[running_idx]
                                .as_ref()
                                .unwrap()
                                .capabilities[cap_idx]
                        } else {
                            sched1.task_table[running_idx]
                                .as_ref()
                                .unwrap()
                                .capabilities[cap_idx]
                        };
                        if let scheduler::Capability::Ipc {
                            endpoint_idx,
                            can_recv: true,
                            ..
                        } = cap
                        {
                            // Search across BOTH cores' schedulers for a waiting sender on this endpoint
                            let mut found_sender: Option<(usize, usize)> = None; // (hart_id, task_idx)
                            for h in 0..2 {
                                for i in 0..scheduler::bitmap::MAX_PARTITIONS {
                                    let other_tcb = if h == 0 {
                                        &sched0.task_table[i]
                                    } else {
                                        &sched1.task_table[i]
                                    };
                                    if let Some(other_tcb) = other_tcb {
                                        if let scheduler::TaskState::BlockedOnIpcSend {
                                            endpoint_idx: tx_ep,
                                            ..
                                        } = other_tcb.state
                                        {
                                            if tx_ep == endpoint_idx {
                                                found_sender = Some((h, i));
                                                break;
                                            }
                                        }
                                    }
                                }
                                if found_sender.is_some() {
                                    break;
                                }
                            }

                            if let Some((tx_hart, tx_idx)) = found_sender {
                                // Rendezvous Met! Transfer data directly.
                                let sender_tcb = if tx_hart == 0 {
                                    sched0.task_table[tx_idx].as_mut().unwrap()
                                } else {
                                    sched1.task_table[tx_idx].as_mut().unwrap()
                                };

                                if let scheduler::TaskState::BlockedOnIpcSend {
                                    msg_addr,
                                    msg_len,
                                    ..
                                } = sender_tcb.state
                                {
                                    let transfer_len = core::cmp::min(msg_len, max_len);

                                    core::ptr::copy_nonoverlapping(
                                        msg_addr as *const u8,
                                        buf_addr as *mut u8,
                                        transfer_len,
                                    );

                                    // Set sender's state to Ready
                                    sender_tcb.state = scheduler::TaskState::Ready;

                                    // Write 0 (success) to sender's saved frame a0
                                    let tx_frame = sender_tcb.saved_sp as *mut usize;
                                    tx_frame.add(6).write_volatile(0);

                                    // Write transfer_len (number of bytes received) to receiver's frame a0
                                    frame.add(6).write_volatile(transfer_len);

                                    telemetry::log_telemetry(&telemetry::TraceEvent::IpcTransfer {
                                        endpoint: endpoint_idx,
                                        bytes: transfer_len as u8,
                                    });

                                    // Trigger software interrupt if sender is on the other core
                                    if tx_hart != hart_id {
                                        let msip = (0x0200_0000 + tx_hart * 4) as *mut u32;
                                        msip.write_volatile(1);
                                    }
                                }
                            } else {
                                // No sender waiting: block the receiver
                                let receiver_tcb = if hart_id == 0 {
                                    sched0.task_table[running_idx].as_mut().unwrap()
                                } else {
                                    sched1.task_table[running_idx].as_mut().unwrap()
                                };
                                receiver_tcb.state = scheduler::TaskState::BlockedOnIpcRecv {
                                    endpoint_idx,
                                    buf_addr,
                                    max_len,
                                };

                                // Reschedule since current task is now blocked
                                let sched = if hart_id == 0 { sched0 } else { sched1 };
                                switch_task(sched, &mut current_sp, false);
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
                8 => {
                    // Syscall 8: Terminate Task (takes task priority/index to terminate in a0)
                    let target_prio = frame.add(6).read_volatile();

                    extern "Rust" {
                        static mut SCHEDULERS: [scheduler::bitmap::BitMapScheduler; 2];
                        static mut MUTEXES: [Option<crate::KernelMutex>; 8];
                        static MUTEX_LOCK: crate::kernel::spinlock::Spinlock;
                        static SCHED_LOCK: crate::kernel::spinlock::Spinlock;
                    }
                    let _sched_guard = SCHED_LOCK.lock_guard(hart_id);
                    let scheds = &mut *core::ptr::addr_of_mut!(SCHEDULERS);
                    let [ref mut sched0, ref mut sched1] = *scheds;

                    let mut found_hart = 0;
                    let mut terminated_name: Option<&'static str> = None;
                    if target_prio < scheduler::bitmap::MAX_PARTITIONS {
                        if let Some(ref mut tcb) = sched0.task_table[target_prio] {
                            defmt::warn!(
                                "Syscall 8: Terminating task '{}' (priority {})",
                                tcb.name,
                                target_prio
                            );
                            tcb.state = scheduler::TaskState::Terminated;
                            terminated_name = Some(tcb.name);
                            found_hart = 0;
                        } else if let Some(ref mut tcb) = sched1.task_table[target_prio] {
                            defmt::warn!(
                                "Syscall 8: Terminating task '{}' (priority {})",
                                tcb.name,
                                target_prio
                            );
                            tcb.state = scheduler::TaskState::Terminated;
                            terminated_name = Some(tcb.name);
                            found_hart = 1;
                        }
                    }

                    if let Some(name) = terminated_name {
                        // Mutex recovery
                        MUTEX_LOCK.lock(hart_id);
                        let mutexes = &mut *core::ptr::addr_of_mut!(MUTEXES);
                        for (i, mutex_opt) in mutexes.iter_mut().enumerate() {
                            if let Some(mutex) = mutex_opt {
                                if mutex.owner_task_idx == Some(target_prio as u8) {
                                    defmt::warn!(
                                        "Releasing Mutex {} owned by terminated task '{}'",
                                        i,
                                        name
                                    );
                                    mutex.locked = false;
                                    mutex.owner_task_idx = None;

                                    // Wake up highest priority waiter if any
                                    let mut highest_waiter: Option<usize> = None;
                                    for w in 0..32 {
                                        if (mutex.waiters_bitmap & (1 << w)) != 0 {
                                            if let Some(curr_w) = highest_waiter {
                                                let w_prio = if w < 2 {
                                                    sched0.task_table[w].as_ref().unwrap().priority
                                                } else {
                                                    sched1.task_table[w].as_ref().unwrap().priority
                                                };
                                                let curr_w_prio = if curr_w < 2 {
                                                    sched0.task_table[curr_w]
                                                        .as_ref()
                                                        .unwrap()
                                                        .priority
                                                } else {
                                                    sched1.task_table[curr_w]
                                                        .as_ref()
                                                        .unwrap()
                                                        .priority
                                                };
                                                if w_prio < curr_w_prio {
                                                    highest_waiter = Some(w);
                                                }
                                            } else {
                                                highest_waiter = Some(w);
                                            }
                                        }
                                    }

                                    if let Some(w) = highest_waiter {
                                        mutex.locked = true;
                                        mutex.owner_task_idx = Some(w as u8);
                                        mutex.waiters_bitmap &= !(1 << w);

                                        if w < 2 {
                                            if let Some(waiter_tcb) = &mut sched0.task_table[w] {
                                                waiter_tcb.state = scheduler::TaskState::Ready;
                                            }
                                        } else if let Some(waiter_tcb) = &mut sched1.task_table[w] {
                                            waiter_tcb.state = scheduler::TaskState::Ready;
                                        }
                                    }
                                }
                            }
                        }
                        MUTEX_LOCK.unlock();

                        frame.add(6).write_volatile(0);

                        if found_hart != hart_id {
                            let msip = (0x0200_0000 + found_hart * 4) as *mut u32;
                            msip.write_volatile(1);
                        } else {
                            let active_idx = if hart_id == 0 {
                                sched0.current_partition_idx
                            } else {
                                sched1.current_partition_idx
                            };
                            if target_prio == active_idx {
                                let sched = if hart_id == 0 { sched0 } else { sched1 };
                                switch_task(sched, &mut current_sp, false);
                            }
                        }
                    } else {
                        frame.add(6).write_volatile(-1isize as usize);
                    }
                }
                _ => {
                    defmt::warn!("Unhandled syscall ID: {}", syscall_id);
                }
            }
        }
        cause => {
            if cause == 2 {
                let frame = current_sp as *mut usize;
                let mepc = unsafe { frame.add(28).read_volatile() };
                let instruction = unsafe { core::ptr::read_unaligned(mepc as *const u32) };

                // Emulate CSR access to cycle (0xC00) or mcycle (0xB00) to allow
                // U-mode cycle profiling without a full trap round-trip.
                // mstatus (0x300) is deliberately NOT emulated here: it's a
                // genuine privilege violation for U-mode code to touch, and
                // must fall through to the containment path below rather
                // than being silently absorbed.
                let csr = instruction >> 20;
                if (instruction & 0x7F) == 0x73 && (csr == 0xC00 || csr == 0xB00) {
                    let rd_idx = (instruction >> 7) & 0x1F;
                    // Return actual hardware cycles read in M-mode
                    let val: usize = {
                        let cycles: usize;
                        unsafe {
                            core::arch::asm!("csrr {}, mcycle", out(reg) cycles);
                        }
                        cycles
                    };

                    let frame_idx = match rd_idx {
                        1 => Some(0),
                        5 => Some(1),
                        6 => Some(2),
                        7 => Some(3),
                        8 => Some(4),
                        9 => Some(5),
                        10 => Some(6),
                        11 => Some(7),
                        12 => Some(8),
                        13 => Some(9),
                        14 => Some(10),
                        15 => Some(11),
                        16 => Some(12),
                        17 => Some(13),
                        18 => Some(14),
                        19 => Some(15),
                        20 => Some(16),
                        21 => Some(17),
                        22 => Some(18),
                        23 => Some(19),
                        24 => Some(20),
                        25 => Some(21),
                        26 => Some(22),
                        27 => Some(23),
                        28 => Some(24),
                        29 => Some(25),
                        30 => Some(26),
                        31 => Some(27),
                        _ => None,
                    };
                    if let Some(idx) = frame_idx {
                        unsafe {
                            frame.add(idx).write_volatile(val);
                        }
                    }
                    unsafe {
                        frame.add(28).write_volatile(mepc + 4);
                    }
                    return current_sp;
                }
            }

            // Causes 1/5/7 are PMP access faults, 2 is an illegal instruction/CSR
            // privilege violation, and 4/6 are misaligned load/store addresses.
            // All are synchronous faults attributable to the currently running
            // task, so all are contained the same way: terminate the task,
            // recover any mutex it held, and reschedule a healthy one rather
            // than letting an unrecognized cause panic the whole core.
            if cause == 1 || cause == 2 || cause == 4 || cause == 5 || cause == 6 || cause == 7 {
                let frame = current_sp as *mut usize;
                let mepc = unsafe { frame.add(28).read_volatile() };
                telemetry::log_telemetry(&telemetry::TraceEvent::FaultInterception {
                    cause: cause as u32,
                    pc: mepc as u32,
                });

                if cause != 2 {
                    crate::kernel::metrics::METRIC_PMP_VIOLATIONS.fetch_add(1, Ordering::Relaxed);
                }

                // Declare the statics
                extern "Rust" {
                    static mut SCHEDULERS: [scheduler::bitmap::BitMapScheduler; 2];
                    static mut MUTEXES: [Option<crate::KernelMutex>; 8];
                    static MUTEX_LOCK: crate::kernel::spinlock::Spinlock;
                    static SCHED_LOCK: crate::kernel::spinlock::Spinlock;
                }
                let _sched_guard = unsafe { SCHED_LOCK.lock_guard(hart_id) };

                let running_idx;
                let mut task_terminated = false;
                let mut terminated_task_name: Option<&'static str> = None;

                // Scope 1: Terminate the task and identify running index and task name
                {
                    let scheds = unsafe { &mut *core::ptr::addr_of_mut!(SCHEDULERS) };
                    let sched = &mut scheds[hart_id];
                    running_idx = sched.current_partition_idx;

                    if let Some(tcb) = &mut sched.task_table[running_idx] {
                        let cause_str = match cause {
                            1 => "Instruction Access Fault",
                            2 => "Illegal Instruction",
                            4 => "Load Address Misaligned",
                            5 => "Load Access Fault",
                            6 => "Store Address Misaligned",
                            7 => "Store Access Fault",
                            _ => "Unknown Exception",
                        };
                        let frame = current_sp as *const usize;
                        let mepc = unsafe { frame.add(28).read_volatile() };
                        defmt::error!(
                            "SECURITY FAULT: Task '{}' triggered exception '{}' (cause: {}) at PC: 0x{:X}. Terminating task.",
                            tcb.name,
                            cause_str,
                            cause,
                            mepc
                        );
                        tcb.state = scheduler::TaskState::Terminated;
                        terminated_task_name = Some(tcb.name);
                        task_terminated = true;
                    }
                }

                // Scope 2: Mutex recovery and waker logic
                if task_terminated {
                    unsafe {
                        MUTEX_LOCK.lock(hart_id);
                        let mutexes = &mut *core::ptr::addr_of_mut!(MUTEXES);
                        let scheds = &mut *core::ptr::addr_of_mut!(SCHEDULERS);
                        for (i, mutex_opt) in mutexes.iter_mut().enumerate() {
                            if let Some(mutex) = mutex_opt {
                                if mutex.owner_task_idx == Some(running_idx as u8) {
                                    if let Some(name) = terminated_task_name {
                                        defmt::warn!(
                                            "Releasing Mutex {} owned by terminated task '{}'",
                                            i,
                                            name
                                        );
                                    } else {
                                        defmt::warn!(
                                            "Releasing Mutex {} owned by terminated task idx {}",
                                            i,
                                            running_idx
                                        );
                                    }
                                    mutex.locked = false;
                                    mutex.owner_task_idx = None;

                                    // Wake up highest priority waiter if any
                                    let mut highest_waiter: Option<usize> = None;
                                    for w in 0..32 {
                                        if (mutex.waiters_bitmap & (1 << w)) != 0 {
                                            if let Some(curr_w) = highest_waiter {
                                                let w_prio = if w < 2 {
                                                    scheds[0].task_table[w]
                                                        .as_ref()
                                                        .unwrap()
                                                        .priority
                                                } else {
                                                    scheds[1].task_table[w]
                                                        .as_ref()
                                                        .unwrap()
                                                        .priority
                                                };
                                                let curr_w_prio = if curr_w < 2 {
                                                    scheds[0].task_table[curr_w]
                                                        .as_ref()
                                                        .unwrap()
                                                        .priority
                                                } else {
                                                    scheds[1].task_table[curr_w]
                                                        .as_ref()
                                                        .unwrap()
                                                        .priority
                                                };
                                                if w_prio < curr_w_prio {
                                                    highest_waiter = Some(w);
                                                }
                                            } else {
                                                highest_waiter = Some(w);
                                            }
                                        }
                                    }

                                    if let Some(w) = highest_waiter {
                                        mutex.locked = true;
                                        mutex.owner_task_idx = Some(w as u8);
                                        mutex.waiters_bitmap &= !(1 << w);

                                        // Set state of the waiter task to Ready
                                        if w < 2 {
                                            if let Some(waiter_tcb) = &mut scheds[0].task_table[w] {
                                                waiter_tcb.state = scheduler::TaskState::Ready;
                                            }
                                        } else if let Some(waiter_tcb) =
                                            &mut scheds[1].task_table[w]
                                        {
                                            waiter_tcb.state = scheduler::TaskState::Ready;
                                        }
                                    }
                                }
                            }
                        }
                        MUTEX_LOCK.unlock();
                    }
                }

                // Scope 3: Reschedule to run a healthy task
                {
                    let scheds = unsafe { &mut *core::ptr::addr_of_mut!(SCHEDULERS) };
                    let sched = &mut scheds[hart_id];
                    switch_task(sched, &mut current_sp, false);
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

fn switch_task(
    sched: &mut scheduler::bitmap::BitMapScheduler,
    current_sp: &mut usize,
    is_tick: bool,
) {
    let from = sched.current_partition_idx;
    if let Some((old_sp_ptr, new_sp)) = sched.schedule(is_tick) {
        let to = sched.current_partition_idx;
        let cycles: u32;
        #[cfg(not(kani))]
        unsafe {
            core::arch::asm!("csrr {0}, cycle", out(reg) cycles);
        }
        #[cfg(kani)]
        {
            cycles = 0;
        }
        telemetry::log_telemetry(&telemetry::TraceEvent::TaskSwap {
            from: from as u8,
            to: to as u8,
            cycles,
        });

        unsafe {
            old_sp_ptr.write_volatile(*current_sp);
            *current_sp = new_sp;
            let active_idx = sched.current_partition_idx;
            if let Some(new_tcb) = &sched.task_table[active_idx] {
                crate::memory::reprogram_pmp_stack(new_tcb.name);
            }
        }
    }
}
