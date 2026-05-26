# Cerberus-OS Research Log — Phase 0

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
