//! Cryptographic security and frame authentication subsystem.
#![no_std]

#[cfg(kani)]
extern crate kani;

pub mod bootloader;
pub mod hmac;
pub mod hsm;

pub use bootloader::{verify_secure_boot, verify_tampered_secure_boot};
pub use hmac::{verify_frame_secure, AuthFrame};
pub use hsm::hsm_task;
