//! The macOS hold: two IOKit power assertions, one against idle display sleep, which is what ends
//! in the lock screen, and one against idle system sleep. The display one alone would hold the
//! system too, but each is named for what it is for, and `pmset -g assertions` lists both under
//! Octoboard's name. The calls are declared by hand rather than taken from a crate: a few plain C
//! entry points of IOKit, with CoreFoundation's strings from `platform::cf`.

use anyhow::{bail, Result};

use crate::platform::cf::Object;

mod ffi {
    use std::ffi::c_void;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        /// `assertion_type` and `name` are `CFStringRef`s; `level` is `kIOPMAssertionLevelOn`.
        pub fn IOPMAssertionCreateWithName(
            assertion_type: *const c_void,
            level: u32,
            name: *const c_void,
            assertion_id: *mut u32,
        ) -> i32;
        pub fn IOPMAssertionRelease(assertion_id: u32) -> i32;
    }
}

/// `kIOPMAssertionLevelOn`.
const LEVEL_ON: u32 = 255;
/// The strings the `kIOPMAssertPreventUserIdle...` constants spell, which are macros over `CFSTR`
/// and so cannot be linked to.
const PREVENT_DISPLAY_SLEEP: &str = "PreventUserIdleDisplaySleep";
const PREVENT_SYSTEM_SLEEP: &str = "PreventUserIdleSystemSleep";

/// The assertions taken, released when it is dropped.
pub(super) struct Held {
    ids: [u32; 2],
}

impl Held {
    pub(super) fn acquire(reason: &str) -> Result<Self> {
        let first = create(PREVENT_DISPLAY_SLEEP, reason)?;
        match create(PREVENT_SYSTEM_SLEEP, reason) {
            Ok(second) => Ok(Self {
                ids: [first, second],
            }),
            Err(err) => {
                release(first);
                Err(err)
            }
        }
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        self.ids.into_iter().for_each(release);
    }
}

pub(super) fn available() -> bool {
    true
}

fn create(assertion_type: &str, reason: &str) -> Result<u32> {
    let string = |text: &str| {
        Object::string(text)
            .ok_or_else(|| anyhow::anyhow!("a string for the power assertion could not be made"))
    };
    let assertion_type_string = string(assertion_type)?;
    let name = string(reason)?;
    let mut id = 0u32;
    // SAFETY: both strings outlive the call, which copies what it keeps, and `id` is a valid
    // place for the assertion's id.
    let status = unsafe {
        ffi::IOPMAssertionCreateWithName(
            assertion_type_string.as_ptr(),
            LEVEL_ON,
            name.as_ptr(),
            &mut id,
        )
    };
    if status == 0 {
        Ok(id)
    } else {
        bail!("the power assertion {assertion_type} was refused (IOReturn {status:#x})")
    }
}

fn release(id: u32) {
    // SAFETY: `id` is one this module created and has not released.
    unsafe { ffi::IOPMAssertionRelease(id) };
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::platform::sleep::REASON;

    /// The lines of `pmset`'s listing that belong to this process, so an Octoboard running beside
    /// the test, holding the same name, does not count.
    fn listed_assertions() -> String {
        let output = Command::new("pmset")
            .args(["-g", "assertions"])
            .output()
            .expect("pmset runs");
        let own = format!("pid {}(", std::process::id());
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| line.contains(&own))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The hold is what `pmset` lists under the name for this process, for both kinds of idle
    /// sleep, and the listing is clean again once it is dropped.
    #[test]
    fn a_hold_is_listed_by_pmset_until_it_is_dropped() {
        let hold = Held::acquire(REASON).expect("the assertions are granted");
        let held = listed_assertions();
        assert!(held.contains(REASON));
        assert!(held.contains(PREVENT_DISPLAY_SLEEP));
        assert!(held.contains(PREVENT_SYSTEM_SLEEP));
        drop(hold);
        assert!(!listed_assertions().contains(REASON));
    }
}
