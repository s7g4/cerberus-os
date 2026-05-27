# src/trap_entry.s
#
# ## Why assembly here and not Rust?
#
# When a trap fires, the CPU is mid-instruction in some task.
# ALL registers (ra, t0-t6, a0-a7, s0-s11) contain that task's live data.
# If we call ANY Rust function before saving them, the Rust calling
# convention will OVERWRITE them — we lose the task's state permanently.
#
# The assembly stub saves every register to a trap frame on the stack
# before handing control to Rust. This is the same technique used by
# Linux, FreeRTOS, and every production RTOS.

.section .text.trap_entry
.global _trap_entry
.align 4

_trap_entry:
    # Step 1: Allocate trap frame on stack (32 registers × 4 bytes = 128 bytes)
    # We allocate 128 bytes to maintain 16-byte stack alignment (ABI requirement)
    addi sp, sp, -128

    # Step 2: Save original t0 first, then read the cycle counter immediately
    # to measure the exact latency of the trap entry.
    sw   t0,   4(sp)
    csrr t0,   mcycle       # Read current CPU clock cycles
    sw   t0, 112(sp)       # Save cycle count to offset 112

    # Step 3: Save remaining caller-saved and callee-saved registers
    sw   ra,   0(sp)
    # t0 is already saved at 4(sp)
    sw   t1,   8(sp)
    sw   t2,  12(sp)
    sw   a0,  16(sp)
    sw   a1,  20(sp)
    sw   a2,  24(sp)
    sw   a3,  28(sp)
    sw   a4,  32(sp)
    sw   a5,  36(sp)
    sw   a6,  40(sp)
    sw   a7,  44(sp)
    sw   t3,  48(sp)
    sw   t4,  52(sp)
    sw   t5,  56(sp)
    sw   t6,  60(sp)
    sw   s0,  64(sp)
    sw   s1,  68(sp)
    sw   s2,  72(sp)
    sw   s3,  76(sp)
    sw   s4,  80(sp)
    sw   s5,  84(sp)
    sw   s6,  88(sp)
    sw   s7,  92(sp)
    sw   s8,  96(sp)
    sw   s9, 100(sp)
    sw  s10, 104(sp)
    sw  s11, 108(sp)

    # Step 4: Pass arguments to Rust trap_handler (ABI: first arg in a0, second in a1)
    csrr a0, mcause        # First arg: mcause register
    lw   a1, 112(sp)       # Second arg: initial cycle count
    call trap_handler

    # Step 5: Restore all registers (reverse order)
    lw   s11, 108(sp)
    lw   s10, 104(sp)
    lw   s9,  100(sp)
    lw   s8,   96(sp)
    lw   s7,   92(sp)
    lw   s6,   88(sp)
    lw   s5,   84(sp)
    lw   s4,   80(sp)
    lw   s3,   76(sp)
    lw   s2,   72(sp)
    lw   s1,   68(sp)
    lw   s0,   64(sp)
    lw   t6,   60(sp)
    lw   t5,   56(sp)
    lw   t4,   52(sp)
    lw   t3,   48(sp)
    lw   a7,   44(sp)
    lw   a6,   40(sp)
    lw   a5,   36(sp)
    lw   a4,   32(sp)
    lw   a3,   28(sp)
    lw   a2,   24(sp)
    lw   a1,   20(sp)
    lw   a0,   16(sp)
    lw   t2,   12(sp)
    lw   t1,    8(sp)
    lw   t0,    4(sp)
    lw   ra,    0(sp)

    # Deallocate stack frame
    addi sp, sp, 128

    # mret: return from Machine mode trap
    mret
