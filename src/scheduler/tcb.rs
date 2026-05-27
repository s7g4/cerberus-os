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
