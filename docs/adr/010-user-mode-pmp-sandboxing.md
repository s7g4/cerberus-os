# ADR-010: User-Mode Task Promotion & PMP Stack Sandboxing

## Context
In safety-critical systems, task memory space isolation is essential to prevent a fault (such as a stack overflow or buffer corruption) in one task from corrupting the memory of another task. 

Running tasks in Machine Mode (M-Mode) prevents dynamic context-sensitive memory sandboxing:
1. PMP entries only apply to M-Mode if they are permanently locked (`L` bit set).
2. Locked PMP entries cannot be dynamically modified at runtime.
3. Unlocked PMP entries do not apply to M-Mode.

To isolate tasks dynamically, tasks must execute in a lower privilege mode where unlocked (and therefore runtime-modifiable) PMP entries apply.

## Decision
1. **User Mode (U-Mode)**: Promote task execution to U-Mode. dropping privilege levels at startup using the `mret` instruction with `mstatus.MPP` set to `0`.
2. **Dynamic PMP Stack Masking**: Reprogram PMP Entry 1 on every context switch to block access to the *inactive* task's stack bounds using Naturally Aligned Power of Two (NAPOT) address mode, while allowing Entry 2 (global RAM read/write) to remain open.
3. **M-Mode Kernel Stack (`mscratch`)**: Use the `mscratch` register to swap to a dedicated Machine-Mode interrupt stack on trap entry. This ensures the kernel trap handler runs on safe memory, isolated from U-Mode stack overruns.
4. **Syscall Interface (`ecall`)**: Implement standard RISC-V environment call (`ecall`) trap routing (cause 8) to handle cooperative yields and OS services on behalf of U-Mode tasks.

## Consequences
- **Pros**:
  - Enforces strict hardware-level spatial isolation between tasks. A memory violation in Task A triggers an immediate CPU exception rather than corrupting Task B.
  - Protects the kernel exception handler from being crashed by user task stack overflows.
  - Implements a standard, secure microkernel syscall architecture.
- **Cons**:
  - Introduces register saving and syscall exception overhead (from the `ecall` trap) for cooperative yields.
