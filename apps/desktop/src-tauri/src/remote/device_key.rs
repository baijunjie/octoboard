//! This device's own SSH key for a remote daemon: after a first login with the remote user's own
//! password or key, the application makes an Ed25519 key pair for that entry, keeps the private key
//! in the system's credential store (`credentials`), appends the public key to the remote user's
//! `~/.ssh/authorized_keys` as `ssh-copy-id` does (on a Windows host, where its OpenSSH server
//! reads an administrator's keys from elsewhere, the file that user's login reads), marked with
//! this device's name, and connects with it from then on. The private key never leaves the device:
//! only the public key is sent, and the credential store is the only place it is written to.

use std::sync::Arc;

use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, PrivateKey};

use super::credentials::Store;
use super::octoboardd::{shell_word, Shell};
use crate::diagnostics::diagnostic;

/// The longest device name an installed key is marked with.
const MAX_NAME_CHARS: usize = 64;

/// A new key pair.
pub fn generate() -> Result<PrivateKey, String> {
    PrivateKey::random(&mut russh::keys::key::safe_rng(), Algorithm::Ed25519)
        .map_err(|err| err.to_string())
}

/// The device key kept for the entry `id`; `None` when none is, or it cannot be read.
pub fn load(store: &dyn Store, service: &str, id: &str) -> Option<Arc<PrivateKey>> {
    let bytes = match store.get(service, id) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return None,
        Err(err) => {
            diagnostic!("cannot read the device key of {id}: {err}");
            return None;
        }
    };
    let decoded = std::str::from_utf8(&bytes)
        .map_err(|err| err.to_string())
        .and_then(|text| PrivateKey::from_openssh(text).map_err(|err| err.to_string()));
    match decoded {
        Ok(key) => Some(Arc::new(key)),
        Err(err) => {
            diagnostic!("ignoring the unreadable device key of {id}: {err}");
            None
        }
    }
}

/// The device key kept for the entry `id`, made and kept now when there is none.
pub fn load_or_make(store: &dyn Store, service: &str, id: &str) -> Result<Arc<PrivateKey>, String> {
    if let Some(key) = load(store, service, id) {
        return Ok(key);
    }
    let key = generate()?;
    let encoded = key
        .to_openssh(LineEnding::LF)
        .map_err(|err| err.to_string())?;
    store.set(service, id, encoded.as_bytes())?;
    Ok(Arc::new(key))
}

/// Forgets the device key of the entry `id`.
pub fn forget(store: &dyn Store, service: &str, id: &str) {
    if let Err(err) = store.delete(service, id) {
        diagnostic!("cannot remove the device key of {id}: {err}");
    }
}

/// What the key is marked with in `authorized_keys`: `Octoboard on <device>`, on one line.
pub fn comment(device: &str) -> String {
    let name: String = device
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME_CHARS)
        .collect();
    match name.trim() {
        "" => "Octoboard".to_owned(),
        name => format!("Octoboard on {name}"),
    }
}

/// The public key's base64 body, which tells an installed copy apart whatever it is marked with.
pub fn public_body(key: &PrivateKey) -> Result<String, String> {
    let line = key
        .public_key()
        .to_openssh()
        .map_err(|err| err.to_string())?;
    line.split_whitespace()
        .nth(1)
        .map(str::to_owned)
        .ok_or_else(|| "the public key has no body".to_owned())
}

/// The `authorized_keys` line of `key`, marked with `device`.
pub fn authorized_line(key: &PrivateKey, device: &str) -> Result<String, String> {
    let body = public_body(key)?;
    Ok(format!(
        "{} {body} {}",
        key.algorithm().as_str(),
        comment(device)
    ))
}

/// The command that appends `line`, whose key body is `body`, to the remote user's authorized keys
/// as the host's `shell` runs it: `~/.ssh/authorized_keys` through `sh` on a POSIX host
/// (`posix_install_command`), the file of a Windows host through PowerShell
/// (`windows_install_command`).
pub fn install_command(shell: Shell, line: &str, body: &str) -> String {
    match shell {
        Shell::Posix => posix_install_command(line, body),
        Shell::Cmd | Shell::PowerShell => windows_install_command(line, body),
    }
}

/// The command that appends `line`, whose key body is `body`, to the remote user's
/// `~/.ssh/authorized_keys`, as `ssh-copy-id` does: run through `sh` whatever the login shell is;
/// `~/.ssh` and the file made private where they are made, and left as they are where they exist;
/// nothing appended when the key is already there; a last line with no line ending ended first, so
/// the entry above is not disturbed.
fn posix_install_command(line: &str, body: &str) -> String {
    let script = format!(
        "umask 077\n\
         cd || exit 1\n\
         mkdir -p .ssh || exit 1\n\
         f=.ssh/authorized_keys\n\
         if [ -f \"$f\" ] && grep -qF -- {body} \"$f\"; then exit 0; fi\n\
         if [ -s \"$f\" ] && [ -n \"$(tail -c 1 \"$f\")\" ]; then echo >> \"$f\" || exit 1; fi\n\
         printf '%s\\n' {line} >> \"$f\"\n",
        body = shell_word(body),
        line = shell_word(line),
    );
    format!("sh -c {}", shell_word(&script))
}

/// The PowerShell script `windows_install_command` runs, with `{LINE}` and `{BODY}` to be filled
/// in as PowerShell string literals.
const WINDOWS_INSTALL_SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
# A PowerShell run with its output redirected writes progress records to stderr as CLIXML.
$ProgressPreference = 'SilentlyContinue'
try {
$line = {LINE}
$body = {BODY}
$administrator = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($administrator) {
  $file = Join-Path $env:ProgramData 'ssh\administrators_authorized_keys'
} else {
  $file = Join-Path $HOME '.ssh\authorized_keys'
}
$directory = Split-Path -Parent $file
if (-not (Test-Path -LiteralPath $directory)) { New-Item -ItemType Directory -Path $directory | Out-Null }
if (-not (Test-Path -LiteralPath $file)) {
  New-Item -ItemType File -Path $file | Out-Null
  if ($administrator) {
    icacls $file /inheritance:r /grant '*S-1-5-32-544:F' /grant '*S-1-5-18:F' | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "icacls exited with status $LASTEXITCODE" }
  }
}
$text = [IO.File]::ReadAllText($file)
if ($text.Contains($body)) { exit 0 }
$prefix = ''
if ($text.Length -gt 0 -and -not $text.EndsWith("`n")) { $prefix = "`r`n" }
[IO.File]::AppendAllText($file, $prefix + $line + "`r`n", (New-Object Text.UTF8Encoding $false))
exit 0
} catch {
[Console]::Error.WriteLine($_.Exception.Message)
exit 1
}
"#;

/// `text` as a single-quoted PowerShell string literal.
fn powershell_literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// The command that appends `line`, whose key body is `body`, to the authorized keys of the remote
/// user of a Windows host: one PowerShell script, passed encoded so that neither the command prompt
/// nor PowerShell as the SSH server's shell reads any of it. An administrator's keys are read from
/// `%ProgramData%\ssh\administrators_authorized_keys` by the OpenSSH server's default
/// configuration, which accepts the file only where just Administrators and SYSTEM may use it, so a
/// file made here is given that; any other user's from `~\.ssh\authorized_keys`. An existing
/// file's access rights are left as they are, nothing is appended where the key is already there,
/// and a last line with no line ending is ended first. The file is written without a byte order
/// mark (PowerShell 5.1's own encodings would add one or use UTF-16).
fn windows_install_command(line: &str, body: &str) -> String {
    use base64::Engine as _;
    let script = WINDOWS_INSTALL_SCRIPT
        .replace("{LINE}", &powershell_literal(line))
        .replace("{BODY}", &powershell_literal(body));
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    format!(
        "powershell.exe -NoProfile -NonInteractive -EncodedCommand {}",
        base64::engine::general_purpose::STANDARD.encode(utf16)
    )
}

/// This device's name, as the owner knows it: the computer name on macOS and Windows, else the
/// host name.
pub fn device_name() -> String {
    platform::computer_name()
        .or_else(|| command_line("hostname", &[]))
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
mod macos {
    /// The computer name System Settings shows.
    pub fn computer_name() -> Option<String> {
        super::command_line("/usr/sbin/scutil", &["--get", "ComputerName"])
    }
}
#[cfg(target_os = "macos")]
use macos as platform;

/// Every target but macOS, Linux included, where the host name stands for the computer name.
#[cfg(not(target_os = "macos"))]
mod windows {
    /// The computer name Windows shows.
    #[cfg(windows)]
    pub fn computer_name() -> Option<String> {
        std::env::var("COMPUTERNAME")
            .ok()
            .filter(|name| !name.trim().is_empty())
    }

    #[cfg(not(windows))]
    pub fn computer_name() -> Option<String> {
        None
    }
}
#[cfg(not(target_os = "macos"))]
use windows as platform;

/// The first line `program` prints, when it succeeds and prints one.
fn command_line(program: &str, args: &[&str]) -> Option<String> {
    #[cfg(test)]
    let _spawning = crate::spawning();
    let output = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_owned())
}

#[cfg(test)]
mod tests {
    use super::super::credentials::Memory;
    use super::*;

    #[test]
    fn a_key_is_made_once_kept_and_read_back() {
        let store = Memory::default();
        assert!(load(&store, "svc", "a").is_none());
        let made = load_or_make(&store, "svc", "a").unwrap();
        assert_eq!(made.algorithm(), Algorithm::Ed25519);
        let again = load_or_make(&store, "svc", "a").unwrap();
        assert_eq!(made.public_key(), again.public_key());
        let other = load_or_make(&store, "svc", "b").unwrap();
        assert_ne!(made.public_key(), other.public_key());
        forget(&store, "svc", "a");
        assert!(load(&store, "svc", "a").is_none());
        assert_eq!(store.accounts(), vec!["b".to_owned()]);
    }

    #[test]
    fn an_unreadable_kept_key_counts_as_none_and_is_replaced() {
        let store = Memory::default();
        store.set("svc", "a", b"not a key").unwrap();
        assert!(load(&store, "svc", "a").is_none());
        let made = load_or_make(&store, "svc", "a").unwrap();
        assert_eq!(
            load(&store, "svc", "a").unwrap().public_key(),
            made.public_key()
        );
    }

    #[test]
    fn the_installed_line_is_marked_with_the_device_name_on_one_line() {
        assert_eq!(
            comment("  Ann's\tMacBook\nPro "),
            "Octoboard on Ann's MacBook Pro"
        );
        assert_eq!(comment(" \u{7} "), "Octoboard");
        assert_eq!(comment(&"x".repeat(100)).len(), "Octoboard on ".len() + 64);
        let key = generate().unwrap();
        let line = authorized_line(&key, "Desk").unwrap();
        let body = public_body(&key).unwrap();
        assert_eq!(line, format!("ssh-ed25519 {body} Octoboard on Desk"));
        assert!(!line.contains('\n'));
    }

    #[test]
    fn this_device_has_a_name() {
        assert!(!device_name().trim().is_empty());
    }

    /// Runs the install command as a login would, with `home` as the home directory.
    #[cfg(unix)]
    fn install(home: &std::path::Path, line: &str, body: &str) -> std::process::Output {
        let _spawning = crate::spawning();
        std::process::Command::new("sh")
            .arg("-c")
            .arg(install_command(Shell::Posix, line, body))
            .env("HOME", home)
            .output()
            .unwrap()
    }

    #[cfg(unix)]
    fn mode(path: &std::path::Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[cfg(unix)]
    #[test]
    fn installing_makes_a_private_file_where_there_is_none() {
        let scratch = crate::config_dir::test_support::Scratch::new("device-key", "fresh");
        std::fs::create_dir_all(&scratch.0).unwrap();
        let line = "ssh-ed25519 AAAAbody Octoboard on Ann's Mac";
        let output = install(&scratch.0, line, "AAAAbody");
        assert!(output.status.success(), "{output:?}");
        let file = scratch.0.join(".ssh/authorized_keys");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), format!("{line}\n"));
        assert_eq!(mode(&scratch.0.join(".ssh")), 0o700);
        assert_eq!(mode(&file), 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn installing_appends_once_leaving_the_other_entries_and_permissions_as_they_were() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = crate::config_dir::test_support::Scratch::new("device-key", "existing");
        let ssh = scratch.0.join(".ssh");
        std::fs::create_dir_all(&ssh).unwrap();
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o750)).unwrap();
        let file = ssh.join("authorized_keys");
        // The last entry has no line ending.
        std::fs::write(
            &file,
            "ssh-rsa AAAAother laptop\nssh-ed25519 AAAAlast phone",
        )
        .unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
        let line = "ssh-ed25519 AAAAmine Octoboard on Desk";
        assert!(install(&scratch.0, line, "AAAAmine").status.success());
        // Installed again, under another name, it is not added twice.
        assert!(install(
            &scratch.0,
            "ssh-ed25519 AAAAmine Octoboard on Other",
            "AAAAmine"
        )
        .status
        .success());
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            format!("ssh-rsa AAAAother laptop\nssh-ed25519 AAAAlast phone\n{line}\n")
        );
        assert_eq!(mode(&ssh), 0o750);
        assert_eq!(mode(&file), 0o640);
    }

    #[cfg(unix)]
    #[test]
    fn installing_fails_where_the_file_cannot_be_written() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = crate::config_dir::test_support::Scratch::new("device-key", "readonly");
        let ssh = scratch.0.join(".ssh");
        std::fs::create_dir_all(&ssh).unwrap();
        let file = ssh.join("authorized_keys");
        std::fs::write(&file, "ssh-rsa AAAAother laptop\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o400)).unwrap();
        let output = install(&scratch.0, "ssh-ed25519 AAAAmine x", "AAAAmine");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(!output.status.success(), "{output:?}");
    }

    /// The script a Windows install command carries, decoded.
    fn decoded_script(command: &str) -> String {
        use base64::Engine as _;
        let encoded = command
            .strip_prefix("powershell.exe -NoProfile -NonInteractive -EncodedCommand ")
            .expect("the command is an encoded PowerShell one");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let units: Vec<u16> = bytes
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16(&units).unwrap()
    }

    #[test]
    fn a_windows_host_gets_one_encoded_powershell_script_whichever_its_shell() {
        let line = "ssh-ed25519 AAAAbody Octoboard on Ann's PC";
        let cmd = install_command(Shell::Cmd, line, "AAAAbody");
        assert_eq!(cmd, install_command(Shell::PowerShell, line, "AAAAbody"));
        assert!(cmd.is_ascii() && !cmd.contains('"') && !cmd.contains('\''));
        let script = decoded_script(&cmd);
        assert!(script.contains("$line = 'ssh-ed25519 AAAAbody Octoboard on Ann''s PC'"));
        assert!(script.contains("$body = 'AAAAbody'"));
        assert!(script.contains("IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)"));
        assert!(script.contains(r"'ssh\administrators_authorized_keys'"));
        assert!(script.contains("$env:ProgramData"));
        assert!(script.contains(r"'.ssh\authorized_keys'"));
        assert!(script.contains("/inheritance:r /grant '*S-1-5-32-544:F' /grant '*S-1-5-18:F'"));
        assert!(script.contains("New-Object Text.UTF8Encoding $false"));
        assert!(!script.contains("{LINE}") && !script.contains("{BODY}"));
    }

    #[test]
    fn a_posix_host_keeps_its_sh_script() {
        let command = install_command(Shell::Posix, "ssh-ed25519 AAAAbody x", "AAAAbody");
        assert!(command.starts_with("sh -c '"));
        assert!(command.contains("grep -qF"));
    }

    /// A store whose every call fails, as a locked keychain's would.
    struct Failing;

    impl Store for Failing {
        fn get(&self, _: &str, _: &str) -> Result<Option<Vec<u8>>, String> {
            Err("locked".into())
        }
        fn set(&self, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
            Err("locked".into())
        }
        fn delete(&self, _: &str, _: &str) -> Result<(), String> {
            Err("locked".into())
        }
    }

    #[test]
    fn a_credential_store_that_cannot_be_used_leaves_no_key_and_fails_the_installing_step() {
        assert!(load(&Failing, "svc", "a").is_none());
        // The error reaches `install_device_key`, whose caller opens the window all the same.
        assert!(load_or_make(&Failing, "svc", "a").is_err());
        // Forgetting is best effort and never panics.
        forget(&Failing, "svc", "a");
    }

    #[test]
    fn the_key_is_kept_by_the_credential_store_and_only_its_public_half_is_sent() {
        let store = Memory::default();
        let key = load_or_make(&store, "svc", "a").unwrap();
        let kept = store.get("svc", "a").unwrap().expect("kept");
        assert!(String::from_utf8(kept).unwrap().contains("PRIVATE KEY"));
        let line = authorized_line(&key, "Desk").unwrap();
        let command = install_command(Shell::Posix, &line, &public_body(&key).unwrap());
        assert!(!command.contains("PRIVATE"), "{command}");
        assert_eq!(line.split_whitespace().next(), Some("ssh-ed25519"));
        let windows = install_command(Shell::Cmd, &line, &public_body(&key).unwrap());
        assert!(!decoded_script(&windows).contains("PRIVATE"));
    }

    #[test]
    fn each_remote_daemon_has_a_key_of_its_own_per_credential_store_service() {
        let store = Memory::default();
        let first = load_or_make(&store, "svc", "a").unwrap();
        let other_service = load_or_make(&store, "svc.dev", "a").unwrap();
        assert_ne!(first.public_key(), other_service.public_key());
    }

    #[cfg(unix)]
    #[test]
    fn a_name_with_quotes_and_shell_syntax_reaches_the_file_as_written() {
        let scratch = crate::config_dir::test_support::Scratch::new("device-key", "quoting");
        std::fs::create_dir_all(&scratch.0).unwrap();
        let line = "ssh-ed25519 AAAAbody Octoboard on $(touch pwned); `id` \"x\" 'y'";
        assert!(install(&scratch.0, line, "AAAAbody").status.success());
        let file = scratch.0.join(".ssh/authorized_keys");
        assert_eq!(std::fs::read_to_string(file).unwrap(), format!("{line}\n"));
        assert!(!scratch.0.join("pwned").exists());
    }
}
