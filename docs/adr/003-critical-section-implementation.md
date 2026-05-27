# ADR-003: Mutual Exclusion via Single-Hart Interrupt Disabling

## Context
Libraries like `defmt-rtt` use the `critical-section` crate to prevent concurrent access to shared resources. In a bare-metal environment, the compiler requires a backend implementation for this mutual exclusion primitive.

## Decision
We enable the `critical-section-single-hart` feature in the `riscv` crate. This provides the implementation by globally disabling interrupts (clearing the `MIE` bit in the `mstatus` CSR) during the critical section and restoring the previous interrupt state afterwards.

## Consequences
- **Pros**:
  - Light-weight, zero-cost mutual exclusion on a single-core system.
  - Compiles natively for ESP32-C3 / QEMU `virt` targets.
- **Cons**:
  - This method is unsound on multi-core (multi-hart) systems. If we migrate to a multi-core chip, we must swap this out for a hardware spinlock-based critical section.
