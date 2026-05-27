# 006: Lock-Free Single-Producer Single-Consumer (SPSC) CAN Ring Buffer

## Context
In a real-time kernel, interrupt handlers (ISRs) must execute as quickly as possible. The CAN receive ISR is responsible for reading raw frames from the hardware transceiver and queuing them for processing by task-space threads. Using a standard mutex-locked queue inside the ISR is forbidden, as it introduces lock contention and priority inversion.

## Decision
We implement a lock-free Single-Producer Single-Consumer (SPSC) Ring Buffer. The ISR acts as the single producer (invoking `push()`), and the task space thread acts as the single consumer (invoking `pop()`). The buffer is pre-allocated statically (no heap). Index arithmetic is optimized using a bitwise mask (`index & (CAPACITY - 1)`), which requires the buffer capacity to be a power of two.

## Consequences
- **Pros**:
  - Lock-free synchronization. No critical sections, locks, or interrupt disabling are needed to push or pop.
  - Zero heap allocation.
  - $O(1)$ push and pop times, with index updates executing in a single CPU cycle via bitwise masking.
- **Cons**:
  - Strictly limited to a single producer and single consumer. If multiple tasks attempt to read from the buffer, software lock coordination would be required.
