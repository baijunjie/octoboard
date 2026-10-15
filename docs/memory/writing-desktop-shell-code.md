# Writing desktop shell code

## To run something after the current AppKit callback returns, queue it on the main `NSOperationQueue`

In the desktop shell (`apps/desktop/src-tauri/`), `AppHandle::run_on_main_thread` only queues the closure when it is
called from another thread. Called on the main thread — from a notification observer, a menu event, an
`NSApplication` delegate method or anything else AppKit calls into — it runs the closure at once, before the call
returns (`send_user_message` in `tauri-runtime-wry`, as of 2.11.4). So it cannot put work after the callback in
progress, whatever the name suggests.

When work on the main thread has to wait until the current callback has returned (for example until a menu has
finished delivering the chosen item's action), queue it with `NSOperationQueue::mainQueue().addOperationWithBlock`
(the `NSOperation` feature of `objc2-foundation`, already enabled), and read any state the work depends on inside the
block rather than when queueing it. From a background thread, `run_on_main_thread` is still the way onto the main
thread.
