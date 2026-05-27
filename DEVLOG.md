# DEVLOG — Cerberus-OS

## Milestone 0 — Environment Setup
- **Goal**: Establish a reproducible build and compilation environment for RISC-V M-mode.
- **What Broke & How it Was Fixed**:
  - *No breaks yet*. Ensured tools (`probe-rs`, `flip-link`, `cargo-binutils`) are installed.
- **Time Log**:
  - Environment tool installations: 30m
  - Writing configuration files: 20m
  - Researching RISC-V M-mode & flip-link: 1h
- **Metric Captured**:
  - Toolchain target `riscv32imac-unknown-none-elf` installed successfully.

## Milestone 1 — Kernel Skeleton
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

## Milestone 2 — Trap Vector & Timer Heartbeat
- **Goal**: Implement the trap handler vector and wire up the hardware timer tick interrupts.
- **What Broke & How it Was Fixed**:
  - *Issue 1*: 64-bit atomics are not supported in hardware on a 32-bit RISC-V target, causing compiler errors when using `AtomicU64`.
    - *Fix*: Swapped the `AtomicU64` tick counter out for `AtomicU32`. A 32-bit counter at 100Hz will last ~497 days before overflowing.
- **Time Log**:
  - Writing low-level assembly trap registers and stack saving: 40m
  - Implementing Rust trap routing: 30m
  - Verifying hardware interrupt signals: 15m
- **Metric Captured**:
  - Heartbeat timer firing successfully.

## Milestone 3 — Context Switch Assembly
- **Goal**: Implement a naked assembler context switcher capable of swapping execution stacks and preserving register context.
- **What Broke & How it Was Fixed**:
  - *No breaks*: Successfully implemented structural representations for `TaskControlBlock` using `#[repr(C)]` and naked register preservation.
- **Time Log**:
  - Designing TCB layouts and memory representations: 20m
  - Writing naked assembly stack switcher: 30m
  - Compiling and checking symbol tables: 15m
- **Metric Captured**:
  - Successfully linked `switch_context` symbol.

## Milestone 4 — O(1) Bitmap Scheduler
- **Goal**: Implement the priority selection bitmap and integrate preemptive task switching inside the timer interrupt.
- **What Broke & How it Was Fixed**:
  - *Issue 1*: Compile error in `switch.rs` stating that `asm!` is prohibited in naked functions.
    - *Fix*: Changed `core::arch::asm!` to the newly stabilized `core::arch::naked_asm!`.
  - *Issue 2*: Compile error stating that `options(noreturn)` is invalid inside `naked_asm!`.
    - *Fix*: Removed the `options(noreturn)` block as `naked_asm!` operates at global scope without parameter qualifiers.
  - *Issue 3*: Mutable static reference warnings for `SCHEDULER` borrows.
    - *Fix*: Replaced direct borrows with raw pointers using `core::ptr::addr_of_mut!` and dereferenced inside `unsafe` blocks to adhere to Rust 2024 specifications.
- **Time Log**:
  - Writing `bitmap.rs` selection logic: 35m
  - Setting up task stacks & initial frame layout: 30m
  - Resolving `naked_asm!` syntax updates: 20m
  - Fixing `static_mut_refs` compiler warnings: 25m
- **Metric Captured**:
  - Built successfully with zero compiler warnings.

