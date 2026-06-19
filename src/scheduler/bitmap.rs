//! O(1) Bitmap Priority Scheduler.

use crate::scheduler::tcb::{TaskControlBlock, TaskState};

pub const MAX_PARTITIONS: usize = 4;

pub struct BitMapScheduler {
    /// Task table containing TCBs mapped by partition index.
    pub task_table: [Option<TaskControlBlock>; MAX_PARTITIONS],
    /// Index of the currently running partition.
    pub current_partition_idx: usize,
    /// Static scheduling table (MIF durations in ticks for each partition).
    pub partition_durations: [u32; MAX_PARTITIONS],
    /// Remaining ticks in the current partition's MIF.
    pub remaining_mif_ticks: u32,
}

impl Default for BitMapScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl BitMapScheduler {
    pub const fn new() -> Self {
        Self {
            task_table: [const { None }; MAX_PARTITIONS],
            current_partition_idx: 0,
            partition_durations: [100, 100, 100, 100], // Default 100 ticks per MIF
            remaining_mif_ticks: 100,
        }
    }

    /// Register a task into its designated time partition.
    /// Maps tcb.priority directly to the partition index.
    pub fn register_task(&mut self, mut tcb: TaskControlBlock) {
        let idx = tcb.priority as usize;
        assert!(idx < MAX_PARTITIONS, "Partition index out of bounds");

        tcb.active_priority = tcb.priority; // Maintain base priority mapping
        self.task_table[idx] = Some(tcb);
    }

    /// Runs the partition scheduling algorithm.
    ///
    /// If `is_tick` is true, it decrements the current partition's budget.
    /// If the budget expires or `is_tick` is false (forced yield/block), it swaps partitions.
    pub fn schedule(&mut self, is_tick: bool) -> Option<(*mut usize, usize)> {
        let current_idx = self.current_partition_idx;

        if is_tick && self.remaining_mif_ticks > 0 {
            self.remaining_mif_ticks -= 1;
        }

        // Determine if the current task is still runnable (not blocked or terminated)
        let current_runnable = if let Some(tcb) = &self.task_table[current_idx] {
            tcb.state == TaskState::Running || tcb.state == TaskState::Ready
        } else {
            false
        };

        // Reschedule trigger conditions:
        // 1. Time slice expired (remaining_mif_ticks == 0)
        // 2. Forced reschedule (cooperative yield/block, is_tick == false)
        // 3. Current task is no longer runnable (blocked on mutex/sleeping/terminated)
        if self.remaining_mif_ticks == 0 || !is_tick || !current_runnable {
            // Find the next ready partition in cyclic order
            let mut next_idx = (current_idx + 1) % MAX_PARTITIONS;
            let mut found = false;
            for _ in 0..MAX_PARTITIONS {
                if let Some(tcb) = &self.task_table[next_idx] {
                    if tcb.state == TaskState::Ready {
                        found = true;
                        break;
                    }
                }
                next_idx = (next_idx + 1) % MAX_PARTITIONS;
            }

            if !found {
                // If no other task is ready, fallback to current task if still runnable
                if current_runnable {
                    if self.remaining_mif_ticks == 0 {
                        self.remaining_mif_ticks = self.partition_durations[current_idx];
                    }
                    return None;
                }
                return None; // CPU absolute idle (no ready tasks)
            }

            // Perform partition swap
            self.current_partition_idx = next_idx;
            self.remaining_mif_ticks = self.partition_durations[next_idx];

            // Transition TCB states
            if let Some(old_tcb) = &mut self.task_table[current_idx] {
                if old_tcb.state == TaskState::Running {
                    old_tcb.state = TaskState::Ready;
                }
            }
            if let Some(new_tcb) = &mut self.task_table[next_idx] {
                new_tcb.state = TaskState::Running;
            }

            let old_sp_ptr = &mut self.task_table[current_idx].as_mut()?.saved_sp as *mut usize;
            let new_sp = self.task_table[next_idx].as_ref()?.saved_sp;

            Some((old_sp_ptr, new_sp))
        } else {
            None
        }
    }

    /// Bootstraps the stack pointer and registers to launch the first partition.
    pub fn start_first_task(&mut self) -> ! {
        let mut first_idx = 0;
        let mut found = false;
        for i in 0..MAX_PARTITIONS {
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

        unsafe {
            // 1. Set PMP isolation to block the inactive task stacks
            crate::memory::reprogram_pmp_stack(task_name);

            // 2. Point mscratch to the top of our dedicated Kernel Stack
            let kernel_stack_top = core::ptr::addr_of_mut!(crate::KERNEL_STACK) as usize + 1024;
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
    }
}
