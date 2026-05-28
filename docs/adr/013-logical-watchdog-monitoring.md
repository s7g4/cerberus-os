# ADR-013: AUTOSAR-Style Logical Watchdog Thread Monitoring

## Context
In micro-RTOS kernels designed for automotive control systems, tasks must execute within strict temporal and logical boundaries. If an application task hangs (due to infinite loops, deadlocks, or driver stalls) but the CPU continues to execute instructions, traditional hardware watchdogs will not trip as long as the main timer interrupt continues to tick or kick the hardware. We need a software-level logical watchdog manager that supervises individual tasks and triggers a system safe-parking or reset sequence if any task misses its execution deadline.

## Decision
1. **Dedicated Watchdog Task**: Implement a high-priority watchdog task at Priority 0. It executes periodically by using a newly added `sleep_ticks` system call to yield the CPU and wake up after 100 ticks.
2. **Task Check-in Syscall**: Implement a `watchdog_checkin` system call (Syscall 5). Supervised tasks (Task A, Task B) call this system call at the start of their loops to record their current execution timestamp in the global `LAST_CHECKIN_TICK` array.
3. **Temporal Supervision**: The Watchdog Task periodically scans the active tasks. If any task's elapsed time since its last check-in exceeds the allowed timeout limit (200 ticks), the Watchdog Task flags a failure.
4. **Exception Containment**: Task C (Low Priority) is excluded from watchdog supervision once it is terminated by the kernel during the fault-injection test (Phase 13), preventing false positives.
5. **Safe Parking Sequence**: Upon detecting a timeout, the Watchdog Task logs the failure via RTT, prints the final telemetry performance dashboard, disables interrupts globally to prevent further context switching, and safe-parks the CPU in an infinite `wfi` loop.

## Consequences
- **Pros**:
  - Detects logical and temporal software hangs at a task-granular level.
  - Ensures a hung task cannot silently starve or block critical system operations without triggering a safe state transition.
  - Provides a clean shutdown and diagnostic dump (via RTT telemetry) upon failures.
- **Cons**:
  - Increases scheduling overhead due to the periodic execution of the Watchdog Task (negligible at 100-tick intervals).
  - Requires tasks to be designed to check in within the worst-case execution time bounds.
