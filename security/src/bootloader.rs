//! Secure Bootloader (SBL) Verification.
//!
//! Checks the integrity of a trusted kernel payload using SHA-256, plus a
//! lightweight (non-cryptographic) linkage check between a stored public key
//! and signature buffer.
//!
//! # Honest scope
//!
//! This is **not** ECDSA or any other asymmetric signature scheme. An
//! earlier revision used `p256` for real ECDSA-P256 verification but it
//! pushed `.text` past the 32 KB budget (see `DEVLOG.md` Milestone 19), and
//! was replaced with the checksum below. The checksum has no cryptographic
//! soundness: it is trivially satisfiable by any attacker who can choose
//! both buffers, and provides no protection against a forged image. What it
//! *does* demonstrate correctly is the hash-based tamper-detection half of
//! the pipeline: `verify_tampered_secure_boot` mutates a real copy of the
//! trusted payload and confirms the hash comparison rejects it, rather than
//! comparing two unrelated precomputed constants.
//!
//! A production version of this design would need a real signature scheme
//! sized to fit the budget (e.g. Ed25519 with a `no_std`, allocation-free
//! implementation) in place of the checksum.

use sha2::{Digest, Sha256};

/// Hardcoded SEC1 uncompressed public key coordinates (X and Y concatenated).
/// Not used for any asymmetric verification -- see module docs.
const PUB_KEY_BYTES: &[u8; 64] = &[
    0xb7, 0x0f, 0x96, 0x22, 0xf9, 0x76, 0x5f, 0x49, 0xda, 0x3b, 0x73, 0xe9, 0xff, 0xc2, 0xa1, 0xfb,
    0xed, 0x84, 0x46, 0xe4, 0x4d, 0x81, 0x53, 0x15, 0xc7, 0x8f, 0xcc, 0xb5, 0xe0, 0x2f, 0xf1, 0x78,
    0x77, 0xcc, 0xbf, 0x02, 0xc4, 0xf8, 0x2a, 0x54, 0xf9, 0xbc, 0x3b, 0x36, 0x77, 0x60, 0x06, 0xe6,
    0xe1, 0xeb, 0x08, 0xb8, 0x92, 0x48, 0x1e, 0xfd, 0x7d, 0x52, 0x30, 0x53, 0x4f, 0x3b, 0xf4, 0x56,
];

/// Hardcoded signature-shaped buffer (R and S components concatenated).
/// Not a real signature -- see module docs.
const SIGNATURE_BYTES: &[u8; 64] = &[
    0x53, 0xd8, 0x26, 0x28, 0xea, 0x32, 0x36, 0x48, 0xb1, 0xba, 0x1e, 0x6d, 0x16, 0x99, 0x9e, 0x5b,
    0xdd, 0xc2, 0x4c, 0x35, 0x5a, 0x9b, 0x0c, 0x28, 0xb9, 0x84, 0xcb, 0x96, 0x53, 0x7e, 0x37, 0xe6,
    0xd3, 0x5d, 0x8c, 0x60, 0xd1, 0xcb, 0xde, 0xde, 0x51, 0xd3, 0xb5, 0x62, 0x6c, 0xc5, 0xba, 0x1d,
    0x31, 0xc6, 0x35, 0xda, 0x91, 0x38, 0x5b, 0xa5, 0x58, 0xb8, 0x35, 0xa5, 0x51, 0x2e, 0xca, 0x8c,
];

/// The trusted kernel payload this boot stage checks against. A real
/// implementation would hash the actual flashed image (e.g. the linked
/// `.text`/`.data` byte range); this checks a fixed message instead, so it
/// demonstrates the containment flow rather than verifying the real binary.
const TRUSTED_PAYLOAD: &[u8] = b"cerberus-os-kernel-image-data-payload-for-verification-v1.0";

const EXPECTED_HASH: [u8; 32] = [
    0xcf, 0xca, 0x73, 0xa3, 0xd7, 0x2f, 0x46, 0x25, 0xf8, 0x54, 0xec, 0xb8, 0xe0, 0xab, 0x75, 0x96,
    0x1d, 0x38, 0xb9, 0xdd, 0xa2, 0x97, 0x42, 0x21, 0xde, 0xe5, 0xdb, 0xe9, 0x46, 0xe5, 0xee, 0x4f,
];

fn sha256_matches(payload: &[u8]) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    hasher.finalize().as_slice() == EXPECTED_HASH
}

/// Non-cryptographic linkage check between the public key and signature
/// buffers. See module docs: this is not a substitute for real signature
/// verification, only a placeholder that fits the size budget.
fn key_signature_linkage_ok() -> bool {
    let mut checksum = 0u8;
    for i in 0..64 {
        checksum ^= PUB_KEY_BYTES[i] ^ SIGNATURE_BYTES[i];
    }
    checksum == 0x76
}

/// Verifies the trusted payload's hash and the key/signature linkage.
pub fn verify_secure_boot() -> bool {
    sha256_matches(TRUSTED_PAYLOAD) && key_signature_linkage_ok()
}

/// Demonstrates tamper detection: flips one byte in a real copy of the
/// trusted payload and confirms the hash comparison correctly rejects it,
/// rather than comparing against an unrelated second constant.
pub fn verify_tampered_secure_boot() -> bool {
    let mut tampered = [0u8; TRUSTED_PAYLOAD.len()];
    tampered.copy_from_slice(TRUSTED_PAYLOAD);
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;

    sha256_matches(&tampered) && key_signature_linkage_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_payload_hash_matches() {
        assert!(sha256_matches(TRUSTED_PAYLOAD));
    }

    #[test]
    fn single_byte_tamper_is_detected() {
        let mut tampered = [0u8; TRUSTED_PAYLOAD.len()];
        tampered.copy_from_slice(TRUSTED_PAYLOAD);
        tampered[0] ^= 0x01;
        assert!(!sha256_matches(&tampered));
    }

    #[test]
    fn verify_secure_boot_passes_on_untampered_payload() {
        assert!(verify_secure_boot());
    }

    #[test]
    fn verify_tampered_secure_boot_rejects_the_mutated_copy() {
        // The function name says "verify" but its contract is "detect
        // tampering", so a healthy boot stage returns false here.
        assert!(!verify_tampered_secure_boot());
    }
}
