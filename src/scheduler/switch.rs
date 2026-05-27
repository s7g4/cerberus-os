//! Context switching assembly routine.

/// Switches execution context from the current task to the next task.
///
/// # Safety
///
/// This is a naked function written in assembly. It bypasses the standard Rust calling convention.
/// It must be called with:
/// - `old_sp_ptr` in register `a0` (pointer to the old TCB's `saved_sp` field).
/// - `new_sp` in register `a1` (the stack pointer of the new task).
#[naked]
#[no_mangle]
pub unsafe extern "C" fn switch_context(old_sp_ptr: *mut usize, new_sp: usize) {
    core::arch::asm!(
        // 1. Allocate frame on the current task's stack to save callee-saved registers.
        // We save 13 registers (ra + s0-s11) = 52 bytes.
        // We adjust to 64 bytes to maintain the mandatory 16-byte stack alignment.
        "addi sp, sp, -64",
        "sw   ra,  0(sp)", // Save return address
        "sw   s0,  4(sp)", // Save callee-saved s0-s11
        "sw   s1,  8(sp)",
        "sw   s2, 12(sp)",
        "sw   s3, 16(sp)",
        "sw   s4, 20(sp)",
        "sw   s5, 24(sp)",
        "sw   s6, 28(sp)",
        "sw   s7, 32(sp)",
        "sw   s8, 36(sp)",
        "sw   s9, 40(sp)",
        "sw   s10, 44(sp)",
        "sw   s11, 48(sp)",
        // 2. Save the current stack pointer `sp` into the old task's TCB.saved_sp (a0)
        "sw   sp, 0(a0)",
        // 3. Switch the stack pointer to the new task's saved stack pointer (a1)
        "mv   sp, a1",
        // 4. Restore the new task's callee-saved registers from its stack
        "lw   ra,  0(sp)",
        "lw   s0,  4(sp)",
        "lw   s1,  8(sp)",
        "lw   s2, 12(sp)",
        "lw   s3, 16(sp)",
        "lw   s4, 20(sp)",
        "lw   s5, 24(sp)",
        "lw   s6, 28(sp)",
        "lw   s7, 32(sp)",
        "lw   s8, 36(sp)",
        "lw   s9, 40(sp)",
        "lw   s10, 44(sp)",
        "lw   s11, 48(sp)",
        "addi sp, sp, 64", // Deallocate stack frame
        // 5. Jump back to the restored return address (ra)
        "ret",
        options(noreturn)
    );
}
