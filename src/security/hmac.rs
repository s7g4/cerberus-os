//! HMAC-SHA256 message authentication for CAN frames.

use crate::network::can::CanFrame;
use sha2::{Digest, Sha256};

const HMAC_KEY: &[u8; 32] = b"cerberus-os-dev-key-not-for-prod";
pub const HMAC_TAG_LEN: usize = 8;

/// A CAN frame paired with its message authentication code.
pub struct AuthFrame {
    pub frame: CanFrame,
    pub tag: [u8; HMAC_TAG_LEN],
}

/// Computes a 64-bit truncated HMAC-SHA256 over a CAN frame.
///
/// Truncating to 64 bits allows the signature to fit inside standard
/// real-time communication frames while maintaining high resistance to forging.
pub fn compute_hmac(frame: &CanFrame) -> [u8; HMAC_TAG_LEN] {
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

/// Verifies if a received frame has a valid signature.
///
/// Uses constant-time execution comparison to mitigate side-channel timing attacks.
pub fn verify_frame(auth: &AuthFrame) -> bool {
    let expected = compute_hmac(&auth.frame);
    // Constant-time accumulation to prevent early exit timing leaks
    expected
        .iter()
        .zip(auth.tag.iter())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}
