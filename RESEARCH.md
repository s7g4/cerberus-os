# Cerberus-OS Research Log — Environment & Architecture

## 1. RISC-V M-Mode Privilege Model
RISC-V defines three primary privilege levels: Machine Mode (M-mode), Supervisor Mode (S-mode), and User Mode (U-mode). 
- **M-mode** is the highest privilege level. It has absolute, unrestricted access to the entire physical address space, interrupt controllers, and Control and Status Registers (CSRs). A bare-metal real-time kernel runs entirely in M-mode because it needs to set up the trap vector (`mtvec`), handle raw timer hardware interrupts (`mtime`/`mtimecmp`), and configure Physical Memory Protection (PMP).
- **Comparison to ARM**:
  - In ARMv7-M/v8-M (Cortex-M microcontrollers), execution is divided into **Thread Mode** (for tasks) and **Handler Mode** (for interrupt service routines/kernels), running in either **Privileged** or **Unprivileged** states. RISC-V M-mode is analogous to Privileged Handler Mode.
  - In ARMv8-A (Cortex-A application processors), privilege is split into Exception Levels EL0 (User) to EL3 (Secure Monitor). RISC-V M-mode maps closely to EL3 (highest privilege, direct hardware interface).

## 2. Linker Stack Reordering via `flip-link`
Under standard compilation, the compiler lays out RAM such that the stack is placed at the end of RAM and grows downwards towards the Heap or BSS sections. 
- **Standard Memory Layout (Before `flip-link`)**:
[Low Memory] -> [ .text ] -> [ .data / .bss ] -> [ Heap ] -> [ Stack (Grows Downward) ] -> [High Memory]

If the stack overflows, it silently overrides heap data or global variables, causing silent, hard-to-debug crashes or security exploits.
- **Protected Memory Layout (After `flip-link`)**:
`flip-link` reorders the layout to place the stack at the lowest address of RAM, growing downwards towards unmapped memory or a read-only page.
[Unmapped / Read-Only Guard Page] <= [ Stack (Grows Downward) ] -> [ .data / .bss ] -> [ Heap ] -> [High Memory]

Now, a stack overflow triggers an immediate physical hardware write violation fault, stopping execution before data corruption can occur. Cost: 0 runtime cycles.

## 3. `probe-rs` and Real-Time Transfer (RTT)
- **`probe-rs` vs. `OpenOCD`**: `OpenOCD` acts as an external GDB server that translates GDB commands into JTAG/SWD protocol commands through a chain of drivers. `probe-rs` is a native Rust library and CLI tool that speaks directly to debug probes (CMSIS-DAP, J-Link, ST-Link). It flashes chips up to 4x faster and streams logs directly, eliminating external server dependencies.
- **RTT vs. UART**: RTT allocates a ring buffer in the microcontroller's RAM. The debug probe polls this buffer directly over JTAG/SWD DMA.
- **UART (115200 baud)**: Blocking. Takes ~86 µs per character to transmit. If debug prints are executed in hot paths or interrupts, they distort real-time behavior.
- **RTT**: Non-blocking. Writing a frame to RAM takes ~2-3 CPU cycles (less than 20 nanoseconds at 160 MHz). Data transfer is offloaded to the debugger hardware.

## 4. `#[naked]` Functions in Rust
- A `#[naked]` function is a function compiled without any compiler-generated prologue (stack allocation, register saving) or epilogue (`ret` instructions).
- **Mandatory for Context Switchers**: A context switcher must manually control the exact state of the stack pointer (`sp`). If the compiler generates a prologue on function entry, it will save register states to the old stack *before* our assembly runs. When our code swaps the stack pointer to the new task, the compiler's epilogue will pop those values from the new stack, resulting in immediate register corruption.

## 5. Crate Auditing: `panic-probe` and `critical-section`

### Why `panic-probe` Failed on RISC-V
- **Problem**: `panic-probe` checks the target architecture during compilation and emits a hard compile-time error (`compile_error!`) if the target is not a `thumbvN-none-eabi[hf]` ARM Cortex-M target. It relies on Cortex-M specific stack-frame layouts and inline ARM assembly (`bkpt` breakpoints) to signal the probe.
- **Solution**: We bypass this target restriction by writing our own target-agnostic, bare-metal panic handler in Rust. Using the `defmt::Debug2Format` wrapper, we format the core panic payload into RTT, then safely park the processor with `wfi`.

### Mutual Exclusion in a Bare-Metal Environment
- **Problem**: RTT buffers require synchronous mutual exclusion to prevent multiple contexts (e.g., interrupts and tasks) from corrupting the logger buffer simultaneously. The `critical-section` crate provides an abstract API, but requires a platform-specific backend to be explicitly linked.
- **Solution**: By enabling the `critical-section-single-hart` feature in the `riscv` crate, we register a bare-metal backend. This backend disables global interrupts during critical blocks by writing to the `mstatus` control and status register, satisfying the linker requirements.

## 6. Context Switch Calling Conventions

### RISC-V ABI Register Partitioning
- **Caller-Saved Registers** (`ra`, `t0-t6`, `a0-a7`): Temporary registers. The calling function is responsible for pushing these to the stack if it needs to preserve their values across a function call. The callee (the function being called) is free to overwrite them without restoring them.
- **Callee-Saved Registers** (`sp`, `s0-s11`): Preserved registers. The called function must ensure these registers hold their original values before returning. If the callee modifies them, it must save them to its stack on entry and restore them on exit.
- **Context Switch Application**: During a voluntary or triggered context switch (`switch_context`), the compiler has already handled saving any active caller-saved registers. Thus, our assembler context switcher only needs to explicitly save and restore the callee-saved registers (`ra` and `s0-s11`).

### Stack Pointer Directives
- **Alignment**: The RISC-V calling convention mandates that the stack pointer `sp` must remain **16-byte aligned** at all times.
- **Direction**: The stack grows downwards (decrementing `sp` allocates space).
- **Addressing**: `sp` points to the **last used byte** (the top of the stack). Pushing a register requires allocating space first (`addi sp, sp, -offset`) and then storing (`sw reg, 0(sp)`).

### The Naked Function Constraint
- **Problem**: Standard compiler functions inject a prologue and epilogue to manage frame pointers and local stack spaces.
- **Impact**: In a context switcher, we enter with the stack pointer of Task A, but we exit after modifying `sp` to point to the stack of Task B. If the compiler injects a prologue, it will push registers onto Task A's stack, but its epilogue will pop values from Task B's stack, corrupting Task B and triggering an immediate CPU crash.
- **Solution**: The `#[naked]` attribute forces the compiler to emit zero prologue or epilogue code. Our assembly represents 100% of the instruction sequence.

## 7. Real-Time Scheduling Algorithms

### Algorithmic Trade-offs
Real-time operating systems (RTOS) require deterministic execution times for scheduling decisions to guarantee hard real-time deadlines. 

| Scheduling Queue Structure | Selection Complexity | Insertion Complexity | Deterministic? |
| :--- | :--- | :--- | :--- |
| **Unsorted Linked List** | O(N) | O(1) | No (depends on active task count) |
| **Sorted Linked List** | O(1) | O(N) | No (insertion search varies) |
| **Bitmap Scheduler (O(1))** | O(1) | O(1) | **Yes (always constant instruction count)** |

*Citation: Buttazzo, G. (2011). Hard Real-Time Computing Systems. Springer. §4.2.*

### Hardware-Accelerated Selection via `ctz`
- **Mechanism**: We represent the ready queue as a single 32-bit bitmask (`ready_bitmap: u32`), where bit `N` represents task priority `N`. Finding the highest priority ready task is mathematically equivalent to finding the lowest set bit index (trailing zeros).
- **RISC-V Implementation**: Rust's `trailing_zeros()` maps directly to the RISC-V **`ctz`** (Count Trailing Zeros) hardware instruction. 
- **Performance**: On RV32IMC processors, this resolves to a single CPU cycle. It guarantees that the time taken to select the next task remains exactly the same whether 1 task is ready or 32 tasks are ready.

## 8. Physical Memory Protection (PMP)

### CSR Configurations
RISC-V Physical Memory Protection is configured using two sets of Control and Status Registers (CSRs):
1. **`pmpcfgN`**: 8-bit configuration registers packed into 32-bit registers (e.g., `pmpcfg0` covers entries 0–3). Each byte contains:
   - `R` (Bit 0): Read permission.
   - `W` (Bit 1): Write permission.
   - `X` (Bit 2): Execute permission.
   - `A` (Bits 3–4): Address matching mode (00: Disabled, 01: TOR, 10: NA4, 11: NAPOT).
   - `L` (Bit 7): Lock bit. When set, PMP rules apply to Machine mode (M-mode) and cannot be cleared until hardware reset.
2. **`pmpaddrN`**: Address registers. In NAPOT mode, the register holds the base address and range size encoded in a single register.

### NAPOT Encoding Formula
Naturally Aligned Power of Two (NAPOT) encodes range size `S` = 2^K and base address `B` using the following conversion:
```ld
pmpaddr = (B >> 2) | ((S / 2 - 1) >> 2)
```
This sets all bits below the scale boundary to 1, letting the CPU decoder calculate the size by finding the first 0 bit from the right.

## 9. Controller Area Network (CAN) Protocol

### Standard Frame Layout (ISO 11898-1)
Standard CAN 2.0A frames use an 11-bit identifier. Transceivers report raw frames in 13-byte packed arrays:
- **Byte 0**: Identifier Bits [10:3] (MSB).
- **Byte 1**: Identifier Bits [2:0] (LSB) shifted to the top 3 bits, followed by RTR (Remote Transmission Request) and IDE (Identifier Extension).
- **Byte 2**: Data Length Code (DLC), indicating payload size (0–8 bytes).
- **Bytes 3–10**: Payload data.

### Bit-Level Extraction
The 11-bit standard ID is reconstructed by extracting the MSB and LSB fields:
```ld
ID = (raw[0] << 3) | (raw[1] >> 5)
```

### Security Filtering at the Network Boundary
To prevent malicious bus attacks (e.g., diagnostic parameter override commands used in physical vehicle control bypasses), we enforce a blocklist at the packet ingestion boundary. Frames with broadcast diagnostic IDs (`0x7DF`) or specific ECU queries (`0x7E0`–`0x7EF`) are rejected immediately, preventing them from entering the kernel queue.

## 10. Cryptographic Frame Authentication

### Why HMAC-SHA256?
- **Replay & Spoofing Mitigation**: The CAN bus has no built-in node authentication. Any compromised node can broadcast arbitrary identifiers. Hash-based Message Authentication Codes (HMAC) combine a cryptographic key with the message payload, preventing unauthorized nodes from generating valid signatures.
- **Why HMAC over Simple Hashing**: HMAC protects against length-extension attacks by hashing the message twice with inner and outer keys:

```ld
HMAC(K, m) = H((K ^ opad) || H((K ^ ipad) || m))
```

### 64-bit Truncation Security Bounds
To fit within standard CAN payload constraints, we truncate the 256-bit SHA-256 MAC output to the first 8 bytes (64 bits). 
- According to **NIST SP 800-107r1 §5.2**, truncating to 64 bits offers a collision resistance threshold of 2^64.
- For a high-bandwidth CAN bus (500 Kbps), attempting a brute-force attack to forge a valid signature would require transmitting millions of frames. This would take years and trigger instant bus faults or network saturation alerts long before a collision could succeed.

### Mitigating Timing Attacks
In signature verification, standard byte comparisons (like `==` or `memcmp`) exit early upon finding the first mismatched byte. Attackers can measure the execution time of the validation routing to determine how many bytes of their guess matched.
To prevent this, we use constant-time verification:
```rust
expected.iter().zip(actual.iter()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
```

## 11. Real-Time Telemetry and Atomic Performance

### Non-Intrusive JTAG RTT DMA
- **Mechanism**: Real-Time Transfer (RTT) uses the debug probe's ability to read and write the target's RAM asynchronously via JTAG/SWD Direct Memory Access (DMA) buses.
- **Observability Impact**: Since the JTAG hardware probe reads and writes memory buffers directly without involving the CPU, logging requires zero processor execution stalls. Unlike serial UART drivers, which block the processor, RTT has no impact on real-time task deadlines.

### Atomic Operations vs. Critical Sections
- **Atomic Operations**: Using atomic variables (`AtomicU32` with `Ordering::Relaxed`) compiles down to RISC-V hardware atomic instructions (like `amoadd.w`). These complete in 1 CPU cycle.
- **Comparison**: Disabling interrupts globally (via critical sections) to increment a normal integer is expensive (takes 10-15 cycles to read, modify, and restore the CSRs) and increases interrupt latency. Atomic instructions eliminate lock contention safely with zero interrupt latency impact.

## 12. CI/CD Static Analysis Gates
### Enforcing Zero Heap Allocations
To guarantee that the compiled binary does not link a heap allocator (complying with real-time requirements), we analyze the compiled ELF symbol table. The Rust compiler redirects heap allocations to `__rust_alloc` and `__rust_dealloc`. By running `cargo-nm` and searching for these symbols, we statically verify that no dynamic memory dependencies are compiled into the image.

### Enforcing Zero Floating-Point Unit (FPU) Usage
Floating-point calculations (single or double precision) require save/restore context overhead on context switches. In microcontrollers running without an FPU or where context switches must remain under 50 cycles, we must ensure the compiler only emits integer instructions. By running `cargo-objdump` and searching for floating-point opcodes (`fadd`, `fmul`, `fdiv`, `fsub`), we statically verify that the compiler has not emitted FPU instructions.

## 13. Dynamic Hardware-Level Performance Benchmarking

### The RISC-V Cycle Counter CSR (`mcycle`)
The RISC-V privileged architecture defines the `mcycle` CSR (Control and Status Register) as a 64-bit counter that increments on every CPU clock cycle. On 32-bit hardware targets (RV32), this is mapped to two 32-bit registers: `mcycle` (low 32 bits) and `mcycleh` (high 32 bits).
For hot-path latency profiling (such as measuring the execution time of a cryptographic hash or a lock-free queue push), reading the low 32-bit `mcycle` register alone is highly efficient. At a CPU frequency of 160 MHz, a 32-bit register wraps around every:
```ld
2^32 / 160,000,000 ≈ 26.84 seconds
```
Because the operations under profile (e.g., HMAC validation) complete within microseconds (thousands of cycles), the 32-bit counter will never wrap around more than once during a single measurement.

### Safe Wrap-Around Cycle Arithmetic
To calculate the elapsed CPU cycles between a start time `t_start` and an end time `t_end` without branching or conditional checks (which would inject pipeline stalls and distort the measurement), we utilize standard unsigned wrapping subtraction:
```rust
let elapsed = t_end.wrapping_sub(t_start);
```
Under two's complement integer representation, if the counter overflows and wraps around to `0` after `t_start` but before `t_end`, the subtraction `t_end - t_start` automatically resolves to the correct modular difference. This guarantees cycle-accurate results under all conditions with zero branching overhead.

## 14. User-Mode Privilege Transition & PMP Stack Sandboxing

### User-Mode Drop via `mret`
In RISC-V, direct register modifications cannot change the CPU privilege mode. Instead, transitions to lower privilege modes are executed via the exception return instruction (`mret`).
On execution of `mret`, the hardware performs the following state updates:
1. The Program Counter (`pc`) is loaded with the value stored in the `mepc` CSR.
2. The privilege mode is set to the value stored in the Machine Previous Privilege (`MPP`) field of `mstatus` (bits [12:11]). Setting `MPP = 0` sets the target mode to User Mode (U-Mode).
3. The global interrupt enable bit `mstatus.MIE` is loaded from `mstatus.MPIE` (Machine Previous Interrupt Enable).

### PMP Priority Masking for Dynamic Stack Isolation
PMP registers are evaluated sequentially from lowest index to highest index (Entry 0 to Entry 3). The first matching entry decides the access rights, and subsequent entries are ignored. We exploit this priority ordering to isolate task stacks:
- **Entry 0**: Flash memory RX (Locked, global code execution).
- **Entry 1**: Inactive Task's Stack region (No Access: R=0, W=0, X=0, Unlocked).
- **Entry 2**: Global RAM RW (Read/Write Access: R=1, W=1, X=0, Unlocked).

When Task A executes, Entry 1 is dynamically programmed to Task B's stack bounds. If Task A attempts to read or write to Task B's stack, it matches Entry 1 first, resulting in an immediate Store/Load Access Fault exception. Access to Task A's own stack or global variables misses Entry 1 and matches Entry 2, allowing standard execution.

### Kernel Stack Isolation via `mscratch`
To prevent user stack overflows or malicious corruption from affecting kernel trap handling, we maintain a separate Machine-Mode Interrupt Stack.
1. The `mscratch` CSR holds the pointer to the top of the secure `KERNEL_STACK`.
2. On trap entry, `csrrw sp, mscratch, sp` swaps the user stack pointer and the kernel stack pointer.
3. The register context is saved directly to the user stack to maintain simple context-switching structures, but the Rust `trap_handler` executes entirely on the secure `KERNEL_STACK`.
4. On return, `csrrw sp, mscratch, sp` restores the user stack pointer to the `sp` register, ensuring U-Mode runs on its own stack.

## 15. Real-Time Priority Inversion & Priority Inheritance Protocol (PIP)

### The Priority Inversion Problem
Priority inversion occurs when a low-priority task (T_L) holds a shared resource (like a mutex) required by a high-priority task (T_H). If an intermediate medium-priority task (T_M) becomes ready, it will preempt T_L (since T_M has higher priority than T_L). This prevents T_L from completing its critical section and releasing the lock. Consequently, T_H remains blocked on the resource, indirectly starved by T_M—effectively reversing the system's priority model.

### Priority Inheritance Protocol (PIP)
To solve this, we implement the Priority Inheritance Protocol:
1. When T_H attempts to acquire a mutex held by T_L, the kernel blocks T_H.
2. The kernel temporarily raises the active priority of the lock owner (T_L) to match the priority of T_H (T_active = P_H).
3. Now, T_L executes at priority P_H, allowing it to preempt T_M and finish its critical section.
4. When T_L releases the mutex, its active priority is restored to its base priority (T_active = P_L).

### O(1) Priority Mapping Array
To prevent priority boosts from degrading our O(1) scheduler complexity into an O(N) search loop, we introduce a lookup array:
```rust
pub priority_to_task: [Option<u8>; MAX_TASKS]
```
