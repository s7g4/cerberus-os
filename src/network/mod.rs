//! Network communication subsystem.

pub mod can;

pub use can::{CanError, CanFrame, CanRingBuffer};
