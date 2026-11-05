//! Policy an administrator sets for a program outside the user's files: the directory of managed
//! files the host keeps for it, and on Windows the `Settings` value under
//! `SOFTWARE\Policies\<product>` of the machine's and of the user's registry. Other hosts have no
//! such registry place, and report none.

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
pub use windows::{managed_dir, registry_policy};

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

/// The policy the registry holds for `product` (a key name under `SOFTWARE\Policies`).
#[cfg(not(windows))]
pub fn registry_policy(_product: &str) -> Policy {
    Policy::default()
}

#[cfg(all(test, windows))]
mod tests {
    use std::process::Command;

    use super::*;

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
