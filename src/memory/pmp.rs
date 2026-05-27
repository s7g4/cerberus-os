//! Physical Memory Protection (PMP) configuration driver.

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
        if self.read { val |= 1 << 0; }
        if self.write { val |= 1 << 1; }
        if self.execute { val |= 1 << 2; }
        val |= (self.mode as u8) << 3;
        if self.locked { val |= 1 << 7; }
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
    // Calculate the Naturally Aligned Power Of Two (NAPOT) address value.
    // Encoding formula: pmpaddr = (base >> 2) | ((size / 2 - 1) >> 2)
    let napot_val = (base_addr >> 2) | ((size / 2 - 1) >> 2);

    // Write base address to the target pmpaddr register
    match entry {
        0 => core::arch::asm!("csrw pmpaddr0, {}", in(reg) napot_val),
        1 => core::arch::asm!("csrw pmpaddr1, {}", in(reg) napot_val),
        2 => core::arch::asm!("csrw pmpaddr2, {}", in(reg) napot_val),
        3 => core::arch::asm!("csrw pmpaddr3, {}", in(reg) napot_val),
        _ => panic!("Unsupported PMP entry index"),
    }

    // Update the configuration bank (pmpcfg0 covers entries 0-3 on RV32)
    let mut pmpcfg0: usize;
    core::arch::asm!("csrr {}, pmpcfg0", out(reg) pmpcfg0);

    let shift = entry * 8;
    let config_byte = config.to_byte() as usize;
    pmpcfg0 = (pmpcfg0 & !(0xFF << shift)) | (config_byte << shift);

    core::arch::asm!("csrw pmpcfg0, {}", in(reg) pmpcfg0);
}
