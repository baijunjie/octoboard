# Writing desktop shell code

The shell's rules (native callbacks and threads, states readable by a command, native hooks, `msg_send!` answers, native
menus) are in `docs/conventions/desktop-shell.md`, and `crate::spawning()` in its tests in `docs/conventions/tests.md`.
What they leave to verification:

## A move of `tauri`, `tray-icon` or `muda` in `Cargo.lock`: check every native hook in the running app

A hand-written hook that reads AppKit objects back through those crates' internals gives up quietly after an update and
still compiles. On any move of one of them, including one pulled in only transitively by raising Tauri (the 2.11 to
2.12 raise took tray-icon from 0.24 to 0.25, and every finishing touch on the tray menu vanished with no error), find the
hooks (search for `with_inner_tray_icon`, `ns_status_item`, `ns_window`), compare what each relies on between the old
and new crate sources, then confirm each in the running app.

## A state lost between an event and its page shows only when it changes while a page loads

The event-plus-command shape guards against a page that is loading when the shell emits, which spaced-out manual checks
do not hit. To check it, make the state change while the window is loading, such as while the shell points the window at
a restarted daemon (`point_window_at` in `apps/desktop/src-tauri/src/recovery.rs`).
