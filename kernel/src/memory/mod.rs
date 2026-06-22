//! Memory management and hardware protection subsystem.

pub mod pmp;

pub use pmp::{configure_pmp, reprogram_pmp_stack, PmpAddressMode, PmpConfig};
