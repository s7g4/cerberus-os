# Contributing to Cerberus-OS

Cerberus-OS is a bare-metal, `#![no_std]` dual-core RISC-V microkernel. Contributions are welcome, but the constraints below are non-negotiable: they exist to keep the kernel small, deterministic, and verifiable on real hardware.

## Before you start

- Read [ARCHITECTURE.md](ARCHITECTURE.md) first. It documents the isolation model (PMP-based stack sandboxing), the scheduler, and the fault-containment philosophy ("terminate the task, never panic the core"). Changes should work within this design, not around it.
- Check [DEVLOG.md](DEVLOG.md) for the history behind non-obvious decisions (e.g. why the secure boot check is a checksum and not real ECDSA — Milestone 19).
- For anything larger than a small fix, open an issue describing the change before writing code.

## Hard constraints

- **No `alloc`, no heap.** The kernel proves this in CI by asserting `__rust_alloc` is absent from the symbol table. New code must not pull in an allocator, directly or transitively.
- **No floating point.** CI disassembles the release binary and fails if any `f*` opcodes appear. Use fixed-point or integer arithmetic.
- **`.text` budget: 32 KB.** CI fails the build if the release binary's `.text` section exceeds this. If your change grows the binary past budget, it needs to justify the growth in the PR description, not just pass CI by luck.
- **Stay `no_std`.** Crates that need host-side unit tests use `#![cfg_attr(not(test), no_std)]` — the pattern already used in `security`, `network`, `scheduler`, and `telemetry`. Don't add a `std` dependency to make testing easier.

## Local checks before opening a PR

```bash
cargo fmt --check
cargo clippy --target riscv32imac-unknown-none-elf -- -D warnings
cargo build --release --target riscv32imac-unknown-none-elf
cargo test --workspace --target x86_64-unknown-linux-gnu
```

The last command runs host-side unit tests for the crates that support them (`security`, `network`, `scheduler`, `telemetry`). `kernel` itself is bare-metal only and isn't part of that test run.

If you touch the Kani-verified modules (`scheduler::bitmap`, `kernel::trap`), also run the relevant proof harness with `cargo kani` and confirm it still passes — see the harness names in [DEVLOG.md](DEVLOG.md).

## Emulator/HIL changes

If your change affects boot behavior, fault handling, or anything exercised by the Renode fault-injection suite (`renode-config/`), run it locally before submitting:

```bash
renode-test renode-config/esp32c3.robot
```

Renode emulation is CPU- and memory-intensive. Keep runs short and bounded — don't leave an unbounded `emulation RunFor` loop running unattended.

## Commit and PR style

- Conventional commit format: `type(scope): description` (e.g. `fix(trap): widen containable exception set`).
- Keep PRs scoped to one concern. A bug fix shouldn't bundle an unrelated refactor.
- Explain *why* in the PR description, not just *what* — the diff already shows what changed.

## What won't be merged

- Abstractions or configurability added "for the future" with no current caller.
- Dependencies pulled in for convenience that could be avoided with a few lines of code (binary size budget applies to dependencies too).
- Silent behavior changes to the fault-containment path without an accompanying DEVLOG entry explaining the reasoning.
