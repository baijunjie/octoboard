//! The hold on Windows: a power request that asks for the system and the display to stay on, which
//! `powercfg /requests` lists under the reason below. A power request belongs to the process, not
//! to a thread as `SetThreadExecutionState`'s flags do, so it needs no thread of its own to live
//! on, and closing its handle is what releases it. Linux has nothing to hold: the request is not
//! made there.

/// `POWER_REQUEST_CONTEXT_VERSION`, which `windows-sys` does not carry.
#[cfg(windows)]
const POWER_REQUEST_CONTEXT_VERSION: u32 = 0;

/// What `powercfg /requests` shows beside Octoboard's process.
#[cfg(windows)]
const REASON: &str = "Octoboard is keeping the computer awake";

#[cfg(windows)]
pub struct Hold {
    request: windows_sys::Win32::Foundation::HANDLE,
}

// The handle is only a token the system resolves; any thread may clear and close it.
#[cfg(windows)]
unsafe impl Send for Hold {}

#[cfg(windows)]
impl Hold {
    pub fn acquire() -> Result<Self, String> {
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Power::{
            PowerCreateRequest, PowerRequestDisplayRequired, PowerRequestSystemRequired,
            PowerSetRequest,
        };
        use windows_sys::Win32::System::Threading::{
            POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
        };

        let mut reason: Vec<u16> = REASON.encode_utf16().chain(Some(0)).collect();
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
            return Err(format!(
                "the power request was refused (error {})",
                unsafe { GetLastError() }
            ));
        }
        for kind in [PowerRequestDisplayRequired, PowerRequestSystemRequired] {
            // SAFETY: `request` is the handle just created.
            if unsafe { PowerSetRequest(request, kind) } == 0 {
                // SAFETY: as above; the closed handle is not used again.
                let error = unsafe { GetLastError() };
                unsafe { CloseHandle(request) };
                return Err(format!("the power request was refused (error {error})"));
            }
        }
        Ok(Self { request })
    }
}

#[cfg(windows)]
impl Drop for Hold {
    fn drop(&mut self) {
        // Closing the handle clears every request made through it.
        // SAFETY: the handle is this hold's own and is not used again.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.request) };
    }
}

/// Nothing is held where there is no power request to make.
#[cfg(not(windows))]
pub struct Hold;

#[cfg(not(windows))]
impl Hold {
    pub fn acquire() -> Result<Self, String> {
        Ok(Self)
    }
}
