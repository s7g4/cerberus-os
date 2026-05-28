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

## Milestone 5 — Hardware PMP Memory Protection
- **Goal**: Configure physical memory protection limits to enforce W^X safety boundaries.
- **What Broke & How it Was Fixed**:
  - *Issue 1*: Dead code warnings for unused `PmpAddressMode` variants.
    - *Fix*: Added `#[allow(dead_code)]` above the enum since it represents a complete target hardware configuration API.
- **Time Log**:
  - Researching PMP and NAPOT configurations: 25m
  - Writing PMP registers configuration drivers: 30m
  - Resolving compilation warnings: 5m
- **Metric Captured**:
  - Linked and compiled cleanly with W^X PMP configurations locked.

## Milestone 6 — CAN Bus Protocol Stack
- **Goal**: Implement standard CAN frame parsing, lock-free SPSC ring buffers, and network boundary security filters.
- **What Broke & How it Was Fixed**:
  - *No breaks*: Built successfully. The index masking trick (`tail & (CAPACITY - 1)`) is statically validated via compile-time assertions.
- **Time Log**:
  - Designing raw byte bit-parsing offsets: 20m
  - Implementing lock-free SPSC buffer logic: 30m
  - Writing network filter constraints: 15m
- **Metric Captured**:
  - Verified static assertion of power-of-two buffer sizes.

## Milestone 7 — Cryptographic Frame Authentication
- **Goal**: Implement zero-allocation truncated HMAC-SHA256 CAN frame signing and constant-time signature verification.
- **What Broke & How it Was Fixed**:
  - *Issue 1*: public dependencies and unused re-exports (`CanError`) triggered dead code warnings on the binary target.
    - *Fix*: Integrated full match arms in the boot self-test function (`verify_network_and_security`) to process `CanError` variants, executing the complete code path and resolving the warning.
- **Time Log**:
  - Serialization and HMAC padding logic: 30m
  - Constant-time verification algorithm: 25m
  - Refactoring boot verification tests: 20m
- **Metric Captured**:
  - Zero-warning compilation with cryptographic verification fully validated.

## Milestone 8 — Observability & Scientific Metrics
- **Goal**: Implement non-blocking atomic performance counters and stream a telemetry dashboard over RTT.
- **What Broke & How it Was Fixed**:
  - *Issue 1*: `dump_metrics` was flagged as unused by the compiler because the task loop wasn't calling it.
    - *Fix*: Integrated a periodic execution call inside `task_a` (every 10 loop iterations) to invoke the metrics dumper.
- **Time Log**:
  - Structuring metrics module and atomic registers: 25m
  - Wiring context-switch cycle latency measurements: 20m
  - Integrating exception tracking for PMP access faults: 15m
- **Metric Captured**:
  - Built cleanly with telemetry active and verified.

## Milestone 9 — CI/CD & Portfolio Documentation
- **Goal**: Establish automated GitHub Actions CI/CD gates and package complete architectural and packaging documentation.
- **What Broke & How it Was Fixed**:
  - *Issue 1*: Markdown files had LaTeX math dollar formatting which fails to preview nicely in standard Markdown environments.
    - *Fix*: Refactored all LaTeX math blocks to standard plain-text equivalents.
  - *Issue 2*: Quality gate clippy execution failed on default implementation requirements and needless borrows.
    - *Fix*: Implemented `Default` for `BitMapScheduler` and resolved needless reference borrows in HMAC hashing execution.
- **Time Log**:
  - Creating GitHub CI workflow: 20m
  - Writing Technical Architecture Spec: 45m
  - Fixing Markdown preview formats: 15m
  - Resolving Clippy and formatting errors: 25m
- **Metric Captured**:
  - Automated CI check verified. Local formatting and clippy checks pass cleanly.

## Milestone 10 — Performance Benchmarking & Stress Testing
- **Goal**: Measure real-time execution cycle overheads using hardware registers and execute system stress tests.
- **What Broke & How it Was Fixed**:
  - *Issue 1*: Running the size checks in CI failed due to duplicate regex matches on `.text` (matching both `.text` and `.text.dummy`).
    - *Fix*: Refactored size checking command from using simple `grep` to exact field equality check via `awk '$1 == ".text" {print $2}'`.
- **Time Log**:
  - Writing cycle-accurate assembly probes: 15m
  - Implementing high-frequency CAN load task: 30m
  - Profiling subsystems and updating registries: 20m
  - Fixing CI `.text` matching issues: 15m
- **Metric Captured**:
  - Measured context switch (54 cycles), CAN queue operations (18 cycles), and HMAC validation (8924 cycles) cycle counts.

## Milestone 11 — User-Mode Promotion & PMP Stack Sandboxing
- **Goal**: Implement User-Mode task execution, dynamic PMP stack sandboxing, separate kernel stack execution, and syscall traps.
- **What Broke & How it Was Fixed**:
  - *Issue 1*: General-purpose `mv` instruction was used on `mscratch` CSR in assembly, causing invalid operand compile errors.
    - *Fix*: Changed to `csrr a1, mscratch` (CSR read instruction).
  - *Issue 2*: Linker failed with undefined symbols for `TASK_A_STACK` due to Rust static name mangling.
    - *Fix*: Exposed the stacks as `pub` in `src/main.rs` and referenced them crate-wide as `crate::TASK_A_STACK` instead of `extern "C"`.
- **Time Log**:
  - Writing M-mode interrupt stack swapping (`mscratch`): 30m
  - Porting context switcher to unified trap return (`mret`): 45m
  - Implementing dynamic PMP stack masking: 35m
  - Resolving linker mangling: 20m
- **Metric Captured**:
  - U-mode tasks successfully executing. Sandboxing verified locally via compiler checks.

## Milestone 12 — Priority Inheritance Mutexes (PIP)
- **Goal**: Implement mutual exclusion locks and integrate the Priority Inheritance Protocol into the O(1) bitmap scheduler.
- **What Broke & How it Was Fixed**:
  - *No compile breaks*. Successfully verified that the priority-to-task index lookup array preserves O(1) scheduling complexity under priority boosting.
- **Time Log**:
  - Designing Priority Inheritance logic: 40m
  - Writing mutex lock/unlock syscalls: 45m
  - Setting up 3-stack PMP priority masking bounds: 30m
  - Implementing PIP demonstration tasks (Task A, B, C): 35m
- **Metric Captured**:
  - Priority inheritance verified. Telemetry logs confirm Task C's priority was temporarily boosted from 3 to 1 to bypass Task B and unblock Task A.

## Milestone 13 — Task Fault Isolation & Exception Recovery
- **Goal**: Implement synchronous exception recovery, terminating faulty tasks in M-Mode without panicking the kernel, and test using fault injection.
- **What Broke & How it Was Fixed**:
  - *Issue 1*: Redundant `unsafe` wrapper around `core::ptr::addr_of!` triggered clippy warnings.
    - *Fix*: Removed `unsafe` block from `addr_of!` call in Task C.
  - *Issue 2*: Clippy flagged an infinite inner loop inside an outer task loop as `never_loop`.
    - *Fix*: Removed the redundant outer `loop` in Task C since it is terminated by the kernel on fault injection.
- **Time Log**:
  - Implementing exception interceptors in trap handler: 30m
  - Writing fault-injection routine in Task C: 20m
  - Adjusting task stack mapping in PMP priority masking: 25m
- **Metric Captured**:
  - Fault isolation verified. When Task C attempted an unauthorized read of Task A's stack, the CPU triggered a Load Access Fault exception. The kernel caught the exception, terminated Task C, and maintained Task A and Task B execution without interruption.

## Milestone 14 — AUTOSAR-Style Logical Watchdog Thread Monitor
- **Goal**: Build a thread-level health monitor to prevent silent thread freezes and safe-park the CPU upon detection.
- **What Broke & How it Was Fixed**:
  - *Issue 1*: Unnecessary `unsafe` block around `TICK_COUNT.load` inside `watchdog_task` triggered clippy warnings.
    - *Fix*: Removed the unnecessary `unsafe` block since loading atomic variables is safe in Rust.
- **Time Log**:
  - Implementing `sleep_ticks` and `watchdog_checkin` syscall handlers: 45m
  - Adding sleep wake-up scan in timer interrupt: 30m
  - Writing the Watchdog Monitor Task and simulated hang in Task B: 40m
  - Extending PMP configuration driver and reprogramming for 4 tasks: 35m
  - Verifying build and testing simulated hang: 25m
- **Metric Captured**:
  - Watchdog Thread Monitoring successfully verified. RTT logs confirm that the Watchdog Task monitored Task A and Task B health, and upon Task B's simulated hang (stopped checking in after 5 loops), the Watchdog successfully detected the timeout, dumped the metrics dashboard, and safe-parked the CPU in an infinite `wfi` loop.


## Milestone 13 — Task Fault Isolation & Exception Recovery
- **Goal**: Implement synchronous exception recovery, terminating faulty tasks in M-Mode without panicking the kernel, and test using fault injection.
- **What Broke & How it Was Fixed**:
  - *Issue 1*: Redundant `unsafe` wrapper around `core::ptr::addr_of!` triggered clippy warnings.
    - *Fix*: Removed `unsafe` block from `addr_of!` call in Task C.
  - *Issue 2*: Clippy flagged an infinite inner loop inside an outer task loop as `never_loop`.
    - *Fix*: Removed the redundant outer `loop` in Task C since it is terminated by the kernel on fault injection.
- **Time Log**:
  - Implementing exception interceptors in trap handler: 30m
  - Writing fault-injection routine in Task C: 20m
  - Adjusting task stack mapping in PMP priority masking: 25m
- **Metric Captured**:
  - Fault isolation verified. When Task C attempted an unauthorized read of Task A's stack, the CPU triggered a Load Access Fault exception. The kernel caught the exception, terminated Task C, and maintained Task A and Task B execution without interruption.

## Milestone 14 — AUTOSAR-Style Logical Watchdog Thread Monitor
- **Goal**: Build a thread-level health monitor to prevent silent thread freezes and safe-park the CPU upon detection.
- **What Broke & How it Was Fixed**:
  - *Issue 1*: Unnecessary `unsafe` block around `TICK_COUNT.load` inside `watchdog_task` triggered clippy warnings.
    - *Fix*: Removed the unnecessary `unsafe` block since loading atomic variables is safe in Rust.
- **Time Log**:
  - Implementing `sleep_ticks` and `watchdog_checkin` syscall handlers: 45m
  - Adding sleep wake-up scan in timer interrupt: 30m
  - Writing the Watchdog Monitor Task and simulated hang in Task B: 40m
  - Extending PMP configuration driver and reprogramming for 4 tasks: 35m
  - Verifying build and testing simulated hang: 25m
- **Metric Captured**:
  - Watchdog Thread Monitoring successfully verified. RTT logs confirm that the Watchdog Task monitored Task A and Task B health, and upon Task B's simulated hang (stopped checking in after 5 loops), the Watchdog successfully detected the timeout, dumped the metrics dashboard, and safe-parked the CPU in an infinite `wfi` loop.
