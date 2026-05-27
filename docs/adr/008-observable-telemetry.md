# 008: Atomic Non-Blocking Telemetry Counters

## Context
A real-time kernel requires debugging and monitoring capabilities (observability) to audit scheduling, security events, and hardware exceptions. However, blocking access locks (critical sections) or heavy string operations inside interrupt handlers introduce non-deterministic execution times and degrade scheduler performance.

## Decision
We implement a telemetry metrics module utilizing public atomic integers (`AtomicU32`). Telemetry collection is performed without locks using atomic memory operations (`load`/`store` / `fetch_add` with `Ordering::Relaxed`). Logging to JTAG RTT is isolated to task-space contexts rather than interrupts.

## Consequences
- **Pros**:
  - Telemetry collection has zero lock overhead and executes in constant instruction cycles.
  - Safe for execution inside interrupts (ISRs) and thread-space contexts without priority inversion.
  - Minimal instruction footprints.
- **Cons**:
  - `Ordering::Relaxed` does not enforce global thread ordering memory barriers, but is sufficient for counting non-blocking metrics.
