//! The rule that OS-specific tokens appear only in the platform layer. It has no exceptions.
//!
//! The tokens, as the scan (`scan::os_sites`) reads them in production code (comments and literal
//! contents are blanked first; test-only code is left out exactly as for the dependency rule):
//!
//! | Kind | What it is |
//! |---|---|
//! | `libc` | a path starting with `libc::` (a `use` or inline) |
//! | `windows_sys` | a path starting with `windows_sys::` |
//! | `portable_pty` | a path starting with `portable_pty::`, the Unix PTY backend's crate |
//! | `tokio::signal` | a path through `tokio::signal::unix` or `tokio::signal::windows` (the portable `tokio::signal::ctrl_c` is not one) |
//! | `std::os` | any path through `std::os`, including a `std::{…, os::…}` group |
//! | `cfg(os)` | a `cfg` predicate naming `target_os`, `unix` or `windows`, at any nesting, in `#[cfg]`, `#![cfg]`, the predicate of `#[cfg_attr]` and `cfg!` |
//! | `/bin/sh` | the text `/bin/sh` inside a string literal |
//! | `mode bits` | an octal literal (`0o…`), `set_mode(…)`, `from_mode(…)`, `.mode(arg)`, and `permissions().mode()` |
//! | `symlink` | the identifiers `symlink`, `symlink_file` and `symlink_dir` (not `symlink_metadata`, `is_symlink` or `read_link`, which are portable) |
//! | `canonicalize` | the identifier `canonicalize`: `fs::canonicalize` and `Path::canonicalize` are one OS call |
//!
//! A bare `metadata.mode()` with no arguments is not a `mode bits` site on its own, since
//! `alacritty_terminal`'s `Term::mode()` looks the same; the octal mask it is used with is one.
//!
//! Every site outside `src/platform/` is a problem, reported with its file, line and kind.

use super::scan::Scan;

fn in_platform(file: &str) -> bool {
    file == "platform.rs" || file.starts_with("platform/")
}

pub(super) fn check(scan: &Scan, problems: &mut Vec<String>) {
    for (file, site) in &scan.os_sites {
        if !in_platform(file) {
            problems.push(format!(
                "apps/daemon/src/{file}:{}: `{}` outside the platform layer",
                site.line, site.kind
            ));
        }
    }
}
