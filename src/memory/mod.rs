//! Memory management and hardware protection subsystem.

pub mod pmp;

pub use pmp::{configure_pmp, PmpAddressMode, PmpConfig};
