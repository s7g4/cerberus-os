//! Virtual Hardware Security Module (vHSM) Partition.
//!
//! Stores private cryptographic keys and processes HMAC signing requests in U-mode.

use network::can::CanFrame;
use sha2::{Digest, Sha256};
use telemetry::syscalls::{sys_recv, sys_send};

/// The private HMAC key, strictly isolated inside the HSM partition.
const HMAC_KEY: &[u8; 32] = b"cerberus-os-dev-key-not-for-prod";
pub const HMAC_TAG_LEN: usize = 8;

/// Computes a 64-bit truncated HMAC-SHA256 over a CAN frame.
fn compute_hmac_local(frame: &CanFrame) -> [u8; HMAC_TAG_LEN] {
    let mut ipad_key = [0x36u8; 64];
    let mut opad_key = [0x5Cu8; 64];
    for i in 0..32 {
        ipad_key[i] ^= HMAC_KEY[i];
        opad_key[i] ^= HMAC_KEY[i];
    }

    // Inner hash: H((K ^ ipad) || message)
    let mut inner = Sha256::new();
    inner.update(ipad_key);

    // Serialize frame fields for hashing
    inner.update([frame.id as u8, (frame.id >> 8) as u8, frame.dlc]);
    inner.update(&frame.payload[..frame.dlc as usize]);
    let inner_hash = inner.finalize();

    // Outer hash: H((K ^ opad) || inner_hash)
    let mut outer = Sha256::new();
    outer.update(opad_key);
    outer.update(inner_hash);
    let full_mac = outer.finalize();

    let mut tag = [0u8; HMAC_TAG_LEN];
    tag.copy_from_slice(&full_mac[..HMAC_TAG_LEN]);
    tag
}

/// HSM Task Entry point (runs as a secure U-mode partition).
pub extern "C" fn hsm_task() -> ! {
    defmt::info!("HSM Partition: Secure cryptographic partition started.");

    let mut req_buf = [0u8; core::mem::size_of::<CanFrame>()];

    loop {
        // Wait for an IPC request on endpoint 2 (receives a CanFrame to sign)
        let res = sys_recv(2, &mut req_buf);
        if res == core::mem::size_of::<CanFrame>() as isize {
            // Reconstruct the CanFrame from raw bytes safely
            let frame: CanFrame =
                unsafe { core::ptr::read_unaligned(req_buf.as_ptr() as *const CanFrame) };

            // Compute tag locally using the isolated key
            let tag = compute_hmac_local(&frame);

            // Send the computed signature tag back to endpoint 3
            let send_res = sys_send(3, &tag);
            if send_res < 0 {
                defmt::error!(
                    "HSM Partition: Failed to send signature response: {}",
                    send_res
                );
            }
        } else {
            defmt::error!(
                "HSM Partition: Received invalid request payload size: {}",
                res
            );
        }
    }
}

