//! Shared benchmarking helper utilities for Cerberus-OS.

use scheduler::{BitMapScheduler, Capability, TaskControlBlock, TaskState};

/// Helper to set up a scheduler with a given number of registered mock tasks.
pub fn setup_benchmark_scheduler(num_tasks: usize) -> BitMapScheduler {
    let mut sched = BitMapScheduler::new();
    for i in 0..num_tasks {
        let tcb = TaskControlBlock {
            saved_sp: 0x1000 + i * 0x100,
            priority: i as u8,
            active_priority: i as u8,
            state: TaskState::Ready,
            name: "Mock Task",
            capabilities: [Capability::None; 8],
        };
        sched.register_task(tcb);
    }
    sched
}
