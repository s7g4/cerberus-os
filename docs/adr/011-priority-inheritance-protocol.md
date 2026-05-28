# ADR-011: Priority Inheritance Protocol (PIP) Mutexes

## Context
In real-time operating systems (RTOS), task synchronization using shared mutex locks can trigger priority inversion bugs. This occurs when a medium-priority CPU-bound task preempts a low-priority lock owner, indirectly blocking a high-priority task waiting on that lock. We require a deterministic synchronization primitive that guarantees bounded blocking times.

## Decision
1. **Kernel Mutexes**: Implement a static registry of shared mutexes (`MUTEXES: [Option<KernelMutex>; 8]`) managed via kernel syscalls.
2. **Syscall Lock/Unlock API**: Expose Syscall `3` (`lock_mutex`) and Syscall `4` (`unlock_mutex`) to U-Mode tasks.
3. **Priority Inheritance Protocol (PIP)**: When a task blocks on a locked mutex, boost the active priority of the lock owner to match the blocked task's priority if it is higher.
4. **O(1) Active Priority Mapping**: Maintain an active-priority-to-task-index lookup mapping array in the scheduler to ensure task selection remains O(1) after priority boosts.
5. **Waiters Bitmap**: Track blocked tasks using a 32-bit `waiters_bitmap` inside each mutex, waking the highest-priority waiter upon release.

## Consequences
- **Pros**:
  - Guarantees bounded blocking times and prevents priority inversion deadlocks.
  - Maintains strict O(1) scheduling time complexity using the lookup array.
  - Thread-safe and ISR-safe lock handovers handled entirely in M-Mode.
- **Cons**:
  - Introduces syscall boundary transition overhead for lock acquisition, but this is required to maintain secure kernel-level scheduling adjustments.
