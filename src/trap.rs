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

            // Scan and wake tasks blocked on ticks
            for i in 0..crate::scheduler::bitmap::MAX_TASKS {
                if let Some(tcb) = &mut sched.task_table[i] {
                    if let crate::scheduler::TaskState::Blocked { wake_tick } = tcb.state {
                        if tick >= wake_tick {
                            tcb.state = crate::scheduler::TaskState::Ready;
                            sched.ready_bitmap |= 1 << tcb.active_priority;
                        }
                    }
                }
            }

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
                2 => {
                    // Syscall 2: Sleep Ticks
                    let ticks = frame.add(6).read_volatile(); // a0 is index 6 (offset 24)
                    extern "Rust" {
                        static mut SCHEDULER: crate::scheduler::bitmap::BitMapScheduler;
                    }
                    let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
                    let running_idx = sched.current_priority.unwrap() as usize;

                    let current_tick = TICK_COUNT.load(Ordering::Relaxed);
                    let tcb = sched.task_table[running_idx].as_mut().unwrap();
                    tcb.state = crate::scheduler::TaskState::Blocked {
                        wake_tick: current_tick + ticks as u32,
                    };
                    sched.ready_bitmap &= !(1 << tcb.active_priority);

                    // Reschedule because the current task is now blocked
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
                3 => {
                    // Syscall 3: Lock Mutex
                    let mutex_idx = frame.add(6).read_volatile(); // a0 is index 6 (offset 24)
                    extern "Rust" {
                        static mut SCHEDULER: crate::scheduler::bitmap::BitMapScheduler;
                        static mut MUTEXES: [Option<crate::KernelMutex>; 8];
                    }
                    let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
                    let running_idx = sched.current_priority.unwrap() as usize;

                    if let Some(mutex) = &mut (*core::ptr::addr_of_mut!(MUTEXES))[mutex_idx] {
                        if !mutex.locked {
                            mutex.locked = true;
                            mutex.owner_task_idx = Some(running_idx as u8);
                        } else {
                            // Mutex is locked: block the current task on the mutex
                            sched.task_table[running_idx].as_mut().unwrap().state =
                                crate::scheduler::TaskState::BlockedOnMutex {
                                    mutex_idx: mutex_idx as u8,
                                };

                            let active_prio = sched.task_table[running_idx]
                                .as_ref()
                                .unwrap()
                                .active_priority;
                            sched.ready_bitmap &= !(1 << active_prio);
                            mutex.waiters_bitmap |= 1 << running_idx;

                            // Priority Inheritance Protocol (PIP)
                            let owner_idx = mutex.owner_task_idx.unwrap() as usize;
                            let current_prio =
                                sched.task_table[running_idx].as_ref().unwrap().priority;
                            let owner_active_prio = sched.task_table[owner_idx]
                                .as_ref()
                                .unwrap()
                                .active_priority;

                            if current_prio < owner_active_prio {
                                // Boost the owner's active priority
                                let owner_tcb = sched.task_table[owner_idx].as_mut().unwrap();
                                if owner_tcb.state == crate::scheduler::TaskState::Ready {
                                    sched.ready_bitmap &= !(1 << owner_active_prio);
                                    sched.ready_bitmap |= 1 << current_prio;
                                }
                                owner_tcb.active_priority = current_prio;
                                sched.priority_to_task[current_prio as usize] =
                                    Some(owner_idx as u8);
                                sched.priority_to_task[owner_active_prio as usize] = None;
                                defmt::info!(
                                    "PIP: Boosted Task {} Priority from {} to {}",
                                    owner_tcb.name,
                                    owner_active_prio,
                                    current_prio
                                );
                            }

                            // Reschedule because the current task is blocked
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
                    }
                }
                4 => {
                    // Syscall 4: Unlock Mutex
                    let mutex_idx = frame.add(6).read_volatile(); // a0
                    extern "Rust" {
                        static mut SCHEDULER: crate::scheduler::bitmap::BitMapScheduler;
                        static mut MUTEXES: [Option<crate::KernelMutex>; 8];
                    }
                    let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
                    let running_idx = sched.current_priority.unwrap() as usize;

                    if let Some(mutex) = &mut (*core::ptr::addr_of_mut!(MUTEXES))[mutex_idx] {
                        assert_eq!(mutex.owner_task_idx, Some(running_idx as u8));

                        // Find the highest priority waiter in the bitmap
                        let mut highest_waiter: Option<usize> = None;
                        for i in 0..32 {
                            if (mutex.waiters_bitmap & (1 << i)) != 0 {
                                if let Some(w) = highest_waiter {
                                    let w_prio = sched.task_table[w].as_ref().unwrap().priority;
                                    let i_prio = sched.task_table[i].as_ref().unwrap().priority;
                                    if i_prio < w_prio {
                                        highest_waiter = Some(i);
                                    }
                                } else {
                                    highest_waiter = Some(i);
                                }
                            }
                        }

                        // Restore owner (current task) priority if it was boosted
                        let current_tcb = sched.task_table[running_idx].as_mut().unwrap();
                        let base_prio = current_tcb.priority;
                        let active_prio = current_tcb.active_priority;
                        if active_prio != base_prio {
                            current_tcb.active_priority = base_prio;
                            sched.priority_to_task[base_prio as usize] = Some(running_idx as u8);
                            sched.priority_to_task[active_prio as usize] = None;
                            defmt::info!(
                                "PIP: Restored Task {} Priority from {} to {}",
                                current_tcb.name,
                                active_prio,
                                base_prio
                            );
                        }

                        if let Some(waiter_idx) = highest_waiter {
                            // Transfer ownership to the highest priority waiter
                            mutex.owner_task_idx = Some(waiter_idx as u8);
                            mutex.waiters_bitmap &= !(1 << waiter_idx);

                            // Unblock the waiter
                            let waiter_tcb = sched.task_table[waiter_idx].as_mut().unwrap();
                            waiter_tcb.state = crate::scheduler::TaskState::Ready;
                            let waiter_active = waiter_tcb.active_priority;
                            sched.ready_bitmap |= 1 << waiter_active;
                        } else {
                            mutex.locked = false;
                            mutex.owner_task_idx = None;
                        }

                        // Reschedule to let higher priority unblocked waiter run immediately
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
                }
                5 => {
                    // Syscall 5: Watchdog Check-in
                    extern "Rust" {
                        static mut SCHEDULER: crate::scheduler::bitmap::BitMapScheduler;
                    }
                    let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
                    if let Some(running_idx) = sched.current_priority {
                        let current_tick = TICK_COUNT.load(Ordering::Relaxed);
                        LAST_CHECKIN_TICK[running_idx as usize] = current_tick;
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
                let running_idx = sched.current_priority.unwrap() as usize;

                if let Some(tcb) = &mut sched.task_table[running_idx] {
                    defmt::error!(
                        "SECURITY FAULT: Task '{}' triggered memory access violation (cause: {}). Terminating task.",
                        tcb.name,
                        cause
                    );
                    tcb.state = crate::scheduler::TaskState::Terminated;
                }

                // Reschedule to run a healthy task
                if let Some((old_sp_ptr, new_sp)) = sched.schedule() {
                    old_sp_ptr.write_volatile(current_sp);
                    current_sp = new_sp;
                    if let Some(prio) = sched.current_priority {
                        if let Some(new_tcb) = &sched.task_table[prio as usize] {
                            crate::memory::reprogram_pmp_stack(new_tcb.name);
                        }
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
