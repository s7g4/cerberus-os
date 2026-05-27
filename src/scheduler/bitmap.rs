//! O(1) Bitmap Priority Scheduler.

use crate::scheduler::tcb::{TaskControlBlock, TaskState};

pub const MAX_TASKS: usize = 32;

pub struct BitMapScheduler {
    /// Bit N = 1 indicates that the task at priority N is ready to run.
    ready_bitmap: u32,
    /// Task table containing TCBs mapped by priority.
    task_table: [Option<TaskControlBlock>; MAX_TASKS],
    /// Priority of the currently running task.
    current_priority: Option<u8>,
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
        }
    }

    /// Register a task in the scheduler table.
    pub fn register_task(&mut self, tcb: TaskControlBlock) {
        let prio = tcb.priority as usize;
        assert!(prio < MAX_TASKS, "Priority out of bounds");

        if tcb.state == TaskState::Ready {
            self.ready_bitmap |= 1 << prio;
        }
        self.task_table[prio] = Some(tcb);
    }

    /// Get the highest priority ready task in O(1) time.
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
    ///
    /// Returns the old TCB's stack pointer address and the new stack pointer if a switch is needed.
    pub fn schedule(&mut self) -> Option<(*mut usize, usize)> {
        let next_prio = self.next_ready_priority()?;

        // If the highest priority ready task is already running, no context switch is needed.
        if Some(next_prio) == self.current_priority {
            return None;
        }

        let old_prio = self.current_priority.take()?;

        // Put the old task back to Ready state if it was running
        if let Some(old_tcb) = &mut self.task_table[old_prio as usize] {
            if old_tcb.state == TaskState::Running {
                old_tcb.state = TaskState::Ready;
                self.mark_ready(old_prio);
            }
        }

        // Set the new task to Running
        self.current_priority = Some(next_prio);
        self.mark_blocked(next_prio);

        if let Some(new_tcb) = &mut self.task_table[next_prio as usize] {
            new_tcb.state = TaskState::Running;
        }

        let old_sp_ptr = &mut self.task_table[old_prio as usize].as_mut()?.saved_sp as *mut usize;
        let new_sp = self.task_table[next_prio as usize].as_ref()?.saved_sp;

        Some((old_sp_ptr, new_sp))
    }

    /// Bootstraps the stack pointer and registers to launch the first task.
    pub fn start_first_task(&mut self) -> ! {
        let next_prio = self
            .next_ready_priority()
            .expect("No ready tasks registered");
        self.current_priority = Some(next_prio);
        self.mark_blocked(next_prio);

        let new_tcb = self.task_table[next_prio as usize].as_mut().unwrap();
        new_tcb.state = TaskState::Running;

        // Perform the initial stack restore and jump to the task's entry function
        unsafe {
            core::arch::asm!(
                "mv sp, {0}",
                "lw ra, 0(sp)",
                "lw s0, 4(sp)",
                "lw s1, 8(sp)",
                "lw s2, 12(sp)",
                "lw s3, 16(sp)",
                "lw s4, 20(sp)",
                "lw s5, 24(sp)",
                "lw s6, 28(sp)",
                "lw s7, 32(sp)",
                "lw s8, 36(sp)",
                "lw s9, 40(sp)",
                "lw s10, 44(sp)",
                "lw s11, 48(sp)",
                "addi sp, sp, 64",
                "ret",
                in(reg) new_tcb.saved_sp,
                options(noreturn)
            );
        }
    }
}
