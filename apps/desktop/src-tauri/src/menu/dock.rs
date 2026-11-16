//! The Dock icon's menu: the Remote menu's lines (`macos::remote_items`) below the open windows
//! the system lists there itself. Tauri offers no Dock menu, so this answers AppKit's
//! `applicationDockMenu:` on tao's application delegate with a menu built by `muda`, the menu
//! library Tauri builds on: choosing a line reaches `on_menu_event` in `lib.rs` like any menu item
//! of the menu bar, Tauri handing every `muda` menu event to it.

use std::cell::RefCell;

use muda::{ContextMenu, Menu, MenuItem, PredefinedMenuItem};
use tauri::AppHandle;

use super::macos::{remote_items, Labels, RemoteItem};
use crate::diagnostics::diagnostic;

thread_local! {
    /// The Dock menu, built on the main thread and handed to AppKit from it; kept alive here for as
    /// long as AppKit may show it.
    static DOCK_MENU: RefCell<Option<Menu>> = const { RefCell::new(None) };
}

/// Rebuilds the Dock menu with `labels` and the saved remote daemons `entries`. On the main thread.
pub fn rebuild(labels: &Labels, entries: &[(String, String)]) {
    let menu = Menu::new();
    for item in remote_items(labels, entries) {
        let appended = match item {
            RemoteItem::Item { id, text, enabled } => {
                menu.append(&MenuItem::with_id(id, text, enabled, None))
            }
            RemoteItem::Separator => menu.append(&PredefinedMenuItem::separator()),
        };
        if let Err(err) = appended {
            diagnostic!("cannot build the Dock menu: {err}");
            return;
        }
    }
    DOCK_MENU.with(|dock| *dock.borrow_mut() = Some(menu));
}

/// Adds `applicationDockMenu:` to tao's application delegate, which answers it with the menu
/// `rebuild` made. tao's delegate does not implement the selector (as with
/// `applicationShouldTerminate:` in `lifecycle/macos.rs`), so this adds a method rather than
/// overriding one; a tao that implements it keeps its own, and the Dock lists only the windows.
/// From `setup`, on the main thread.
pub fn install(_app: &AppHandle) {
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::{Encode, MainThreadMarker};
    use objc2_app_kit::NSApplication;

    let Some(mtm) = MainThreadMarker::new() else {
        diagnostic!("the Dock menu is installed off the main thread; leaving it out");
        return;
    };
    let Some(delegate) = NSApplication::sharedApplication(mtm).delegate() else {
        diagnostic!("there is no application delegate to give a Dock menu");
        return;
    };
    let delegate: &AnyObject = delegate.as_ref();
    let class = delegate.class() as *const AnyClass as *mut _;
    // SAFETY: `class` is tao's own, registered delegate class; the method added takes and returns
    // what AppKit's declaration of `applicationDockMenu:` does (the application, and an `NSMenu`
    // AppKit does not take ownership of), and its type encoding is built from the same `Encode`
    // impls.
    unsafe {
        let types = std::ffi::CString::new(format!(
            "{}{}{}{}",
            <*mut AnyObject>::ENCODING,
            <*mut AnyObject>::ENCODING,
            Sel::ENCODING,
            <&NSApplication>::ENCODING,
        ))
        .expect("encoding string has no interior NUL");
        let imp: Imp = std::mem::transmute::<
            unsafe extern "C-unwind" fn(&AnyObject, Sel, &NSApplication) -> *mut AnyObject,
            Imp,
        >(application_dock_menu);
        let added = objc2::ffi::class_addMethod(
            class,
            objc2::sel!(applicationDockMenu:),
            imp,
            types.as_ptr(),
        );
        if !added.as_bool() {
            diagnostic!("applicationDockMenu: is already implemented on tao's delegate");
        }
    }
}

/// The method added above: the Dock menu, or none before it is built. Never unwinds into AppKit.
unsafe extern "C-unwind" fn application_dock_menu(
    _this: &objc2::runtime::AnyObject,
    _sel: objc2::runtime::Sel,
    _sender: &objc2_app_kit::NSApplication,
) -> *mut objc2::runtime::AnyObject {
    std::panic::catch_unwind(|| {
        DOCK_MENU.with(|dock| {
            dock.borrow()
                .as_ref()
                .map_or(std::ptr::null_mut(), |menu| menu.ns_menu().cast())
        })
    })
    .unwrap_or(std::ptr::null_mut())
}
