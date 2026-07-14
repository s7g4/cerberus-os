//! Physical Memory Protection (PMP) configuration driver.

#![allow(unused_variables, dead_code)]

/// PMP address configuration modes.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PmpAddressMode {
    Off = 0,
    Tor = 1,
    Na4 = 2,
    Napot = 3,
}

/// PMP configuration register flags.
#[derive(Clone, Copy)]
pub struct PmpConfig {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub mode: PmpAddressMode,
    pub locked: bool,
}

impl PmpConfig {
    /// Convert configuration parameters to a single packed byte.
    pub fn to_byte(self) -> u8 {
        let mut val = 0u8;
        if self.read {
            val |= 1 << 0;
        }
        if self.write {
            val |= 1 << 1;
        }
        if self.execute {
            val |= 1 << 2;
        }
        val |= (self.mode as u8) << 3;
        if self.locked {
            val |= 1 << 7;
        }
        val
    }
}

/// Configures a specific PMP address and configuration entry.
///
/// # Safety
///
/// Writing to PMP CSR registers modifies hardware memory isolation limits.
/// Incorrect values can trigger physical instruction access violations.
pub unsafe fn configure_pmp(entry: usize, base_addr: usize, size: usize, config: PmpConfig) {
    #[cfg(not(kani))]
    {
        // Calculate the Naturally Aligned Power Of Two (NAPOT) address value.
        // Encoding formula: pmpaddr = (base >> 2) | ((size / 2 - 1) >> 2)
        let napot_val = (base_addr >> 2) | ((size / 2 - 1) >> 2);

        // Write base address to the target pmpaddr register
        match entry {
            0 => core::arch::asm!("csrw pmpaddr0, {}", in(reg) napot_val),
            1 => core::arch::asm!("csrw pmpaddr1, {}", in(reg) napot_val),
            2 => core::arch::asm!("csrw pmpaddr2, {}", in(reg) napot_val),
            3 => core::arch::asm!("csrw pmpaddr3, {}", in(reg) napot_val),
            4 => core::arch::asm!("csrw pmpaddr4, {}", in(reg) napot_val),
            5 => core::arch::asm!("csrw pmpaddr5, {}", in(reg) napot_val),
            6 => core::arch::asm!("csrw pmpaddr6, {}", in(reg) napot_val),
            _ => panic!("Unsupported PMP entry index"),
        }

        // Update the configuration bank (pmpcfg0 covers entries 0-3, pmpcfg1 covers 4-7 on RV32)
        let cfg_reg = entry / 4;
        let shift = (entry % 4) * 8;
        let config_byte = config.to_byte() as usize;

        if cfg_reg == 0 {
            let mut pmpcfg0: usize;
            core::arch::asm!("csrr {}, pmpcfg0", out(reg) pmpcfg0);
            pmpcfg0 = (pmpcfg0 & !(0xFF << shift)) | (config_byte << shift);
            core::arch::asm!("csrw pmpcfg0, {}", in(reg) pmpcfg0);
        } else if cfg_reg == 1 {
            let mut pmpcfg1: usize;
            core::arch::asm!("csrr {}, pmpcfg1", out(reg) pmpcfg1);
            pmpcfg1 = (pmpcfg1 & !(0xFF << shift)) | (config_byte << shift);
            core::arch::asm!("csrw pmpcfg1, {}", in(reg) pmpcfg1);
        }
    }
}

/// Dynamically reprograms PMP Entries 1, 2, 4, 5, and 6, one dedicated entry
/// per known task stack (Watchdog, Task A, Task B, Task C, HSM Task). The
/// entry belonging to `active_task_name` is set to `Off` so that stack falls
/// through to Entry 3's broad SRAM allow rule; every other entry blocks its
/// stack outright. A name that matches none of the five (both idle tasks,
/// or anything unrecognized) blocks all five -- there is no active task
/// whose stack needs to stay reachable.
pub unsafe fn reprogram_pmp_stack(active_task_name: &str) {
    let stacks: [(&str, usize, usize); 5] = [
        (
            "Watchdog",
            core::ptr::addr_of_mut!(crate::TASK_WD_STACK) as usize,
            1,
        ),
        (
            "Task A",
            core::ptr::addr_of_mut!(crate::TASK_A_STACK) as usize,
            2,
        ),
        (
            "Task B",
            core::ptr::addr_of_mut!(crate::TASK_B_STACK) as usize,
            4,
        ),
        (
            "Task C",
            core::ptr::addr_of_mut!(crate::TASK_C_STACK) as usize,
            5,
        ),
        (
            "HSM Task",
            core::ptr::addr_of_mut!(crate::HSM_STACK) as usize,
            6,
        ),
    ];

    for (name, addr, entry) in stacks {
        if name == active_task_name {
            configure_pmp(
                entry,
                0,
                1024,
                PmpConfig {
                    read: false,
                    write: false,
                    execute: false,
                    mode: PmpAddressMode::Off,
                    locked: false,
                },
            );
        } else {
            configure_pmp(
                entry,
                addr,
                1024,
                PmpConfig {
                    read: false,
                    write: false,
                    execute: false,
                    mode: PmpAddressMode::Napot,
                    locked: false,
                },
            );
        }
    }
}
