//! Pi resolves relative packages against the active configuration directory, even when its
//! settings file is a link. Auth and credential-bearing settings keep the account's own file.

use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::accounts::shared_config::{
    config_dir, config_file, history_dir, FileRegistry, SharedItem,
};
use crate::adapter::files::LARGE_FILE_LIMIT;
use crate::hostfs::json_paths::Select;
use crate::providers::pi::LAYOUT;

pub(super) const SETTINGS: &str = "settings.json";

pub(super) const ITEMS: &[SharedItem] = &[
    config_file(SETTINGS),
    config_dir("extensions"),
    history_dir(LAYOUT.records_dir),
];

pub(super) const REGISTRIES: &[FileRegistry] = &[FileRegistry {
    item: SETTINGS,
    select: Select::AnyOf(&[&["packages", "*"], &["packages", "*", "source"]]),
    source_path: Some(package_path),
}];

fn package_path(path: &Path) -> io::Result<Option<PathBuf>> {
    let source = path.to_string_lossy();
    let trimmed = source.trim();
    if [
        "npm:", "git:", "github:", "http:", "https:", "ssh:", "builtin:",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(*prefix))
    {
        return Ok(None);
    }
    if let Some(url) = source.strip_prefix("file://") {
        return file_url_path(url).map(Some);
    }
    Ok(Some(crate::hostfs::expand(&source)))
}

/// Pi uses Node's file URL paths for local packages, including percent-encoded filenames.
/// Reject an ambiguous URL instead of linking settings with a source the account cannot use.
fn file_url_path(url: &str) -> io::Result<PathBuf> {
    let invalid = || io::Error::other("A Pi package file URL could not be resolved.");
    let url = url.split(['?', '#']).next().unwrap_or_default();
    let (host, path) = url.split_once('/').ok_or_else(invalid)?;
    let local = host.is_empty() || host.eq_ignore_ascii_case("localhost");
    let mut decoded = Vec::with_capacity(path.len());
    let mut input = path.bytes();
    let windows = crate::platform::HostOs::current() == crate::platform::HostOs::Windows;
    while let Some(byte) = input.next() {
        let byte = if byte == b'%' {
            let mut hex = || input.next().and_then(|byte| char::from(byte).to_digit(16));
            let byte = (hex().ok_or_else(invalid)? * 16 + hex().ok_or_else(invalid)?) as u8;
            if byte == b'/' || windows && byte == b'\\' {
                return Err(invalid());
            }
            byte
        } else {
            byte
        };
        if byte == 0 {
            return Err(invalid());
        }
        decoded.push(byte);
    }
    let path = String::from_utf8(decoded).map_err(|_| invalid())?;
    let path = match (windows, local) {
        (false, true) => Path::new("/").join(path),
        (true, true) => PathBuf::from(path.replace('/', "\\")),
        (true, false)
            if host
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ".-".contains(ch)) =>
        {
            PathBuf::from(format!(r"\\{host}\{}", path.replace('/', "\\")))
        }
        _ => return Err(invalid()),
    };
    path.is_absolute().then_some(path).ok_or_else(invalid)
}

pub(super) fn check_settings(account: &Path, default: &Path) -> io::Result<()> {
    if !check_file(default)? {
        return Ok(());
    }
    check_file(account).map(|_| ())
}

/// Reads only enough to decide eligibility. The diagnostic never includes any field value.
fn check_file(file: &Path) -> io::Result<bool> {
    let text = match crate::hostfs::read_regular_text(file, LARGE_FILE_LIMIT) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    let settings: Value = serde_json::from_str(&text).map_err(|_| {
        io::Error::other("Pi settings are not readable JSON and were left private.")
    })?;
    if contains_credential(&settings) {
        return Err(io::Error::other(
            "Pi settings contain an inline credential and were left private.",
        ));
    }
    Ok(true)
}

fn contains_credential(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.iter().any(|(key, value)| {
            let key = key.to_ascii_lowercase().replace(['_', '-'], "");
            let secret = matches!(
                key.as_str(),
                "apikey"
                    | "token"
                    | "accesstoken"
                    | "refreshtoken"
                    | "password"
                    | "authorization"
                    | "credentials"
                    | "secret"
                    | "clientsecret"
                    | "authtoken"
                    | "proxyauthorization"
                    | "privatekey"
                    | "bearertoken"
            );
            (secret && !value.is_null() && value.as_str() != Some("")) || contains_credential(value)
        }),
        Value::Array(entries) => entries.iter().any(contains_credential),
        Value::String(text) => inline_url_credential(text),
        _ => false,
    }
}

fn inline_url_credential(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if let Some((userinfo, _)) = authority.rsplit_once('@') {
        let protocol = scheme.rsplit(':').next().unwrap_or_default();
        if userinfo.contains(':')
            || protocol.eq_ignore_ascii_case("http")
            || protocol.eq_ignore_ascii_case("https")
        {
            return true;
        }
    }
    rest.split_once('?').is_some_and(|(_, query)| {
        query.split(['&', '#']).any(|part| {
            part.split_once('=')
                .is_some_and(|(key, _)| matches!(key, "token" | "access_token" | "api_key"))
        })
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::coordinator::account_sharing::set;
    use crate::protocol::{Account, AgentAvailability, AgentId, Availability};
    use crate::test_support::app_state;

    fn write(root: &Path, name: &str, text: &str) {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[tokio::test]
    async fn shares_configuration_and_history_but_keeps_account_files_private() {
        if !crate::platform::fs::file_links_available() {
            return;
        }
        let (state, dir) = app_state("pi-sharing");
        let (default, account) = (dir.join("default"), dir.join("account"));
        let agent = AgentId::PI;
        state.set_agent_availability(vec![AgentAvailability {
            agent,
            availability: Availability::Available,
            default_account_dir: Some(default.to_string_lossy().into_owned()),
        }]);
        state
            .store
            .insert_account(&Account {
                id: "account".to_owned(),
                agent,
                name: "Account".to_owned(),
                config_dir: account.to_string_lossy().into_owned(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
                versions: Default::default(),
            })
            .unwrap();

        let shared = ["extensions/example.js"];
        let private = ["models.json", "mcp.json", "auth.json", "mcp-auth.json"];
        for name in shared.iter().chain(&private) {
            write(&default, name, "default");
            write(&account, name, "account");
        }
        write(&account, SETTINGS, r#"{"theme":"dark"}"#);
        write(
            &default,
            SETTINGS,
            r#"{"packages":[{"source":"./extensions/example.js"}]}"#,
        );
        let old_record = format!("{}/existing/record.jsonl", LAYOUT.records_dir);
        write(&account, &old_record, "earlier history");

        set(&state, Some("account".to_owned()), true).await.unwrap();

        for name in shared {
            assert_eq!(
                std::fs::read_to_string(account.join(name)).unwrap(),
                "default",
                "{name}"
            );
        }
        for name in private {
            assert_eq!(
                std::fs::read_to_string(account.join(name)).unwrap(),
                "account",
                "{name}"
            );
        }
        let settings: Value =
            serde_json::from_str(&std::fs::read_to_string(account.join(SETTINGS)).unwrap())
                .unwrap();
        assert_eq!(
            settings["packages"][0]["source"],
            serde_json::json!(crate::hostfs::canonical(default.join(shared[0])).unwrap())
        );
        let new_record = format!("{}/new/record.jsonl", LAYOUT.records_dir);
        write(&account, &new_record, "later history");
        assert_eq!(
            std::fs::read_to_string(default.join(&old_record)).unwrap(),
            "earlier history"
        );
        assert_eq!(
            std::fs::read_to_string(default.join(&new_record)).unwrap(),
            "later history"
        );

        set(&state, Some("account".to_owned()), false)
            .await
            .unwrap();

        for name in shared {
            assert_eq!(
                std::fs::read_to_string(account.join(name)).unwrap(),
                "account",
                "{name}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(account.join(SETTINGS)).unwrap(),
            r#"{"theme":"dark"}"#
        );
        for name in [old_record, new_record] {
            assert!(default.join(&name).exists(), "{name}");
            assert!(!account.join(&name).exists(), "{name}");
        }
    }

    #[tokio::test]
    async fn sharing_keeps_pi_settings_with_inline_credentials_private() {
        if !crate::platform::fs::file_links_available() {
            return;
        }
        let (state, dir) = app_state("pi-private-settings");
        let (default, account) = (dir.join("default"), dir.join("account"));
        let private = r#"{"httpProxy":"https://scratch-user:scratch-sentinel@localhost:9999"}"#;
        write(&account, SETTINGS, private);
        write(&default, SETTINGS, r#"{"theme":"dark"}"#);
        state.set_agent_availability(vec![AgentAvailability {
            agent: AgentId::PI,
            availability: Availability::Available,
            default_account_dir: Some(default.to_string_lossy().into_owned()),
        }]);
        state
            .store
            .insert_account(&Account {
                id: "account".to_owned(),
                agent: AgentId::PI,
                name: "Account".to_owned(),
                config_dir: account.to_string_lossy().into_owned(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
                versions: Default::default(),
            })
            .unwrap();

        let err = set(&state, Some("account".to_owned()), true)
            .await
            .unwrap_err();
        assert_eq!(
            crate::test_support::code_of(&err),
            crate::protocol::error_code::ACCOUNT_SHARING_FAILED
        );
        let states = state.account_sharing_states().unwrap();
        let failure = states[0].failure.as_ref().unwrap();
        assert_eq!(failure.items[0].item, SETTINGS);
        let detail = &failure.items[0].detail;
        assert!(detail.contains("inline credential"));
        assert!(!detail.contains("scratch-sentinel"));
        assert_eq!(
            std::fs::read_to_string(account.join(SETTINGS)).unwrap(),
            private
        );
        assert_eq!(
            std::fs::read_to_string(default.join(SETTINGS)).unwrap(),
            r#"{"theme":"dark"}"#
        );
    }
}
