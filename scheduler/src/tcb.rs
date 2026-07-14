/// Capability tokens representing permissions to access resources.
#[derive(Debug, Clone, Copy, PartialEq, defmt::Format)]
pub enum Capability {
    /// No capability.
    None,
    /// Permission to access a global Mutex.
    Mutex {
        /// Index of the mutex in the global MUTEXES array.
        mutex_idx: u8,
        /// Permission to lock the mutex.
        can_lock: bool,
        /// Permission to unlock the mutex.
        can_unlock: bool,
    },
    /// Permission to communicate over a synchronous IPC endpoint channel.
    Ipc {
        /// Index of the IPC channel/endpoint.
        endpoint_idx: u8,
        /// Permission to send messages.
        can_send: bool,
        /// Permission to receive messages.
        can_recv: bool,
    },
}

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
    /// Suspended waiting to send a message on an IPC endpoint (synchronous rendezvous).
    BlockedOnIpcSend {
        endpoint_idx: u8,
        msg_addr: usize,
        msg_len: usize,
    },
    /// Suspended waiting to receive a message on an IPC endpoint (synchronous rendezvous).
    BlockedOnIpcRecv {
        endpoint_idx: u8,
        buf_addr: usize,
        max_len: usize,
    },
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
    /// Capability table (C-List) representing local resource access tokens.
    pub capabilities: [Capability; 8],
}

impl TaskControlBlock {
    /// Initialize a task stack with a dummy context frame for U-Mode.
    pub fn initialize_stack(stack: &mut [u8], entry_fn: extern "C" fn() -> !) -> usize {
        let stack_start = stack.as_ptr() as usize;
        let stack_end = stack_start + stack.len();

        // Frame is 32 registers wide; size it in bytes from the actual word
        // width instead of hardcoding 128, so the reservation stays correct
        // if usize's width ever changes (e.g. running host-side unit tests
        // on a 64-bit target).
        let frame_bytes = 32 * core::mem::size_of::<usize>();

        // 16-byte stack alignment (ABI requirement)
        let aligned_sp = (stack_end & !0xF) - frame_bytes;

        let frame = aligned_sp as *mut usize;
        unsafe {
            // Clear the entire frame to zero
            for i in 0..32 {
                frame.add(i).write_volatile(0);
            }

            // Write mepc (word index 28) = entry_fn
            frame.add(28).write_volatile(entry_fn as usize);

            // Write mstatus (word index 29) = 0x80 (MPIE = 1, MPP = U-mode)
            frame.add(29).write_volatile(0x80);
        }

        aligned_sp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn dummy_entry() -> ! {
        loop {}
    }

    #[test]
    fn initialize_stack_returns_16_byte_aligned_sp() {
        let mut stack = [0u8; 256];
        let sp = TaskControlBlock::initialize_stack(&mut stack, dummy_entry);
        assert_eq!(sp % 16, 0);
    }

    #[test]
    fn initialize_stack_writes_entry_fn_as_mepc() {
        let mut stack = [0u8; 512];
        let sp = TaskControlBlock::initialize_stack(&mut stack, dummy_entry);
        let mepc = unsafe { *(sp as *const usize).add(28) };
        assert_eq!(mepc, dummy_entry as usize);
    }

    #[test]
    fn initialize_stack_writes_u_mode_mstatus() {
        let mut stack = [0u8; 512];
        let sp = TaskControlBlock::initialize_stack(&mut stack, dummy_entry);
        let mstatus = unsafe { *(sp as *const usize).add(29) };
        assert_eq!(mstatus, 0x80);
    }
}
