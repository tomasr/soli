//! Waiting for input while the table is at rest, so an idle game draws no
//! frames at all.

use std::time::Duration;

/// Blocks until a window message (input, resize, close, ...) arrives or
/// `timeout` passes, without taking the message off the queue: the macroquad
/// loop handles it as usual. Returns true if it woke for a message.
#[cfg(windows)]
pub fn wait_for_input(timeout: Duration) -> bool {
    const QS_ALLINPUT: u32 = 0x04FF;
    // Also wake for messages already in the queue that a peek has seen but
    // not removed, not only for ones that arrive from now on.
    const MWMO_INPUTAVAILABLE: u32 = 0x0004;
    const WAIT_TIMEOUT: u32 = 0x0102;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MsgWaitForMultipleObjectsEx(
            count: u32,
            handles: *const isize,
            milliseconds: u32,
            wake_mask: u32,
            flags: u32,
        ) -> u32;
    }

    let ms = timeout.as_millis().min(u32::MAX as u128 - 1) as u32;
    // SAFETY: no handles are passed, so the null pointer is never read.
    let result = unsafe {
        MsgWaitForMultipleObjectsEx(0, std::ptr::null(), ms, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
    };
    result != WAIT_TIMEOUT
}

/// Without a way to wait for window messages, keep drawing at the idle frame
/// rate.
#[cfg(not(windows))]
pub fn wait_for_input(_timeout: Duration) -> bool {
    false
}
