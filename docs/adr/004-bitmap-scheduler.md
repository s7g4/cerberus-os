# 004: O(1) Bitmap Priority Scheduler

## Context
A real-time operating system (RTOS) requires scheduling decisions to be made in deterministic, bounded time. Traditional general-purpose operating systems use multi-level feedback queues or linked lists, which exhibit O(N) lookup times (where N is the number of active tasks). This is unacceptable for hard real-time systems (such as automotive safety systems) where scheduler latency must be strictly constant.

## Decision
We implement a priority-based scheduler utilizing a single 32-bit bitmask (`ready_bitmap: u32`) where bit N = 1 indicates that a task at priority N is ready to execute. Task selection is performed using the `trailing_zeros()` hardware instruction, which maps directly to the RISC-V `ctz` instruction.

## Consequences
- **Pros**:
  - Deterministic O(1) task selection. The scheduler decides the next task in exactly the same time regardless of how many tasks are ready.
  - Minimal memory footprint (a single 32-bit integer for state tracking).
  - Direct hardware acceleration on the CPU (executes in 1 cycle on RV32 cores).
- **Cons**:
  - Limits the maximum number of active task priorities to 32 (standard for embedded real-time microcontrollers, but can be scaled to 64 using `u64` or custom arrays if needed).
