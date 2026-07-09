# Cerberus-OS Technical Architecture

Cerberus-OS is a bare-metal, high-integrity Real-Time Operating System (RTOS) microkernel written in Rust for 32-bit RISC-V targets. It acts as a secure partitioning layer for automotive Electronic Control Units (ECUs).

## System Layout

```mermaid
flowchart TB

    subgraph TS["User/Task Space (U-Mode)"]
        direction LR
        WDTask["Watchdog Task<br/>- Priority 0"]
        TaskA["Task A<br/>- Priority 1"]
        TaskB["Task B<br/>- Priority 2"]
        TaskC["Task C<br/>- Priority 3"]
    end

    TS -->|"Syscall (ecall) / Interrupt"| KS

    subgraph KS["Kernel Space (M-Mode)"]
        direction TB

        CS["Assembly Trap Handler (trap_entry.s)<br/>- Swaps stack via mscratch<br/>- Preserves full context<br/>- Measures entry latency"]

        SCH["Partition Scheduler (bitmap.rs)<br/>- Cyclic Major/Minor Frames (ARINC 653)<br/>- Dynamic PMP stack reprogramming"]

        PMP["Physical Memory Protection (pmp.rs)<br/>- Enforces global W^X boundaries<br/>- Dynamically reprograms stack guards"]

        EX["Fault Interceptor & Recovery<br/>- Traps memory violations<br/>- Terminates faulty tasks gracefully"]

        CAN["CAN Network Security Filter (can.rs)<br/>- OBD-II Diagnostic Filter<br/>- Lock-free SPSC queue"]

        HMAC["Cryptographic Auth (hmac.rs)<br/>- Truncated HMAC-SHA256<br/>- Constant-time verification"]

        CS --> SCH
        SCH --> PMP
        PMP --> EX
        EX --> CAN
        CAN --> HMAC
    end
```

## Core Subsystems

### 1. Privilege & Execution Model
* **Privilege Separation**: The kernel executes in Machine Mode (M-Mode), while all application tasks execute in User Mode (U-Mode). 
* **W^X Policy**: We enforce a strict **Write XOR Execute** configuration at the CPU level. Using Physical Memory Protection (PMP), we configure Flash memory (Code segment) as Read+Execute, and SRAM memory (Data segment) as Read+Write. If any code attempts to execute from RAM or write to Flash, a hardware violation fault immediately triggers a system halt.
* **Kernel Interrupt Stack**: To prevent User-space stack overflows from corrupting the kernel, the `mscratch` register holds the secure `KERNEL_STACK` pointer. On trap entry, the kernel swaps stacks, executes the Rust handler on kernel memory, and swaps back before dropping privilege back to U-Mode.

### 2. Trap Handler Vector
* **Entry Path**: The `mtvec` register points to the entry vector in `src/trap_entry.s`.
* **Context Preservation**: On trap, the assembly saves all 32 integer registers to the user stack frame. It then reads the hardware cycle counter (`mcycle`) to calculate context preservation latency and calls the Rust `trap_handler`.
* **Preemption**: When a timer interrupt triggers, the handler re-arms the CLINT comparator (`mtimecmp`) and calls the scheduler. If a different task is ready, the stack pointer is swapped, restoring registers from the new task's stack.

### 3. ARINC 653 Time Partition Scheduler
* **Design**: Preemptive priority scheduling is replaced with fixed-time slicing. The system execution timeline is split into **Major Frames (MAFs)**, which are subdivided into fixed **Minor Frames (MIFs)** allocated to specific tasks (e.g., 100 ticks per partition).
* **Safety Guarantee**: Temporal isolation is enforced by the hardware timer. Even if a partition runs into an infinite loop or crashes, the timer interrupt preempts it on the MIF boundary, reprogramms PMP stack limits, and context-switches to the next scheduled partition.
* **PIP Removal**: Because each partition is allocated a dedicated temporal slot and runs exactly one task context, priority inversion across partitions is impossible. Cooperative blocks (e.g. blocking on Mutex 0) naturally trigger a context swap, rendering the Priority Inheritance Protocol (PIP) obsolete and allowing its clean removal.

### 4. Hardware Exception Trapping & Recovery
* Synchronous exception causes — Instruction Access Fault (`1`), Illegal Instruction (`2`), Load/Store Address Misaligned (`4`/`6`), and Load/Store Access Fault (`5`/`7`) — are all caught by the same containment path in the kernel trap handler. Widening this set (it originally handled only `1`/`5`/`7`) closed a real gap: any *other* synchronous exception used to fall through to a hard `panic!()` that halted the whole core rather than just the offending task.
* Rather than panicking, the kernel terminates the offending task, marks it as `Terminated` in its TCB, releases any mutex it held to the highest-priority waiter, clears its ready bit, and reschedules to healthy tasks.

### 5. CAN Stack and Cryptographic Authentication
* **Boundary Filtering**: Parses raw transceiver bytes, extracting IDs and data payloads. Rejects OBD-II diagnostic request packets (`0x7DF`) and ECU query ranges (`0x7E0`–`0x7EF`) at the boundary.
* **HMAC Signatures**: Appends a 64-bit truncated HMAC-SHA256 signature to payloads, ensuring authenticity over low-bandwidth buses.
* **Side-Channel Mitigation**: Verification uses a constant-time bitwise accumulator to avoid early-exit timing leaks.

### 6. Dual-Core SMP & Cross-Core Synchronization
* **Per-Core Scheduling**: Each hart (`hart0`, `hart1`) runs its own independent `BitMapScheduler` instance and takes its own timer/software interrupts; task-to-core assignment is fixed at registration (Watchdog, Task A, and the vHSM partition on hart0; Task B and Task C on hart1).
* **`SCHED_LOCK`**: Several operations are inherently cross-core — IPC rendezvous searches both cores' task tables for a waiting sender/receiver, mutex unlock scans both cores for the highest-priority waiter, and `Syscall 8` can terminate a task on either core. All of these hold a single reentrant spinlock (`SCHED_LOCK`) for the full duration of the access, not just the initial pointer fetch. Reentrancy matters here specifically: a synchronous fault can occur *while a hart already holds the lock* (that's exactly what the fault-injection suite exercises), and a plain non-reentrant lock would deadlock a hart against itself in that case.
* **Cross-Core Wakeup**: When a rendezvous or mutex handoff resolves a task on the *other* core, the resolving hart writes to that core's CLINT `msip` register to raise a software interrupt, which the target hart handles as a normal reschedule point.

### 7. Real-Time Telemetry
* `telemetry::log_telemetry` runs inside the trap handler (never from task code) on every task swap, IPC transfer, and contained fault, encoding a small `TraceEvent` with Postcard and writing it to a dedicated RTT channel separate from the human-readable log.
* A host-side broker and Streamlit dashboard (see `README.md`) decode and visualize this stream live; the MCU side has no network stack, no allocator, and no knowledge of the host tooling.
