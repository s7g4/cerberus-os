//! Cryptographic security and frame authentication subsystem.

pub mod hmac;

pub use hmac::{compute_hmac, verify_frame, AuthFrame};
