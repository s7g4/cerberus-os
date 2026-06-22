//! HMAC-SHA256 message authentication for CAN frames.

use network::can::CanFrame;
use telemetry::syscalls::{sys_recv, sys_send};

pub const HMAC_TAG_LEN: usize = 8;

/// A CAN frame paired with its message authentication code.
pub struct AuthFrame {
    pub frame: CanFrame,
    pub tag: [u8; HMAC_TAG_LEN],
}

/// Verifies if a received frame has a valid signature by requesting the HSM to sign the frame via IPC.
///
/// Uses constant-time execution comparison to mitigate side-channel timing attacks.
pub fn verify_frame_secure(auth: &AuthFrame, hsm_send_cap: usize, hsm_recv_cap: usize) -> bool {
    let frame_bytes = unsafe {
        core::slice::from_raw_parts(
            &auth.frame as *const CanFrame as *const u8,
            core::mem::size_of::<CanFrame>(),
        )
    };

    // Send frame to HSM for signing
    if sys_send(hsm_send_cap, frame_bytes) < 0 {
        return false;
    }

    // Receive computed signature tag back from HSM
    let mut expected = [0u8; HMAC_TAG_LEN];
    if sys_recv(hsm_recv_cap, &mut expected) < 0 {
        return false;
    }

    // Constant-time accumulation to prevent early exit timing leaks
    expected
        .iter()
        .zip(auth.tag.iter())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

