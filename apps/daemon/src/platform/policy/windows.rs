use std::path::PathBuf;

use super::Policy;
use crate::platform::registry::{read_value, Hive};

/// The directory an administrator keeps a program's managed files in: `name` in the Program Files
/// folder, as the system names it (`ProgramFilesDir`, where `FOLDERID_ProgramFiles` comes from),
/// else as `%ProgramFiles%` names it, else at its default place. `_etc_name` is the Linux name.
pub fn managed_dir(name: &str, _etc_name: &str) -> PathBuf {
    let system = read_value(
        Hive::LocalMachine,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion",
        "ProgramFilesDir",
    )
    .filter(|value| !value.expandable)
    .map(|value| value.text);
    let program_files = system
        .or_else(|| std::env::var("ProgramFiles").ok())
        .filter(|dir| !dir.is_empty())
        .unwrap_or_else(|| r"C:\Program Files".to_string());
    PathBuf::from(program_files).join(name)
}

/// The `Settings` value of `HKLM` and `HKCU` `SOFTWARE\Policies\<product>`, as stored: a
/// `REG_EXPAND_SZ` value is not expanded, as what it holds is a document, not a path.
pub fn registry_policy(product: &str) -> Policy {
    let key = format!(r"SOFTWARE\Policies\{product}");
    let read = |hive| read_value(hive, &key, "Settings").map(|value| value.text);
    Policy {
        machine: read(Hive::LocalMachine),
        user: read(Hive::CurrentUser),
    }
}
