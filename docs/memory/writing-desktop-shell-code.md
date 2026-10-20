# Writing desktop shell code

## To run something after the current AppKit callback returns, queue it on the main `NSOperationQueue`

In the desktop shell (`apps/desktop/src-tauri/`), `AppHandle::run_on_main_thread` only queues the closure when it is
called from another thread. Called on the main thread — from a notification observer, a menu event, an
`NSApplication` delegate method or anything else AppKit calls into — it runs the closure at once, before the call
returns (`send_user_message` in `tauri-runtime-wry`, as of 2.12.2). So it cannot put work after the callback in
progress, whatever the name suggests.

When work on the main thread has to wait until the current callback has returned (for example until a menu has
finished delivering the chosen item's action), queue it with `NSOperationQueue::mainQueue().addOperationWithBlock`
(the `NSOperation` feature of `objc2-foundation`, already enabled), and read any state the work depends on inside the
block rather than when queueing it. From a background thread, `run_on_main_thread` is still the way onto the main
thread.

## After raising Tauri, tray-icon or muda, check each native hook into their internals in the running app

Applies to any change that moves `tauri`, `tray-icon` or `muda` in `Cargo.lock`, including a move pulled in only
transitively by raising Tauri (the 2.11 to 2.12 raise took tray-icon from 0.24 to 0.25). The shell reaches past their
public API to the AppKit objects they own (`TrayIcon::with_inner_tray_icon` with `ns_status_item`, `ns_window`), and
those hooks give up quietly when an accessor comes back empty, so a change inside the crate still compiles and passes
the tests while the hook does nothing: after that raise every finishing touch on the tray menu vanished with no
error. So with such a raise, find the hooks (search `apps/desktop/src-tauri/src/` for `with_inner_tray_icon`,
`ns_status_item` and `ns_window`), compare what each relies on between the old and new crate sources, and confirm in
the app that each still has its effect.

When writing such a hook, take the AppKit object from the notification or delegate call that hands it over (the
`NSMenu` that is the object of `NSMenuDidBeginTrackingNotification`, say) rather than reading it back through the
crate's internals, wherever one is delivered.
