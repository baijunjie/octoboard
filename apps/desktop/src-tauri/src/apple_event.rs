//! Reading the Apple event AppKit is handling now: the enum code of one of its attributes or
//! parameters, which is how a quit says why it was asked for and a launch says what opened the app.
//! The calls are made by hand through `objc2`'s message sends, as the framework has no binding
//! among the crates this shell already depends on.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};

/// Where in the event a value sits.
#[derive(Clone, Copy)]
pub enum Slot {
    /// An attribute, such as `keyAEQuitReason`'s usual place.
    Attribute,
    /// A parameter, such as `keyAEPropData`.
    Param,
}

/// The enum code under `keyword` in the Apple event being handled now, `None` when no event is
/// current or it has nothing there. Meaningful on the main thread, inside the handling of the
/// event.
pub fn current_enum_code(keyword: u32, slot: Slot) -> Option<u32> {
    // SAFETY: plain message sends on Foundation objects; each answer is checked for nil.
    unsafe {
        let manager: Option<Retained<AnyObject>> =
            msg_send![class!(NSAppleEventManager), sharedAppleEventManager];
        let event: Option<Retained<AnyObject>> = msg_send![&*manager?, currentAppleEvent];
        let event = event?;
        let descriptor: Option<Retained<AnyObject>> = match slot {
            Slot::Attribute => msg_send![&*event, attributeDescriptorForKeyword: keyword],
            Slot::Param => msg_send![&*event, paramDescriptorForKeyword: keyword],
        };
        let descriptor = descriptor?;
        Some(msg_send![&*descriptor, enumCodeValue])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_current_event_gives_no_code() {
        let keyword = u32::from_be_bytes(*b"why?");
        assert_eq!(current_enum_code(keyword, Slot::Attribute), None);
        assert_eq!(current_enum_code(keyword, Slot::Param), None);
    }
}
