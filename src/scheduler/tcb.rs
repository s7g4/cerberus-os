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
    /// Execution completed.
    Terminated,
}

/// Task Control Block representing a thread context.
///
/// We apply `#[repr(C)]` to guarantee that fields are not reordered or padded
/// by the compiler, allowing the assembly context switcher to read the `saved_sp`
/// field at a fixed offset (0 bytes).
#[repr(C)]
pub struct TaskControlBlock {
    /// Saved stack pointer. Must remain the first field in the struct (offset 0).
    pub saved_sp: usize,
    /// Static priority of the task. Lower value indicates higher priority.
    pub priority: u8,
    /// Current execution state of the task.
    pub state: TaskState,
    /// Debug name of the task.
    pub name: &'static str,
}

impl TaskControlBlock {
    /// Initialize a task stack with a dummy context frame.
    ///
    /// The stack grows downwards, so we calculate the top of the stack,
    /// align it, allocate space for the frame, and populate it.
    pub fn initialize_stack(stack: &mut [u8], entry_fn: extern "C" fn() -> !) -> usize {
        let stack_start = stack.as_ptr() as usize;
        let stack_end = stack_start + stack.len();

        // 16-byte stack alignment (ABI requirement)
        let aligned_sp = (stack_end & !0xF) - 64;

        let frame = aligned_sp as *mut usize;
        unsafe {
            // Store entry function in the ra slot (offset 0)
            frame.write_volatile(entry_fn as usize);

            // Clear callee-saved registers s0-s11 (offsets 4 to 48) to zero
            for i in 1..=12 {
                frame.add(i).write_volatile(0);
            }
        }

        aligned_sp
    }
}
