//! The host's launchers: revealing a path in the file manager, opening it with its default
//! application and opening a terminal at a directory. What each starts is chosen here; whether a
//! path may be opened and the running of the command are `host_open`'s.

pub mod paths;
pub mod terminal;

/// Extensions of files the host's shell launches or acts on when opened, beyond the types refused
/// on every host: on Windows a shortcut, a ClickOnce or Store package or installer, a saved search
/// or library, a settings or Explorer command, a compiled help file, a script component, a setup
/// information file, a troubleshooting pack, and a disk image Explorer mounts. Matched without case
/// on a regular file's own name.
#[cfg(windows)]
pub const LAUNCHED_FILE_EXTENSIONS: &[&str] = &[
    "lnk",
    "appref-ms",
    "application",
    "settingcontent-ms",
    "library-ms",
    "search-ms",
    "scf",
    "appx",
    "msix",
    "appinstaller",
    "chm",
    "wsc",
    "sct",
    "inf",
    "diagcab",
    "iso",
    "img",
    "vhd",
    "vhdx",
];

/// Extensions of files the host's shell launches or acts on when opened, beyond the types refused
/// on every host: none here.
#[cfg(not(windows))]
pub const LAUNCHED_FILE_EXTENSIONS: &[&str] = &[];
