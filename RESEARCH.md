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
