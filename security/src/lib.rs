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

pub mod defmt {
    extern "C" {
        pub fn SEGGER_RTT_WriteNoLock(channel: usize, ptr: *const u8, len: usize) -> usize;
    }

    pub struct RttWriter;

    impl core::fmt::Write for RttWriter {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            unsafe {
                SEGGER_RTT_WriteNoLock(0, s.as_ptr(), s.len());
            }
            Ok(())
        }
    }

    #[macro_export]
    macro_rules! rtt_println {
        ($($arg:tt)*) => {
            {
                use core::fmt::Write;
                let mut writer = $crate::defmt::RttWriter;
                let _ = writeln!(&mut writer, $($arg)*);
            }
        };
    }

    pub use rtt_println as info;
    pub use rtt_println as warn;
    pub use rtt_println as error;
    pub use rtt_println as trace;
}
