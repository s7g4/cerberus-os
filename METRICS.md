# Cerberus-OS — Scientific Metrics Registry

| ID | Metric | Definition | Unit | Target | Measured | Tool | Milestone |
|----|--------|-----------|------|--------|----------|------|-----------|
| M01 | binary_size_text | .text section size | bytes | <32768 | 25968 | cargo-size | Milestone 21 |
| M02 | binary_size_bss | .bss section size | bytes | <4096 (historical) | 10304 | cargo-size | Milestone 21 |
| M03 | trap_entry_latency | Context preservation overhead | cycles | <80 | 68 | mcycle CSR | Interrupt Vector |
| M04 | context_switch_latency | Scheduler execution and stack swap | cycles | <100 | 54 | mcycle CSR | Scheduler |
| M05 | can_enqueue_latency | SPSC queue push/pop overhead | cycles | <50 | 18 | mcycle CSR | CAN Protocol |
| M06 | hmac_verify_latency | Cryptographic signature verification | cycles | <12000 | 8924 | mcycle CSR | HMAC Security |
| M07 | pmp_fault_recovery | Fault intercept and task termination | cycles | <150 | 92 | mcycle CSR | Fault Containment |
| M08 | watchdog_checkin_latency | Syscall 5 check-in registration overhead | cycles | <50 | 12 | mcycle CSR | Thread Watchdog |
| M09 | sleep_ticks_latency | Syscall 2 sleep blocking overhead | cycles | <60 | 14 | mcycle CSR | Thread Watchdog |
| M10 | partition_swap_latency | Time to swap time partition context | cycles | <80 | 58 | mcycle CSR | Time Partitioning |
| M11 | zero_copy_ipc_latency | Direct stack-to-stack rendezvous transfer | cycles | <70 | 32 | mcycle CSR | Capability IPC |
| M12 | secure_boot_size_text | .text size after Secure Boot & vHSM (historical checkpoint) | bytes | <32768 | 26364 | cargo-size | Milestone 19 |

M01/M02 are re-measured as of Milestone 21 (SMP concurrency hardening + telemetry). M02's original <4096 target predates SMP: two full per-core schedulers, additional task stacks (HSM, both idle tasks), and the mutex/capability tables roughly tripled static state — expected growth from added scope, not a regression. It is not CI-enforced (only M01, zero-alloc, and zero-FPU are gated in `cerberus-ci.yml`). M03–M11 are carried over from their originating milestones and have not been independently re-measured after Milestone 21's locking changes; the added `SCHED_LOCK` acquire/release is a handful of cycles on the uncommon-contention fast path and is not expected to move these figures by an order of magnitude, but the exact numbers above predate that change.
