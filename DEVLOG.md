# DEVLOG — Cerberus-OS

## Day 1 — Environment Setup
- **Goal**: Establish a reproducible build and compilation environment for RISC-V M-mode.
- **What Broke & How it Was Fixed**:
  - *No breaks yet*. Ensured tools (`probe-rs`, `flip-link`, `cargo-binutils`) are installed.
- **Time Log**:
  - Environment tool installations: 30m
  - Writing configuration files: 20m
  - Researching RISC-V M-mode & flip-link: 1h
- **Metric Captured**:
  - Toolchain target `riscv32imac-unknown-none-elf` installed successfully.

## Day 2 — Kernel Skeleton
- **Goal**: Implement a minimal valid kernel entry point that compiles, links, and is boot-observable via RTT.
- **What Broke & How it Was Fixed**:
  - *Issue 1*: Compiling `panic-probe` failed on RISC-V since the crate is Cortex-M specific.
    - *Fix*: Removed `panic-probe` and implemented a custom, bare-metal `#[panic_handler]` in `src/main.rs` that prints over `defmt-rtt` and halts with `wfi`.
  - *Issue 2*: Linker failed with `memory region not defined: REGION_TEXT`.
    - *Fix*: Added a local `build.rs` to copy `memory.x` to the build output directory and modified `.cargo/config.toml` to explicitly pass `-Tmemory.x` to the linker.
  - *Issue 3*: Linker failed with `undefined symbol: _critical_section_1_0_acquire` and `_critical_section_1_0_release` due to `defmt-rtt` dependency.
    - *Fix*: Enabled the `critical-section-single-hart` feature for the `riscv` dependency in `Cargo.toml` to provide the bare-metal interrupt-disabling implementation.
- **Time Log**:
  - Solving `panic-probe` and writing custom panic handler: 30m
  - Writing `build.rs` and fixing `memory.x` linker flags: 45m
  - Resolving `critical-section` undefined symbols: 20m
  - Measuring metrics: 10m
- **Metric Captured**:
  - Measured `.text` (10,246 bytes) and `.bss` (8 bytes) size using `cargo size`.

