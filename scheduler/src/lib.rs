//! Task scheduling and context switching subsystem.
#![cfg_attr(not(test), no_std)]

#[cfg(kani)]
extern crate kani;

pub mod bitmap;
pub mod tcb;

pub use bitmap::BitMapScheduler;
pub use tcb::{Capability, TaskControlBlock, TaskState};
