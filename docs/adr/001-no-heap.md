# ADR-001: No Heap Allocation (Static-Only Memory Model)

## Context
In safety-critical embedded systems (e.g., ISO 26262, AUTOSAR), dynamic memory allocation (heap) is highly restricted or completely prohibited. The standard Rust `alloc` crate relies on a global allocator, which introduces non-deterministic execution times, memory fragmentation, and potential Out-Of-Memory (OOM) panic conditions.

## Decision
We prohibit the use of a heap allocator (no `Box`, `Vec`, `HashMap`, or `Rc`). All memory must be allocated statically at compile-time (using `static` memory blocks, stack-allocated variables, or const-generic sizes).

## Consequences
- **Pros**:
  - Deterministic execution time (zero allocation latency).
  - No memory fragmentation.
  - Immunity to Out-Of-Memory (OOM) crashes at runtime.
- **Cons**:
  - Requires pre-sizing all data structures (e.g., ring buffers and task stacks must have maximum sizes resolved at compile-time).
  - RAM usage must be carefully budgeted.
