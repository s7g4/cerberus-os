//! O(1) Bitmap Priority Scheduler.

#![allow(unused_variables)]

use crate::tcb::{TaskControlBlock, TaskState};

pub const MAX_PARTITIONS: usize = 32;

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
            partition_durations: [100; MAX_PARTITIONS], // Default 100 ticks per MIF
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
            // Pass 1: Find next ready partition in cyclic order (excluding idle partition 31)
            let mut next_idx = (current_idx + 1) % MAX_PARTITIONS;
            let mut found = false;
            for _ in 0..MAX_PARTITIONS {
                if next_idx != 31 {
                    if let Some(tcb) = &self.task_table[next_idx] {
                        if tcb.state == TaskState::Ready {
                            found = true;
                            break;
                        }
                    }
                }
                next_idx = (next_idx + 1) % MAX_PARTITIONS;
            }

            if !found {
                // Pass 2: Fall back to partition 31 (Idle task) if ready
                if let Some(tcb) = &self.task_table[31] {
                    if tcb.state == TaskState::Ready {
                        next_idx = 31;
                        found = true;
                    }
                }
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
}

#[cfg(kani)]
mod verification {
    use super::*;

    // Safely generate a valid symbolic TaskState variant
    fn any_task_state() -> TaskState {
        match kani::any::<u8>() % 5 {
            0 => TaskState::Ready,
            1 => TaskState::Running,
            2 => TaskState::Blocked {
                wake_tick: kani::any::<u32>(),
            },
            3 => TaskState::BlockedOnMutex {
                mutex_idx: kani::any::<u8>(),
            },
            _ => TaskState::Terminated,
        }
    }

    #[kani::proof]
    #[kani::unwind(33)] // Loop runs up to MAX_PARTITIONS (32) times
    fn verify_scheduler_invariants() {
        let mut sched = BitMapScheduler::new();

        // 1. Populate the scheduler task table with symbolic tasks
        for i in 0..MAX_PARTITIONS {
            if kani::any::<bool>() {
                let state = any_task_state();
                let priority = i as u8;
                let tcb = TaskControlBlock {
                    saved_sp: kani::any::<usize>(),
                    priority,
                    active_priority: priority,
                    state,
                    name: "Symbolic Task",
                    capabilities: [crate::tcb::Capability::None; 8],
                };
                sched.task_table[i] = Some(tcb);
            }
        }

        // Generate symbolic scheduler context variables
        let is_tick = kani::any::<bool>();
        let current_partition_idx = kani::any::<usize>() % MAX_PARTITIONS;
        sched.current_partition_idx = current_partition_idx;

        // Assume the current running partition is registered in the task table
        kani::assume(sched.task_table[current_partition_idx].is_some());

        let remaining_mif_ticks = kani::any::<u32>();
        kani::assume(remaining_mif_ticks <= 100);
        sched.remaining_mif_ticks = remaining_mif_ticks;

        // Detect if there is at least one ready task in the system
        let mut has_ready_task = false;
        for i in 0..MAX_PARTITIONS {
            if let Some(tcb) = &sched.task_table[i] {
                if tcb.state == TaskState::Ready
                    || (i == current_partition_idx && tcb.state == TaskState::Running)
                {
                    has_ready_task = true;
                }
            }
        }

        // Call our scheduling algorithm
        let result = sched.schedule(is_tick);

        // 2. Assert Invariants
        if let Some((_old_sp, _new_sp)) = result {
            // Invariant 1: If context switch occurred, the newly selected task MUST be in Running state
            let next_idx = sched.current_partition_idx;
            let next_tcb = sched.task_table[next_idx].as_ref().unwrap();
            assert_eq!(next_tcb.state, TaskState::Running);
        } else {
            // Invariant 2: If we didn't switch, and there was a ready task, assert the current task is still running
            if has_ready_task {
                let current_tcb = sched.task_table[current_partition_idx].as_ref().unwrap();
                assert!(
                    current_tcb.state == TaskState::Running
                        || current_tcb.state == TaskState::Ready
                );
            }
        }
    }
}
