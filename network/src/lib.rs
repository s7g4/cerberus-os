//! Network communication subsystem.

#![cfg_attr(not(test), no_std)]

#[cfg(kani)]
extern crate kani;

pub mod can;

pub use can::{CanError, CanFrame, CanRingBuffer};
