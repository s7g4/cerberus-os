//! O(1) Bitmap Priority Scheduler.

use crate::scheduler::tcb::{TaskControlBlock, TaskState};

pub const MAX_TASKS: usize = 32;

pub struct BitMapScheduler {
    /// Bit N = 1 indicates that the task at priority N is ready to run.
    pub ready_bitmap: u32,
    /// Task table containing TCBs mapped by priority.
    pub task_table: [Option<TaskControlBlock>; MAX_TASKS],
    /// Priority of the currently running task.
    pub current_priority: Option<u8>,
    /// O(1) active priority to task table index mapping.
    pub priority_to_task: [Option<u8>; MAX_TASKS],
}

impl Default for BitMapScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl BitMapScheduler {
    pub const fn new() -> Self {
        Self {
            ready_bitmap: 0,
            task_table: [const { None }; MAX_TASKS],
            current_priority: None,
            priority_to_task: [const { None }; MAX_TASKS],
        }
    }

    /// Register a task in the scheduler table.
    pub fn register_task(&mut self, mut tcb: TaskControlBlock) {
        let prio = tcb.priority as usize;
        assert!(prio < MAX_TASKS, "Priority out of bounds");

        tcb.active_priority = tcb.priority; // Active priority starts at base

        if tcb.state == TaskState::Ready {
            self.ready_bitmap |= 1 << prio;
        }
        self.priority_to_task[prio] = Some(tcb.priority);
        self.task_table[prio] = Some(tcb);
    }

    /// Get the highest active priority ready task in O(1) time.
    pub fn next_ready_priority(&self) -> Option<u8> {
        if self.ready_bitmap == 0 {
            None
        } else {
            // trailing_zeros compiles to a single hardware 'ctz' instruction on RISC-V
            Some(self.ready_bitmap.trailing_zeros() as u8)
        }
    }

    /// Mark a task priority as ready.
    pub fn mark_ready(&mut self, priority: u8) {
        self.ready_bitmap |= 1 << priority;
    }

    /// Mark a task priority as blocked.
    pub fn mark_blocked(&mut self, priority: u8) {
        self.ready_bitmap &= !(1 << priority);
    }

    /// Runs the scheduling algorithm to select the next task.
    pub fn schedule(&mut self) -> Option<(*mut usize, usize)> {
        let next_active_prio = self.next_ready_priority()?;
        let next_task_idx = self.priority_to_task[next_active_prio as usize]? as usize;

        // If the selected task is already running, no context switch is needed.
        if Some(next_task_idx as u8) == self.current_priority {
            return None;
        }

        let old_task_idx = self.current_priority.take()? as usize;

        // Put the old task back to Ready state if it was running
        if let Some(old_tcb) = &mut self.task_table[old_task_idx] {
            if old_tcb.state == TaskState::Running {
                old_tcb.state = TaskState::Ready;
                let active = old_tcb.active_priority;
                self.mark_ready(active);
            }
        }

        // Set the new task to Running
        self.current_priority = Some(next_task_idx as u8);
        let new_active_prio = self.task_table[next_task_idx].as_ref()?.active_priority;
        self.mark_blocked(new_active_prio);

        if let Some(new_tcb) = &mut self.task_table[next_task_idx] {
            new_tcb.state = TaskState::Running;
        }

        let old_sp_ptr = &mut self.task_table[old_task_idx].as_mut()?.saved_sp as *mut usize;
        let new_sp = self.task_table[next_task_idx].as_ref()?.saved_sp;

        Some((old_sp_ptr, new_sp))
    }

    /// Bootstraps the stack pointer and registers to launch the first task in U-mode.
    pub fn start_first_task(&mut self) -> ! {
        let next_prio = self
            .next_ready_priority()
            .expect("No ready tasks registered");
        self.current_priority = Some(next_prio);
        self.mark_blocked(next_prio);

        let new_tcb = self.task_table[next_prio as usize].as_mut().unwrap();
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

                // Load mepc and mstatus from the frame
                "lw t0, 112(sp)",
                "csrw mepc, t0",
                "lw t1, 116(sp)",
                "csrw mstatus, t1",

                // Restore all user registers
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

                // Deallocate user stack frame
                "addi sp, sp, 128",

                // Drop to User mode
                "mret",
                in(reg) user_sp,
                options(noreturn)
            );
        }
    }
}
