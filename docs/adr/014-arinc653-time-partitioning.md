# ADR 014: Transition to ARINC 653 Time Partitioning

## Context
Our priority-based bitmap scheduler was vulnerable to temporal starvation and scheduling jitter. In critical embedded systems, we need mathematical guarantees that no partition can consume another partition's execution budget.

## Decision
We replace the priority scheduler with a time-triggered cyclic scheduler executing fixed Minor Frames. The timer interrupt acts as the preemption boundary. During partition swaps, the active PMP stack registers are reprogrammed.

## Consequences
- **Temporal Isolation**: Starvation is impossible; stuck tasks are preempted.
- **Simplification**: Priority Inheritance Protocol (PIP) becomes obsolete and is removed, reducing kernel execution path latency.
- **Determinism**: Execution ordering is statically defined at compile-time.
