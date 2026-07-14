#![cfg_attr(not(test), no_std)]

#[cfg(kani)]
extern crate kani;

pub mod syscalls;

use serde::Serialize;

#[derive(Serialize, Debug, Clone, Copy)]
#[cfg_attr(test, derive(serde::Deserialize, PartialEq))]
pub enum TraceEvent {
    TaskSwap { from: u8, to: u8, cycles: u32 },
    IpcTransfer { endpoint: u8, bytes: u8 },
    FaultInterception { cause: u32, pc: u32 },
}

extern "C" {
    pub fn SEGGER_RTT_WriteNoLock(channel: usize, ptr: *const u8, len: usize) -> usize;
}

// Gated on `target_arch` (not just `kani`, matching the same fix already
// applied in `syscalls.rs`): `csrr` is a RISC-V-only mnemonic and
// `SEGGER_RTT_WriteNoLock` is only ever linked on the bare-metal target, so
// both the real body and the extern symbol reference must be absent when
// this crate is compiled for a host test/dev build, not just for `kani`.
#[inline(always)]
fn get_hart_id() -> usize {
    #[cfg(all(target_arch = "riscv32", not(kani)))]
    {
        let hart_id: usize;
        unsafe {
            core::arch::asm!("csrr {}, mhartid", out(reg) hart_id);
        }
        hart_id
    }
    #[cfg(any(kani, not(target_arch = "riscv32")))]
    {
        0
    }
}

pub fn log_telemetry(event: &TraceEvent) {
    #[cfg(all(target_arch = "riscv32", not(kani)))]
    {
        static mut BUFS: [[u8; 32]; 2] = [[0; 32]; 2];
        let hart_id = get_hart_id() & 1;
        unsafe {
            if let Ok(used) = postcard::to_slice(event, &mut BUFS[hart_id]) {
                SEGGER_RTT_WriteNoLock(1, used.as_ptr(), used.len());
            }
        }
    }
    #[cfg(any(kani, not(target_arch = "riscv32")))]
    {
        let _ = event;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(event: TraceEvent) {
        let mut buf = [0u8; 32];
        let used = postcard::to_slice(&event, &mut buf).unwrap();
        let decoded: TraceEvent = postcard::from_bytes(used).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn task_swap_round_trips() {
        round_trip(TraceEvent::TaskSwap {
            from: 1,
            to: 2,
            cycles: 12345,
        });
    }

    #[test]
    fn ipc_transfer_round_trips() {
        round_trip(TraceEvent::IpcTransfer {
            endpoint: 2,
            bytes: 64,
        });
    }

    #[test]
    fn fault_interception_round_trips() {
        round_trip(TraceEvent::FaultInterception {
            cause: 6,
            pc: 0x2000_1234,
        });
    }
}
