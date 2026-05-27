//! Global kernel telemetry metrics.

use core::sync::atomic::{AtomicU32, Ordering};

pub static METRIC_SWITCH_CYCLES:   AtomicU32 = AtomicU32::new(0);
pub static METRIC_FRAMES_RX:       AtomicU32 = AtomicU32::new(0);
pub static METRIC_FRAMES_DROPPED:  AtomicU32 = AtomicU32::new(0);
pub static METRIC_HMAC_FAILURES:   AtomicU32 = AtomicU32::new(0);
pub static METRIC_PMP_VIOLATIONS:  AtomicU32 = AtomicU32::new(0);

/// Prints the kernel health and performance dashboard over RTT.
pub fn dump_metrics() {
    defmt::info!("=== Cerberus-OS Telemetry ===");
    defmt::info!("  Context Switch Cycles = {}", METRIC_SWITCH_CYCLES.load(Ordering::Relaxed));
    defmt::info!("  CAN Frames Received  = {}", METRIC_FRAMES_RX.load(Ordering::Relaxed));
    defmt::info!("  CAN Frames Dropped   = {}", METRIC_FRAMES_DROPPED.load(Ordering::Relaxed));
    defmt::info!("  HMAC Auth Failures   = {}", METRIC_HMAC_FAILURES.load(Ordering::Relaxed));
    defmt::info!("  PMP Memory Violations = {}", METRIC_PMP_VIOLATIONS.load(Ordering::Relaxed));
    defmt::info!("=============================");
}
