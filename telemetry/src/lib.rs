#![no_std]

#[cfg(kani)]
extern crate kani;

pub mod syscalls;

use serde::Serialize;

#[derive(Serialize, Debug, Clone, Copy)]
pub enum TraceEvent {
    TaskSwap { from: u8, to: u8, cycles: u32 },
    IpcTransfer { endpoint: u8, bytes: u8 },
    FaultInterception { cause: u32, pc: u32 },
}

extern "C" {
    pub fn SEGGER_RTT_WriteNoLock(channel: usize, ptr: *const u8, len: usize) -> usize;
}

#[inline(always)]
fn get_hart_id() -> usize {
    #[cfg(not(kani))]
    {
        let hart_id: usize;
        unsafe {
            core::arch::asm!("csrr {}, mhartid", out(reg) hart_id);
        }
        hart_id
    }
    #[cfg(kani)]
    {
        0
    }
}

pub fn log_telemetry(event: &TraceEvent) {
    #[cfg(not(kani))]
    {
        static mut BUFS: [[u8; 32]; 2] = [[0; 32]; 2];
        let hart_id = get_hart_id() & 1;
        unsafe {
            if let Ok(used) = postcard::to_slice(event, &mut BUFS[hart_id]) {
                SEGGER_RTT_WriteNoLock(1, used.as_ptr(), used.len());
            }
        }
    }
    #[cfg(kani)]
    {
        let _ = event;
    }
}
