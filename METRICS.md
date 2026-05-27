# Cerberus-OS — Scientific Metrics Registry

| ID | Metric | Definition | Unit | Target | Measured | Tool | Milestone |
|----|--------|-----------|------|--------|----------|------|-----------|
| M01 | binary_size_text | .text section size | bytes | <32768 | 19106 | cargo-size | Initial Boot |
| M02 | binary_size_bss | .bss section size | bytes | <4096 | 2080 | cargo-size | Initial Boot |
| M03 | trap_entry_latency | Context preservation overhead | cycles | <80 | 68 | mcycle CSR | Interrupt Vector |
| M04 | context_switch_latency | Scheduler execution and stack swap | cycles | <100 | 54 | mcycle CSR | Scheduler |
| M05 | can_enqueue_latency | SPSC queue push/pop overhead | cycles | <50 | 18 | mcycle CSR | CAN Protocol |
| M06 | hmac_verify_latency | Cryptographic signature verification | cycles | <12000 | 8924 | mcycle CSR | HMAC Security |
