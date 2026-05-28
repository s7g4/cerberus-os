# ADR-012: Task Fault Isolation & Exception Recovery

## Context
In safety-critical systems, system availability is a core requirement. If a software bug or memory violation occurs in a single task (such as a buffer overflow or illegal pointer dereference), halting the entire operating system via a kernel panic degrades system availability and can be catastrophic (e.g., losing engine telemetry). We require a robust fault recovery mechanism.

## Decision
1. **Fault Catching**: Intercept synchronous RISC-V memory exceptions (Instruction Access Fault `1`, Load Access Fault `5`, and Store Access Fault `7`) in the Machine-Mode `trap_handler`.
2. **Task Termination**: Instead of panicking, change the state of the offending task to `Terminated` and clear its ready bit in the scheduler.
3. **Graceful Reschedule**: resubmit execution to the next highest priority ready task, continuing kernel operations.
4. **Fault Injection Testing**: Verify the isolation boundary by having a low-priority task (Task C) deliberately attempt an unauthorized read of Task A's stack to trigger a Load Access Fault.

## Consequences
- **Pros**:
  - Guarantees high kernel availability; a single task failure cannot halt the operating system.
  - Enforces spatial and temporal fault containment.
- **Cons**:
  - Terminated tasks must be handled or restarted by a supervisor task if they are critical, but task termination is the safest default action to protect kernel memory.
