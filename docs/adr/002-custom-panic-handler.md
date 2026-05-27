# ADR-002: Custom defmt-based Panic Handler for RISC-V

## Context
Embedded Rust projects often use `panic-probe` to report crashes. However, `panic-probe` is hardcoded to support ARM Cortex-M architecture, checking the target at compilation time and failing on RISC-V targets.

## Decision
We implement a custom `#[panic_handler]` in `src/main.rs`. This handler formats the panic location and payload using `defmt::Debug2Format` and prints it over the RTT stream, then executes an infinite loop containing the `wfi` (Wait For Interrupt) instruction.

## Consequences
- **Pros**:
  - Compiles natively on any bare-metal target (including RISC-V).
  - Eliminates external dependency on ARM-specific crates.
  - Retains detailed logging capabilities over JTAG/RTT.
- **Cons**:
  - We do not get automatic frame pointer stack unwinding unless we write a custom unwinder in assembly later.
