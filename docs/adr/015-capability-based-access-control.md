# ADR 015: Capability-Based Access Control and Zero-Copy IPC

## Context
Allowing tasks to access resources via global IDs (ambient authority) is a security vulnerability. Additionally, communication across isolated stacks usually requires intermediate kernel buffers, adding latency and requiring dynamic allocation.

## Decision
We enforce Capability-Based Access Control. Tasks refer to local indices in their C-List. We also implement synchronous zero-copy rendezvous IPC (`sys_send`/`sys_recv`) which copies data directly stack-to-stack in Machine Mode.

## Consequences
- **Least Privilege**: Tasks can only access resources explicitly allowed by their capabilities list.
- **Zero Copy**: Communication bypasses intermediate buffers, executing in a single `memcpy` during the trap.
- **Zero Allocation**: No dynamic buffers or heap memory are used.
