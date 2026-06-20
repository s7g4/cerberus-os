# ADR 016: Formal Verification via Bounded Model Checking

## Context
In safety-critical kernels (ASIL-D / DO-178C), runtime unit testing cannot guarantee that scheduling and resource management routines are completely free of concurrency bugs or unreachable state transitions. We need mathematical proofs of correctness for our core algorithms.

## Decision
We introduce formal verification to the codebase using the Kani Model Checker. We write symbolic verification harnesses that run on the host target, verifying:
1. **Liveness**: The scheduler never transitions execution context to a non-runnable task state.
2. **Work-Conservation**: If any task is ready, the scheduler will never return a `None` schedule (no phantom CPU hangs).

## Consequences
- **High Assurance**: Code paths are mathematically validated against all symbolic task tables and states up to a loop bound of 32 (`MAX_PARTITIONS`).
- **Zero Overhead**: Proof harnesses are conditionally compiled out of the target RISC-V release binary using `#[cfg(kani)]`.
- **IDE Support**: The custom configuration is registered inside `build.rs` to keep compile diagnostic logs 100% warning-free.
