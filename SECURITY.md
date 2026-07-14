# Security Policy

Cerberus-OS is a research/portfolio microkernel, not a certified or production-hardened product. This document sets expectations accordingly.

## Supported versions

There are no tagged releases yet. Only the `main` branch is supported; security fixes land there.

## Reporting a vulnerability

Please report security issues privately rather than opening a public issue:

- Use [GitHub's private vulnerability reporting](../../security/advisories/new) for this repository, or
- Email shauryagaur07@gmail.com with a description and, if possible, reproduction steps.

Expect an initial response within a few days. This is a single-maintainer project, so timelines are best-effort.

## Known scope limitations

These are documented design tradeoffs, not undisclosed bugs:

- **Secure boot is checksum-based, not cryptographic.** `security/src/bootloader.rs` verifies a SHA-256 hash of a fixed trusted payload plus a non-cryptographic XOR linkage check between hardcoded key/signature buffers. It demonstrates the tamper-detection *flow* (hash mismatch → reject), not a real signature scheme. An earlier revision used real ECDSA-P256 (`p256` crate) but exceeded the 32 KB `.text` budget — see `DEVLOG.md` Milestone 19. Do not treat this as verifying image authenticity against a real attacker.
- **PMP-based isolation, not a full MMU.** Task stacks are sandboxed via RISC-V Physical Memory Protection (NAPOT regions), which gives coarse-grained, fixed-size spatial isolation. It is not equivalent to per-process virtual address spaces.
- **No guard page against a task overflowing its own stack.** PMP blocks a task from reaching *other* tasks' stacks, but the currently active task's own stack has no guard region below it. `RESEARCH.md` documents `flip-link` as a technique that would provide this; it was researched but never wired into `.cargo/config.toml`'s linker invocation, so it is not currently in effect.
- **The vHSM's HMAC key is a hardcoded compile-time constant** (`security/src/hsm.rs`), embedded in plaintext in the flashed image, as its own name (`cerberus-os-dev-key-not-for-prod`) says. It demonstrates the IPC-isolated-partition pattern, not real key custody — anyone with firmware read access can recover it.
- **`host/telemetry_broker.py` and `host/dashboard.py` are local developer tooling**, not hardened network services. The broker binds to `127.0.0.1` only and expects a single trusted local Renode instance as its data source; it does no authentication and should never be exposed to an untrusted network.
- **Kani formal verification currently covers the scheduler invariants** (`scheduler::bitmap`) and select trap-handling properties, not the entire kernel. Absence of a proof for a given module is not a claim of correctness.

If you find a gap beyond what's listed above — e.g. a way to defeat task isolation from U-mode, a PMP misconfiguration that leaks another task's stack, or a panic reachable from untrusted input rather than an internal invariant — please report it via the channels above.
