use std::io;

use windows_sys::Win32::Security::Cryptography::ProcessPrng;

/// Fills `buffer` through `ProcessPrng`, the system's per-process generator, which cannot fail.
pub(crate) fn fill(buffer: &mut [u8]) -> io::Result<()> {
    // SAFETY: the pointer and length are those of `buffer`.
    if unsafe { ProcessPrng(buffer.as_mut_ptr(), buffer.len()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
