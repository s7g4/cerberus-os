# Cerberus-OS — Scientific Metrics Registry

| ID | Metric | Definition | Unit | Target | Measured | Tool | Milestone |
|----|--------|-----------|------|--------|----------|------|-----------|
| M01 | binary_size_text | .text section size | bytes | <32768 | 20436 | cargo-size | Initial Boot |
| M02 | binary_size_bss | .bss section size | bytes | <4096 | 3120 | cargo-size | Initial Boot |
| M03 | trap_entry_latency | Context preservation overhead | cycles | <80 | 68 | mcycle CSR | Interrupt Vector |
| M04 | context_switch_latency | Scheduler execution and stack swap | cycles | <100 | 54 | mcycle CSR | Scheduler |
| M05 | can_enqueue_latency | SPSC queue push/pop overhead | cycles | <50 | 18 | mcycle CSR | CAN Protocol |
| M06 | hmac_verify_latency | Cryptographic signature verification | cycles | <12000 | 8924 | mcycle CSR | HMAC Security |
| M07 | pmp_fault_recovery | Fault intercept and task termination | cycles | <150 | 92 | mcycle CSR | Fault Containment |
| M08 | watchdog_checkin_latency | Syscall 5 check-in registration overhead | cycles | <50 | 12 | mcycle CSR | Thread Watchdog |
| M09 | sleep_ticks_latency | Syscall 2 sleep blocking overhead | cycles | <60 | 14 | mcycle CSR | Thread Watchdog |
| M10 | partition_swap_latency | Time to swap time partition context | cycles | <80 | 58 | mcycle CSR | Time Partitioning |
| M11 | zero_copy_ipc_latency | Direct stack-to-stack rendezvous transfer | cycles | <70 | 32 | mcycle CSR | Capability IPC |
