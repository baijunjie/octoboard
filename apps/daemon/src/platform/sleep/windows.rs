//! The hold on Windows: a power request that asks for the system and the display to stay on, which
//! `powercfg /requests` lists under the reason. A power request belongs to the process, not to a
//! thread as `SetThreadExecutionState`'s flags do, so it needs no thread of its own to live on,
//! and closing its handle is what releases it. The display part is best effort: a session with no
//! display (a service, an SSH session in Session 0) refuses it with error 50 while still granting
//! the system part, and keeping the machine from sleeping is what the setting is for.

use anyhow::{bail, Result};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Power::{
    PowerCreateRequest, PowerRequestDisplayRequired, PowerRequestSystemRequired, PowerSetRequest,
};
use windows_sys::Win32::System::Threading::{
    POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
};

/// `POWER_REQUEST_CONTEXT_VERSION`, which `windows-sys` does not carry.
const POWER_REQUEST_CONTEXT_VERSION: u32 = 0;

pub(super) struct Held {
    request: HANDLE,
}

// The handle is only a token the system resolves; any thread may clear and close it.
unsafe impl Send for Held {}

impl Held {
    pub(super) fn acquire(reason: &str) -> Result<Self> {
        let mut reason: Vec<u16> = reason.encode_utf16().chain(Some(0)).collect();
        let context = REASON_CONTEXT {
            Version: POWER_REQUEST_CONTEXT_VERSION,
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 {
                SimpleReasonString: reason.as_mut_ptr(),
            },
        };
        // SAFETY: `context` and the string it points to outlive the call, which copies the string.
        let request = unsafe { PowerCreateRequest(&context) };
        if request.is_null() || request == INVALID_HANDLE_VALUE {
            // SAFETY: reads the calling thread's last error.
            let error = unsafe { GetLastError() };
            bail!("the power request was refused (error {error})");
        }
        // SAFETY: `request` is the handle just created.
        if unsafe { PowerSetRequest(request, PowerRequestSystemRequired) } == 0 {
            // SAFETY: reads the calling thread's last error; the closed handle is not used again.
            let error = unsafe { GetLastError() };
            unsafe { CloseHandle(request) };
            bail!("the power request was refused (error {error})");
        }
        // SAFETY: `request` is the handle just created, still open.
        if unsafe { PowerSetRequest(request, PowerRequestDisplayRequired) } == 0 {
            // SAFETY: reads the calling thread's last error.
            let error = unsafe { GetLastError() };
            tracing::warn!(
                error,
                "keeping the display on was refused; holding the system only"
            );
        }
        Ok(Self { request })
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        // Closing the handle clears every request made through it.
        // SAFETY: the handle is this hold's own and is not used again.
        unsafe { CloseHandle(self.request) };
    }
}

pub(super) fn available() -> bool {
    true
}
