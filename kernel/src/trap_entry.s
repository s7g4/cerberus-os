.global _trap_entry
.align 4
_trap_entry:
    csrrw sp, mscratch, sp
    csrrw sp, mscratch, sp
    addi sp, sp, -128

    sw ra, 0(sp)
    sw t0, 4(sp)
    sw t1, 8(sp)
    sw t2, 12(sp)
    sw s0, 16(sp)
    sw s1, 20(sp)
    sw a0, 24(sp)
    sw a1, 28(sp)
    sw a2, 32(sp)
    sw a3, 36(sp)
    sw a4, 40(sp)
    sw a5, 44(sp)
    sw a6, 48(sp)
    sw a7, 52(sp)
    sw s2, 56(sp)
    sw s3, 60(sp)
    sw s4, 64(sp)
    sw s5, 68(sp)
    sw s6, 72(sp)
    sw s7, 76(sp)
    sw s8, 80(sp)
    sw s9, 84(sp)
    sw s10, 88(sp)
    sw s11, 92(sp)
    sw t3, 96(sp)
    sw t4, 100(sp)
    sw t5, 104(sp)
    sw t6, 108(sp)

    csrr t0, mepc
    sw t0, 112(sp)
    csrr t1, mstatus
    sw t1, 116(sp)

    csrrw sp, mscratch, sp

    csrr a0, mcause
    csrr a1, mscratch
    csrr a2, mcycle

    jal trap_handler

    csrw mscratch, a0
    csrrw sp, mscratch, sp

    lw t0, 112(sp)
    csrw mepc, t0
    lw t1, 116(sp)
    csrw mstatus, t1

    lw ra, 0(sp)
    lw t0, 4(sp)
    lw t1, 8(sp)
    lw t2, 12(sp)
    lw s0, 16(sp)
    lw s1, 20(sp)
    lw a0, 24(sp)
    lw a1, 28(sp)
    lw a2, 32(sp)
    lw a3, 36(sp)
    lw a4, 40(sp)
    lw a5, 44(sp)
    lw a6, 48(sp)
    lw a7, 52(sp)
    lw s2, 56(sp)
    lw s3, 60(sp)
    lw s4, 64(sp)
    lw s5, 68(sp)
    lw s6, 72(sp)
    lw s7, 76(sp)
    lw s8, 80(sp)
    lw s9, 84(sp)
    lw s10, 88(sp)
    lw s11, 92(sp)
    lw t3, 96(sp)
    lw t4, 100(sp)
    lw t5, 104(sp)
    lw t6, 108(sp)

    addi sp, sp, 128
    mret
