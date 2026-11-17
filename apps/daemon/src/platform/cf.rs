//! The few CoreFoundation calls the macOS parts of the platform layer share: a retained object that
//! is released when it is dropped, and the strings and URLs made from text. Declared by hand rather
//! than taken from a crate, as the calls are a handful of plain C entry points.

use std::ffi::c_void;

mod ffi {
    use std::ffi::c_void;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        /// With `allocator` null (the default one), `bytes` of `length` bytes in `encoding`, and
        /// `is_external` false, which makes the string copy them.
        pub fn CFStringCreateWithBytes(
            allocator: *const c_void,
            bytes: *const u8,
            length: isize,
            encoding: u32,
            is_external: u8,
        ) -> *const c_void;
        /// A URL from a `CFString` and no base, or null where the string is not a URL.
        pub fn CFURLCreateWithString(
            allocator: *const c_void,
            url: *const c_void,
            base: *const c_void,
        ) -> *const c_void;
        pub fn CFRelease(object: *const c_void);
    }
}

/// `kCFStringEncodingUTF8`.
const ENCODING_UTF8: u32 = 0x0800_0100;

/// A retained CoreFoundation object (a `CFString`, a `CFURL`, an error), released when it is
/// dropped.
pub(crate) struct Object(*const c_void);

impl Object {
    /// Takes over `object`, which a call that returns a retained one made; `None` for a null.
    ///
    /// # Safety
    /// `object` is null or a retained CoreFoundation object nothing else releases.
    pub(crate) unsafe fn own(object: *const c_void) -> Option<Self> {
        (!object.is_null()).then(|| Self(object))
    }

    /// A `CFString` of `text`, copied.
    pub(crate) fn string(text: &str) -> Option<Self> {
        // SAFETY: `text` is valid for its length for the call, which copies it, and the result is
        // retained.
        unsafe {
            Self::own(ffi::CFStringCreateWithBytes(
                std::ptr::null(),
                text.as_ptr(),
                text.len() as isize,
                ENCODING_UTF8,
                0,
            ))
        }
    }

    /// A `CFURL` of the text this `CFString` holds, `None` where it is not a URL.
    pub(crate) fn url(&self) -> Option<Self> {
        // SAFETY: the string outlives the call, which retains what it keeps, and the result is
        // retained.
        unsafe {
            Self::own(ffi::CFURLCreateWithString(
                std::ptr::null(),
                self.0,
                std::ptr::null(),
            ))
        }
    }

    /// The object for a call that takes one.
    pub(crate) fn as_ptr(&self) -> *const c_void {
        self.0
    }
}

impl Drop for Object {
    fn drop(&mut self) {
        // SAFETY: the object is this value's own, from a call that returns a retained one.
        unsafe { ffi::CFRelease(self.0) };
    }
}
