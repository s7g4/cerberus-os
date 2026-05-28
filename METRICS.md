# Cerberus-OS — Scientific Metrics Registry

| ID | Metric | Definition | Unit | Target | Measured | Tool | Milestone |
|----|--------|-----------|------|--------|----------|------|-----------|
| M01 | binary_size_text | .text section size | bytes | <32768 | 19106 | cargo-size | Initial Boot |
| M02 | binary_size_bss | .bss section size | bytes | <4096 | 2080 | cargo-size | Initial Boot |
| M03 | trap_entry_latency | Context preservation overhead | cycles | <80 | 68 | mcycle CSR | Interrupt Vector |
| M04 | context_switch_latency | Scheduler execution and stack swap | cycles | <100 | 54 | mcycle CSR | Scheduler |
| M05 | can_enqueue_latency | SPSC queue push/pop overhead | cycles | <50 | 18 | mcycle CSR | CAN Protocol |
| M06 | hmac_verify_latency | Cryptographic signature verification | cycles | <12000 | 8924 | mcycle CSR | HMAC Security |

## Milestone 13 — Task Fault Isolation & Recovery
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
