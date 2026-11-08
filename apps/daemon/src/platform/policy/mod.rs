//! Policy an administrator sets for a program outside the user's files: the directory of managed
//! files the host keeps for it, a Unix program's system configuration directory under `/etc` where
//! the host has one, and on Windows the `Settings` value under `SOFTWARE\Policies\<product>` of
//! the machine's and of the user's registry. Other hosts have no such registry place, and report
//! none.

#[cfg(not(windows))]
use std::path::PathBuf;

/// The text of a product's policy value, where one is set, for the machine and for the user.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Policy {
    pub machine: Option<String>,
    pub user: Option<String>,
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{managed_dir, registry_policy, system_config_dir};

/// The directory an administrator keeps a program's managed files in: `name` under
/// `/Library/Application Support` on macOS; `etc_name` under `/etc` on Linux; `name` in the
/// Program Files folder on Windows.
#[cfg(target_os = "macos")]
pub fn managed_dir(name: &str, _etc_name: &str) -> PathBuf {
    PathBuf::from("/Library/Application Support").join(name)
}

/// The directory an administrator keeps a program's managed files in: `name` under
/// `/Library/Application Support` on macOS; `etc_name` under `/etc` on Linux; `name` in the
/// Program Files folder on Windows.
#[cfg(not(any(target_os = "macos", windows)))]
pub fn managed_dir(_name: &str, etc_name: &str) -> PathBuf {
    PathBuf::from("/etc").join(etc_name)
}

/// The system configuration directory of a Unix program: `name` under `/etc`. Windows has no such
/// directory, where a path from the root would name a folder on the current drive that no program
/// reads, so it reports none.
#[cfg(not(windows))]
pub fn system_config_dir(name: &str) -> Option<PathBuf> {
    Some(PathBuf::from("/etc").join(name))
}

/// The policy the registry holds for `product` (a key name under `SOFTWARE\Policies`).
#[cfg(not(windows))]
pub fn registry_policy(_product: &str) -> Policy {
    Policy::default()
}

#[cfg(all(test, not(windows)))]
mod unix_tests {
    use super::*;

    #[test]
    fn a_unix_programs_system_configuration_directory_is_under_etc() {
        assert_eq!(system_config_dir("grok"), Some(PathBuf::from("/etc/grok")));
    }
}

#[cfg(all(test, windows))]
mod tests {
    use std::process::Command;

    use super::*;

    #[test]
    fn windows_has_no_unix_system_configuration_directory() {
        assert_eq!(system_config_dir("grok"), None);
    }

    /// A `Settings` value in the user's registry is read as it is stored, and a product with none
    /// has no policy.
    #[test]
    fn the_registry_policy_is_the_settings_value_of_the_products_key() {
        let product = format!("OctoboardTest-{}", uuid::Uuid::new_v4());
        let key = format!(r"HKCU\SOFTWARE\Policies\{product}");
        let json = r#"{"allowedHttpHookUrls":["http://127.0.0.1:*"],"p":"%TEMP%"}"#;
        assert_eq!(registry_policy(&product), Policy::default());
        let added = Command::new("reg")
            .args([
                "add",
                &key,
                "/v",
                "Settings",
                "/t",
                "REG_EXPAND_SZ",
                "/d",
                json,
                "/f",
            ])
            .output()
            .unwrap();
        let read = registry_policy(&product);
        let _ = Command::new("reg").args(["delete", &key, "/f"]).output();
        assert!(added.status.success(), "{added:?}");
        assert_eq!(
            read,
            Policy {
                machine: None,
                user: Some(json.to_owned())
            }
        );
    }
}
