# ADR-009: Low-Overhead Hardware Performance Benchmarking via `mcycle` CSR

## Context
In a real-time operating system (RTOS), execution latencies for scheduling decisions, network stack operations, and cryptographic operations must remain strictly bounded and verified. Traditional software-based profiling methods (such as serial log statements, operating system timer calls, or instrumentation frameworks) introduce substantial instruction overhead. This alters the timing characteristics of the hot paths under test (the probe effect) and introduces jitter. We require a non-intrusive, cycle-accurate benchmarking mechanism.

## Decision
We utilize the RISC-V Machine Cycle Counter (`mcycle`) Control and Status Register (CSR) to perform direct, low-overhead cycle-accurate profiling:
1. **Direct CSR Access**: Read the `mcycle` register via inline assembly before and after executing critical sections.
2. **Atomic Metric Accumulation**: Store the measured delta cycles in static atomic registers (`AtomicU32`) to prevent locking, interrupt disabling, or buffer contention.
3. **Wrap-Around Arithmetic**: Calculate elapsed cycles using wrapping subtraction (`wrapping_sub()`) to ensure correctness when the 32-bit cycle register overflows, avoiding branch instructions.

## Consequences
- **Pros**:
  - Extremely low overhead: Querying the `mcycle` register completes in a single CPU cycle, minimizing probe distortion.
  - Sub-microsecond precision: Latencies are measured in raw CPU clock cycles rather than microsecond clock ticks.
  - Thread-safe and ISR-safe: Writing metrics to atomic registers avoids mutual exclusion overhead.
- **Cons**:
  - The 32-bit counter wraps around every ~26.8 seconds at 160 MHz. This is sufficient for benchmarking functions taking microseconds but cannot be used for tracking long-term time durations without combining with the `mcycleh` register.
