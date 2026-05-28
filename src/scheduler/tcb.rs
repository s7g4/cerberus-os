//! Task Control Block (TCB) definitions.

/// Task execution states.
#[derive(Debug, Clone, Copy, PartialEq, defmt::Format)]
pub enum TaskState {
    /// Ready for execution.
    Ready,
    /// Currently running on the CPU core.
    Running,
    /// Suspended waiting for timer tick intervals.
    Blocked { wake_tick: u32 },
    /// Suspended waiting for a mutex lock.
    BlockedOnMutex { mutex_idx: u8 },
    /// Execution completed.
    Terminated,
}

/// Task Control Block representing a thread context.
#[repr(C)]
pub struct TaskControlBlock {
    /// Saved stack pointer. Must remain the first field in the struct (offset 0).
    pub saved_sp: usize,
    /// Static base priority of the task. Lower value indicates higher priority.
    pub priority: u8,
    /// Dynamic active priority (inherited during priority inversion).
    pub active_priority: u8,
    /// Current execution state of the task.
    pub state: TaskState,
    /// Debug name of the task.
    pub name: &'static str,
}

impl TaskControlBlock {
    /// Initialize a task stack with a dummy context frame for U-Mode.
    pub fn initialize_stack(stack: &mut [u8], entry_fn: extern "C" fn() -> !) -> usize {
        let stack_start = stack.as_ptr() as usize;
        let stack_end = stack_start + stack.len();

        // 16-byte stack alignment (ABI requirement)
        let aligned_sp = (stack_end & !0xF) - 128;

        let frame = aligned_sp as *mut usize;
        unsafe {
            // Clear the entire 128-byte frame (32 words) to zero
            for i in 0..32 {
                frame.add(i).write_volatile(0);
            }

            // Write mepc (offset 112, word index 28) = entry_fn
            frame.add(28).write_volatile(entry_fn as usize);

            // Write mstatus (offset 116, word index 29) = 0x80 (MPIE = 1, MPP = U-mode)
            frame.add(29).write_volatile(0x80);
        }

        aligned_sp
    }
}
