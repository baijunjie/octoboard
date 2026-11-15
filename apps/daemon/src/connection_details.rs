//! The connection details of a daemon: what another device needs to reach it over SSH — the SSH
//! host, port and user, the fingerprint of the SSH server's host key and the path of this
//! `octoboardd` — as one line of text and as a QR code of that same line. They carry no secret: a
//! device still logs in over SSH, and reads the owner key through `octoboardd address --json` once
//! it has. `octoboardd connection` prints them on a server; the desktop application's daemon gives
//! them to its Remote access section while remote access is on.
//!
//! The text is a URI, `octoboard+ssh://<user>@<host>:<port>?fingerprint=…&octoboardd=…&name=…`,
//! which the desktop application reads back when it is pasted into a remote daemon's form; a mobile
//! client is to scan the QR code (see "Printing the connection details" in
//! `docs/product/daemon-command-line.md`).

use std::ffi::OsString;
use std::path::Path;

use base64::Engine;
use sha2::{Digest, Sha256};

use crate::platform::fs;

/// The scheme of the connection details' text.
pub const SCHEME: &str = "octoboard+ssh";

/// The SSH port a host is reached at when nothing says otherwise.
pub const DEFAULT_SSH_PORT: u16 = 22;

/// The host key files tried, in the order the application's SSH client prefers their algorithms,
/// so that the fingerprint printed is the one the host shows it.
const HOST_KEY_FILES: [&str; 3] = [
    "ssh_host_ed25519_key.pub",
    "ssh_host_ecdsa_key.pub",
    "ssh_host_rsa_key.pub",
];

/// What another device needs to connect to this machine's daemon over SSH.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Details {
    pub user: String,
    pub host: String,
    pub port: u16,
    /// The SSH server's host key fingerprint, `SHA256:<base64>` as OpenSSH prints it; `None` when
    /// no host key could be read, for the application to show at the first connection instead.
    pub fingerprint: Option<String>,
    pub octoboardd: Option<String>,
    /// The name the machine gives itself, for the entry's name; `None` when it is the host.
    pub name: Option<String>,
}

impl Details {
    /// The details as their one line of text, which the QR code carries unchanged.
    pub fn to_text(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host.replace('%', "%25"))
        } else {
            encoded(&self.host, "")
        };
        let mut text = format!(
            "{SCHEME}://{}@{host}:{}",
            encoded(&self.user, ""),
            self.port
        );
        let fields = [
            ("fingerprint", &self.fingerprint),
            ("octoboardd", &self.octoboardd),
            ("name", &self.name),
        ];
        let mut separator = '?';
        for (field, value) in fields {
            if let Some(value) = value {
                text.push(separator);
                text.push_str(field);
                text.push('=');
                text.push_str(&encoded(value, ":/@"));
                separator = '&';
            }
        }
        text
    }
}

/// `text` percent-encoded as UTF-8, leaving as they are the characters a URI never reserves and
/// those in `keep`.
fn encoded(text: &str, keep: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        let c = byte as char;
        if c.is_ascii_alphanumeric() || "-._~".contains(c) || (byte.is_ascii() && keep.contains(c))
        {
            out.push(c);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The fingerprint of the host key in a public key file's `line` (`<type> <base64> [comment]`),
/// `SHA256:` followed by the unpadded base64 of the SHA-256 of the key, as OpenSSH prints it.
pub fn fingerprint_of_line(line: &str) -> Option<String> {
    let blob = line.split_whitespace().nth(1)?;
    let key = base64::engine::general_purpose::STANDARD
        .decode(blob)
        .ok()?;
    let digest = Sha256::digest(&key);
    Some(format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest)
    ))
}

/// The fingerprint of the SSH server's host key in `dir`, of the first key file there that reads,
/// in the order of [`HOST_KEY_FILES`]; `None` when none does.
pub fn host_key_fingerprint(dir: &Path) -> Option<String> {
    HOST_KEY_FILES.iter().find_map(|file| {
        let text = std::fs::read_to_string(dir.join(file)).ok()?;
        text.lines().find_map(fingerprint_of_line)
    })
}

/// The QR code of `text`: its rows of modules from the top, each module from the left, `true` for
/// a dark one, with no quiet zone around it.
pub fn qr_modules(text: &str) -> anyhow::Result<Vec<Vec<bool>>> {
    let code = qrcode::QrCode::new(text.as_bytes())
        .map_err(|err| anyhow::anyhow!("making the QR code: {err}"))?;
    let width = code.width();
    let colors = code.to_colors();
    Ok(colors
        .chunks(width)
        .map(|row| row.iter().map(|c| *c == qrcode::Color::Dark).collect())
        .collect())
}

/// `value` as text, where it is valid Unicode and not empty.
pub fn text_of(value: OsString) -> Option<String> {
    value.into_string().ok().filter(|value| !value.is_empty())
}

/// The details of this machine as the desktop application's daemon gives them: the machine's own
/// name as the host (which other devices may know by another name; the owner corrects it there) at
/// the default SSH port, as [`of_this_machine_at`] gives them otherwise. `None` when the user's or
/// the machine's name cannot be read.
pub fn of_this_machine(octoboardd: Option<&Path>) -> Option<Details> {
    let host = fs::host_name().and_then(text_of)?;
    of_this_machine_at(host, DEFAULT_SSH_PORT, None, octoboardd)
}

/// The details of this machine reached at `host` and `port`: the user this process runs as, the
/// host key of the computer's SSH server where it can be read (in [`fs::ssh_host_key_dir`]),
/// `name` for the entry's name, and `octoboardd`, the path of this daemon's executable. `None`
/// when the user's name cannot be read.
pub fn of_this_machine_at(
    host: String,
    port: u16,
    name: Option<String>,
    octoboardd: Option<&Path>,
) -> Option<Details> {
    Some(Details {
        user: fs::account_name().and_then(text_of)?,
        host,
        port,
        fingerprint: host_key_fingerprint(&fs::ssh_host_key_dir()),
        octoboardd: octoboardd.and_then(|path| text_of(path.as_os_str().to_owned())),
        name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn details() -> Details {
        Details {
            user: "me".into(),
            host: "server.example".into(),
            port: 22,
            fingerprint: Some("SHA256:ab+c/d".into()),
            octoboardd: Some("/home/me/.local/bin/octoboardd".into()),
            name: Some("box".into()),
        }
    }

    #[test]
    fn the_text_is_a_uri_with_every_field_encoded_where_it_must_be() {
        assert_eq!(
            details().to_text(),
            "octoboard+ssh://me@server.example:22?fingerprint=SHA256:ab%2Bc/d\
             &octoboardd=/home/me/.local/bin/octoboardd&name=box"
        );
    }

    #[test]
    fn missing_fields_are_left_out_and_an_ipv6_host_is_bracketed() {
        let details = Details {
            user: "a b".into(),
            host: "fe80::1%eth0".into(),
            port: 2222,
            fingerprint: None,
            octoboardd: None,
            name: Some("Büro & Co".into()),
        };
        assert_eq!(
            details.to_text(),
            "octoboard+ssh://a%20b@[fe80::1%25eth0]:2222?name=B%C3%BCro%20%26%20Co"
        );
        let bare = Details {
            name: None,
            ..details
        };
        assert_eq!(
            bare.to_text(),
            "octoboard+ssh://a%20b@[fe80::1%25eth0]:2222"
        );
    }

    #[test]
    fn a_public_key_line_gives_the_fingerprint_openssh_prints() {
        // What `ssh-keygen -lf` prints for this key.
        let line =
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPqkQ9TrjG2BMo0d/QN9beunLYkUQM/s4+zTxLgT9dp6 \
                    root@host";
        assert_eq!(
            fingerprint_of_line(line).as_deref(),
            Some("SHA256:PGa3TsO8+Aag9NDGPxVKhovqDTyCh3OFkg2O1lNjCbA")
        );
        assert_eq!(fingerprint_of_line("ssh-ed25519 not-base64!"), None);
        assert_eq!(fingerprint_of_line(""), None);
    }

    #[test]
    fn the_host_key_is_taken_in_the_order_the_application_prefers() {
        let dir = crate::test_support::ScratchDir::new("connection-host-keys");
        assert_eq!(host_key_fingerprint(&dir), None);
        std::fs::write(dir.join("ssh_host_rsa_key.pub"), "ssh-rsa AAAA root@h\n").unwrap();
        let rsa = host_key_fingerprint(&dir).unwrap();
        std::fs::write(
            dir.join("ssh_host_ed25519_key.pub"),
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 root@h\n",
        )
        .unwrap();
        let ed25519 = host_key_fingerprint(&dir).unwrap();
        assert_ne!(rsa, ed25519);
        assert_eq!(
            Some(ed25519),
            fingerprint_of_line("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5")
        );
    }

    #[test]
    fn the_qr_modules_are_the_codes_and_square() {
        let text = details().to_text();
        let modules = qr_modules(&text).unwrap();
        let code = qrcode::QrCode::new(text.as_bytes()).unwrap();
        assert_eq!(modules.len(), code.width());
        assert!(modules.iter().all(|row| row.len() == code.width()));
        let colors = code.to_colors();
        for (y, row) in modules.iter().enumerate() {
            for (x, dark) in row.iter().enumerate() {
                assert_eq!(*dark, colors[y * code.width() + x] == qrcode::Color::Dark);
            }
        }
    }

    #[test]
    fn this_machines_details_name_the_executable_and_the_default_port() {
        let Some(details) = of_this_machine(Some(Path::new("/opt/octoboard/octoboardd"))) else {
            return;
        };
        assert_eq!(details.port, DEFAULT_SSH_PORT);
        assert_eq!(
            details.octoboardd.as_deref(),
            Some("/opt/octoboard/octoboardd")
        );
        assert_eq!(details.name, None);
        assert!(details.to_text().starts_with("octoboard+ssh://"));
    }

    /// A name or user that holds what a URI reserves is encoded where it stands: `@` in the user,
    /// `=`, `&`, `#`, `?` and a space in a value, a Windows path whole, so that a field cannot end
    /// early or start another; `:`, `/` and `@` stay readable in a value.
    #[test]
    fn what_a_uri_reserves_never_ends_a_field_early() {
        let details = Details {
            user: "me@corp".into(),
            host: "host.example".into(),
            port: 22,
            fingerprint: None,
            octoboardd: Some(r"C:\Program Files\Octoboard\octoboardd.exe".into()),
            name: Some("a=b&c#d?e f".into()),
        };
        assert_eq!(
            details.to_text(),
            "octoboard+ssh://me%40corp@host.example:22\
             ?octoboardd=C:%5CProgram%20Files%5COctoboard%5Coctoboardd.exe\
             &name=a%3Db%26c%23d%3Fe%20f"
        );
    }

    /// An IPv6 host with no zone is bracketed as it is, and a host with a character outside ASCII
    /// is encoded as UTF-8; no field is left that a reader could mistake for the port.
    #[test]
    fn an_ipv6_host_is_bracketed_and_a_unicode_host_encoded() {
        let ipv6 = Details {
            host: "2001:db8::1".into(),
            ..details()
        };
        assert!(
            ipv6.to_text()
                .starts_with("octoboard+ssh://me@[2001:db8::1]:22?"),
            "{}",
            ipv6.to_text()
        );
        let unicode = Details {
            host: "büro.example".into(),
            ..details()
        };
        assert!(
            unicode
                .to_text()
                .starts_with("octoboard+ssh://me@b%C3%BCro.example:22?"),
            "{}",
            unicode.to_text()
        );
    }

    /// The text of the details carries no secret: nothing of an owner key or a launch key is in
    /// any field, whatever the fields hold, which are the host, the user, the port, the
    /// fingerprint, the path and the name alone.
    #[test]
    fn the_text_has_only_the_fields_of_the_details() {
        let text = details().to_text();
        let (_, query) = text.split_once('?').expect("fields");
        let names: Vec<&str> = query
            .split('&')
            .map(|field| field.split_once('=').expect("a field").0)
            .collect();
        assert_eq!(names, ["fingerprint", "octoboardd", "name"]);
    }

    /// A line that holds a key of any type gives the fingerprint of that key, a comment line the
    /// host key file may start with is passed over, and a file with no key line gives none.
    #[test]
    fn a_host_key_file_is_read_for_the_first_line_that_holds_a_key() {
        let dir = crate::test_support::ScratchDir::new("connection-host-key-lines");
        std::fs::write(
            dir.join("ssh_host_ecdsa_key.pub"),
            "# written by the installer\necdsa-sha2-nistp256 AAAAE2VjZHNh host\n",
        )
        .unwrap();
        assert_eq!(
            host_key_fingerprint(&dir),
            fingerprint_of_line("ecdsa-sha2-nistp256 AAAAE2VjZHNh")
        );
        assert!(host_key_fingerprint(&dir).is_some());
        let empty = crate::test_support::ScratchDir::new("connection-host-key-empty");
        std::fs::write(empty.join("ssh_host_ed25519_key.pub"), "\n\n").unwrap();
        assert_eq!(host_key_fingerprint(&empty), None);
    }

    /// Text beyond what a QR code holds has no code rather than a wrong one.
    #[test]
    fn text_too_long_for_a_qr_code_has_none() {
        assert!(qr_modules(&"x".repeat(4000)).is_err());
        assert!(qr_modules(&details().to_text()).is_ok());
    }

    /// Only text that is valid Unicode and not empty is a field; an empty value is left out.
    #[test]
    fn an_empty_or_non_unicode_value_is_no_field() {
        assert_eq!(text_of(OsString::from("x")).as_deref(), Some("x"));
        assert_eq!(text_of(OsString::new()), None);
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            assert_eq!(text_of(OsString::from_vec(vec![0xff, 0xfe])), None);
        }
        let none = of_this_machine(Some(Path::new("")));
        if let Some(details) = none {
            assert_eq!(details.octoboardd, None, "an empty path is left out");
        }
    }
}
