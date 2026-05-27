# 005: Physical Memory Protection (PMP) Isolation

## Context
In safety-critical embedded systems, it is essential to protect code space and data space from corruption. An unauthorized memory write (e.g. buffer overflow in RAM attempting to overwrite executable instructions) or execution of data (e.g. code injection) must be prevented at the hardware level.

## Decision
We configure the RISC-V Physical Memory Protection (PMP) hardware registers on boot. We define two static memory regions:
1. **Flash (Code) Region**: Read & Execute permissions only. Locked (`L` bit set) to apply to all execution modes including Machine mode (M-mode).
2. **RAM (Data) Region**: Read & Write permissions only. Locked to prevent execution from RAM.

## Consequences
- **Pros**:
  - Enforces a hardware-level W^X (Write XOR Execute) security policy.
  - Prevents code injection attacks in RAM and self-modifying code bugs.
  - Generates an immediate Instruction/Load/Store access fault trap if violated, halting the CPU safely.
- **Cons**:
  - Locked configurations are permanent until hard reset, meaning we cannot modify these regions dynamically during runtime.
