//! The one place the application writes to a remote host's files: the remote user's authorized
//! keys, to which a device key's line (`device_key`) is added once the owner has agreed to it in
//! the window, and from which it is taken out again when the key is given up (`key_removal`).
//! `~/.ssh/authorized_keys` on a POSIX host, changed through `sh`; on a Windows host, where its
//! OpenSSH server reads an administrator's keys from elsewhere, the file that user's login reads,
//! changed through PowerShell.
//!
//! Every line but the device key's own is another program's or the owner's: both changes read and
//! write the file as bytes, whatever encoding and line endings its lines have, and leave every
//! byte of every other line as it was.

use super::octoboardd::{shell_word, Shell};
use super::ssh::Session;

/// A change to the remote user's authorized keys.
#[derive(Clone, Copy, Debug)]
pub enum KeyChange<'a> {
    /// Appends `line`, whose key body is `body`, unless the key is already there.
    Add { line: &'a str, body: &'a str },
    /// Takes out the line that carries the key body `body`, leaving every other line as it is.
    Remove { body: &'a str },
}

/// Why the remote user's authorized keys were not changed.
#[derive(Debug, PartialEq, Eq)]
pub enum ChangeError {
    /// The file holds no line carrying the key to take out: there is no such file, the line was
    /// removed or moved there, or the host reads its keys from another file.
    NotFound,
    /// The host could not run the change, or the change failed there: what it said.
    Failed(String),
    /// The file was being rewritten when the write failed, and putting its content back failed
    /// too: it may hold only part of its lines, or none. `copy` is where the host keeps the
    /// content it had before, for the owner to restore it from; `detail` what the host said.
    Incomplete { copy: String, detail: String },
}

impl std::fmt::Display for ChangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str(
                "the file holds no line carrying this key: it was removed or moved there, or the \
                 host reads its keys from another file",
            ),
            Self::Failed(detail) => f.write_str(detail),
            Self::Incomplete { copy, detail } => write!(
                f,
                "the file may be incomplete; its earlier content is kept on the host in {copy}: {detail}"
            ),
        }
    }
}

/// What a change's command exits with where the key to take out is not in the file.
const NOT_FOUND_STATUS: u32 = 3;

/// What a removal ends with where it left the file possibly incomplete: the write failed, and so
/// did putting the earlier content back.
const INCOMPLETE_STATUS: u32 = 4;

/// What a removal writes before it ends with `INCOMPLETE_STATUS`, followed by where the host keeps
/// the file's earlier content. Like `NOT_FOUND_ANSWER`, it arrives where the status does not.
const INCOMPLETE_ANSWER: &str = "file-incomplete ";

/// What a Windows script writes, and nothing else, before it ends with `NOT_FOUND_STATUS`: the
/// status is lost where a second PowerShell runs the script's (`change_outcome`).
const NOT_FOUND_ANSWER: &str = "no-such-line";

/// Changes the remote user's authorized keys over `session`, on a host whose commands `shell`
/// runs. Every write the application makes to a remote host's files goes through here: the two
/// changes are the device key's line added and taken out again, each only as the owner agreed to
/// in the window.
pub async fn change_authorized_keys(
    session: &Session,
    shell: Shell,
    change: KeyChange<'_>,
) -> Result<(), ChangeError> {
    run_change(session, &change_command(shell, change), change).await
}

/// Runs `command`, which makes `change`, over `session`, and reads what it ended with.
async fn run_change(
    session: &Session,
    command: &str,
    change: KeyChange<'_>,
) -> Result<(), ChangeError> {
    let output = session
        .run(command)
        .await
        .map_err(|err| ChangeError::Failed(err.to_string()))?;
    change_outcome(change, output.status, &output.stdout, &output.stderr)
}

/// What a command that makes `change` came to, from the status it ended with and what it wrote.
///
/// A removal that found no line to take out says so in two ways, since the status alone does not
/// always arrive: where PowerShell is the SSH server's shell, the script's own PowerShell is run
/// by another, which ends with 1 whatever status other than 0 the script ended with. So a Windows
/// script also writes `NOT_FOUND_ANSWER` before it ends with `NOT_FOUND_STATUS`, and either one
/// says the line was not there. Telling the two apart decides what the owner is told: that the
/// line may still be on the host, or that its removal failed for a reason.
fn change_outcome(
    change: KeyChange<'_>,
    status: Option<u32>,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<(), ChangeError> {
    let failed = || {
        Err(ChangeError::Failed(format!(
            "changing the host's authorized keys ended with {status:?}: {}",
            host_said(stderr)
        )))
    };
    if status == Some(0) {
        return Ok(());
    }
    if !matches!(change, KeyChange::Remove { .. }) || status.is_none() {
        return failed();
    }
    if status == Some(NOT_FOUND_STATUS) {
        return Err(ChangeError::NotFound);
    }
    let answered = String::from_utf8_lossy(stdout);
    let answered = answered.trim();
    if answered == NOT_FOUND_ANSWER {
        return Err(ChangeError::NotFound);
    }
    // A file left possibly incomplete is told by its words alone, on every host: they carry where
    // the earlier content is kept, without which there is nothing to tell the owner.
    match answered.strip_prefix(INCOMPLETE_ANSWER) {
        Some(copy) => Err(ChangeError::Incomplete {
            copy: host_said(copy.as_bytes()),
            detail: host_said(stderr),
        }),
        None => failed(),
    }
}

/// The file `change_authorized_keys` changes for the user `session` signed in as, as the owner is
/// told of it: `~/.ssh/authorized_keys` on a POSIX host, and on a Windows host the path of the
/// file that user's login reads, which the host is asked for (nothing is written by asking).
pub async fn authorized_keys_file(session: &Session, shell: Shell) -> Result<String, String> {
    if shell == Shell::Posix {
        return Ok(POSIX_FILE.to_owned());
    }
    let output = session
        .run(&windows_command(WINDOWS_FILE_SCRIPT))
        .await
        .map_err(|err| err.to_string())?;
    let file = host_said(&output.stdout);
    if output.status != Some(0) || file.is_empty() {
        return Err(format!(
            "asking the host for its authorized keys file ended with {:?}: {}",
            output.status,
            host_said(&output.stderr)
        ));
    }
    Ok(file)
}

/// What a script wrote where it marks text as carried (`CARRIED`): the text itself, decoded.
/// Anything else, a POSIX host's own words included, is taken as it was written.
fn host_said(written: &[u8]) -> String {
    use base64::Engine as _;
    let text = String::from_utf8_lossy(written);
    let text = text.trim();
    text.strip_prefix(CARRIED)
        .and_then(|encoded| {
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .ok()
        })
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_else(|| text.to_owned())
}

/// What the PowerShell scripts put before text they write as base64 of its UTF-8 bytes. A Windows
/// console writes in the system's code page, which loses a path or a message in another script;
/// base64 is the same in every code page.
const CARRIED: &str = "utf8:";

/// The authorized keys of a user of a POSIX host, as the owner is told of the file.
pub const POSIX_FILE: &str = "~/.ssh/authorized_keys";

/// The command that makes `change` as the host's `shell` runs it: through `sh` on a POSIX host,
/// through PowerShell on a Windows one.
fn change_command(shell: Shell, change: KeyChange<'_>) -> String {
    change_command_at(shell, change, None)
}

/// `change_command`, with the file a Windows host's script works on replaced by `file` where one
/// is given: the script is the same but for one line, which names that file in place of the one
/// the remote user's login reads. The application always gives none; the test against a live
/// Windows host gives a scratch file, so that it never writes the host's real one.
fn change_command_at(shell: Shell, change: KeyChange<'_>, file: Option<&str>) -> String {
    let elsewhere = file
        .map(|file| format!("$file = {}\n", powershell_value(file)))
        .unwrap_or_default();
    match (shell, change) {
        (Shell::Posix, KeyChange::Add { line, body }) => posix_install_command(line, body),
        (Shell::Posix, KeyChange::Remove { body }) => posix_remove_command(body),
        (Shell::Cmd | Shell::PowerShell, KeyChange::Add { line, body }) => {
            windows_command(&format!(
                "{elsewhere}$line = {}\n$body = {}\n{WINDOWS_INSTALL_SCRIPT}",
                powershell_value(line),
                powershell_value(body)
            ))
        }
        (Shell::Cmd | Shell::PowerShell, KeyChange::Remove { body }) => windows_command(&format!(
            "{elsewhere}$body = {}\n{WINDOWS_REMOVE_SCRIPT}",
            powershell_value(body)
        )),
    }
}

/// The command that appends `line`, whose key body is `body`, to the remote user's
/// `~/.ssh/authorized_keys`, as `ssh-copy-id` does: run through `sh` whatever the login shell is;
/// `~/.ssh` and the file made private where they are made, and left as they are where they exist;
/// nothing appended when the key is already there; a last line with no line ending ended first, so
/// the entry above is not disturbed. The file is searched as bytes (`LC_ALL=C`, `-a`): a line in
/// another encoding, or a NUL, does not hide what follows it.
fn posix_install_command(line: &str, body: &str) -> String {
    let script = format!(
        "umask 077\n\
         cd || exit 1\n\
         mkdir -p .ssh || exit 1\n\
         f=.ssh/authorized_keys\n\
         if [ -f \"$f\" ] && LC_ALL=C grep -a -qF -- {body} \"$f\"; then exit 0; fi\n\
         if [ -s \"$f\" ] && [ -n \"$(tail -c 1 \"$f\")\" ]; then echo >> \"$f\" || exit 1; fi\n\
         printf '%s\\n' {line} >> \"$f\"\n",
        body = shell_word(body),
        line = shell_word(line),
    );
    format!("sh -c {}", shell_word(&script))
}

/// The command that takes the line carrying the key body `body` out of the remote user's
/// `~/.ssh/authorized_keys`, exiting with `NOT_FOUND_STATUS` where there is no such file or no
/// such line.
///
/// The lines are filtered as bytes (`LC_ALL=C grep -a`): in a UTF-8 locale `grep` drops a line it
/// cannot decode, or stops at a NUL, and another key would be lost with ours. A last line with no
/// line ending keeps having none (`grep` ends every line it prints).
///
/// The lines to keep are written back into the file itself, not into a new file renamed over it.
/// A rename would be atomic, but it replaces the file: it breaks a symbolic or a hard link, and
/// gives the new file the owner, access control list and security label of whatever made it,
/// which an SSH server that checks them may then refuse, locking the user out of the host.
/// Writing in place keeps all of that; its cost is the moment the file is being rewritten, so the
/// file is checked to be writable before anything is touched, and a copy taken first is written
/// back where the write fails. The working copies live in a private temporary directory, made
/// where `TMPDIR` says or else in `/tmp` (named in full: a bare `mktemp -d` picks its own place on
/// some systems, macOS for one, whatever `TMPDIR` says), that is
/// removed however the command ends, so nothing of it stays on the host, with one exception: where
/// putting the copy back fails too (a disk that filled, a home that went away), the directory is
/// kept, since the copy in it is then the only whole one, and the command says where it is
/// (`INCOMPLETE_ANSWER`) and ends with `INCOMPLETE_STATUS`.
fn posix_remove_command(body: &str) -> String {
    let script = format!(
        "umask 077\n\
         cd || exit 1\n\
         f=.ssh/authorized_keys\n\
         [ -f \"$f\" ] || exit {missing}\n\
         LC_ALL=C grep -a -qF -- {body} \"$f\"\n\
         case $? in 0) ;; 1) exit {missing} ;; *) exit 1 ;; esac\n\
         if ! [ -w \"$f\" ]; then echo \"$f cannot be written\" >&2; exit 1; fi\n\
         d=$(mktemp -d \"${{TMPDIR:-/tmp}}/octoboard-keys.XXXXXX\") || exit 1\n\
         trap 'rm -rf \"$d\"' EXIT\n\
         cp \"$f\" \"$d/before\" || exit 1\n\
         LC_ALL=C grep -a -vF -- {body} \"$f\" > \"$d/kept\"\n\
         [ $? -le 1 ] || exit 1\n\
         if [ -n \"$(tail -c 1 \"$f\")\" ] && ! tail -n 1 \"$f\" | LC_ALL=C grep -a -qF -- {body}; then\n\
         head -c $(($(wc -c < \"$d/kept\") - 1)) \"$d/kept\" > \"$d/trimmed\" || exit 1\n\
         mv \"$d/trimmed\" \"$d/kept\" || exit 1\n\
         fi\n\
         if ! cat \"$d/kept\" > \"$f\"; then\n\
         if cat \"$d/before\" > \"$f\"; then exit 1; fi\n\
         trap - EXIT\n\
         printf '{answer}%s' \"$d/before\"\n\
         echo \"$f may be incomplete: writing it failed, and so did putting its content back\" >&2\n\
         exit {incomplete}\n\
         fi\n",
        body = shell_word(body),
        missing = NOT_FOUND_STATUS,
        answer = INCOMPLETE_ANSWER,
        incomplete = INCOMPLETE_STATUS,
    );
    format!("sh -c {}", shell_word(&script))
}

/// What every PowerShell script here begins with: `$file` is the file the remote user's login
/// reads its keys from, and `$bytes` an encoding that maps every byte to one character and back
/// (ISO 8859-1), so a file read and written with it keeps every byte of every line, whatever
/// encoding the lines are in and whether or not it starts with a byte order mark. An
/// administrator's keys are read from `%ProgramData%\ssh\administrators_authorized_keys` by the
/// OpenSSH server's default configuration, any other user's from `~\.ssh\authorized_keys`.
const WINDOWS_SCRIPT_HEAD: &str = r#"$ErrorActionPreference = 'Stop'
# A PowerShell run with its output redirected writes progress records to stderr as CLIXML.
$ProgressPreference = 'SilentlyContinue'
try {
$bytes = [Text.Encoding]::GetEncoding(28591)
$administrator = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($administrator) {
  $file = Join-Path $env:ProgramData 'ssh\administrators_authorized_keys'
} else {
  $file = Join-Path $HOME '.ssh\authorized_keys'
}
"#;

/// What every PowerShell script here ends with: a failure's message, written as `CARRIED` text.
const WINDOWS_SCRIPT_TAIL: &str = r#"exit 0
} catch {
[Console]::Error.Write('utf8:' + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($_.Exception.Message)))
exit 1
}
"#;

/// Prints the file the remote user's login reads its keys from, as `CARRIED` text, and writes
/// nothing.
const WINDOWS_FILE_SCRIPT: &str = "[Console]::Out.Write('utf8:' + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($file)))\n";

/// Appends `$line`, whose key body is `$body`. The server accepts an administrator's file only
/// where just Administrators and SYSTEM may use it, so a file made here is given that; an existing
/// file's access rights are left as they are, nothing is appended where the key is already there,
/// and a last line with no line ending is ended first. The line is appended as UTF-8 without a
/// byte order mark (PowerShell 5.1's own encodings would add one or use UTF-16); what the file
/// already holds is only read, as bytes, and is not written again. The bytes are read as such and
/// then decoded: `ReadAllText` would decode a file that starts with a byte order mark with that
/// mark's encoding, whatever encoding it is given, and so miss a key behind a line in another
/// encoding. A file written as UTF-16 (which Windows PowerShell's `>` makes, and the SSH server
/// reads no key from) is refused: a line of UTF-8 appended to it would be neither.
const WINDOWS_INSTALL_SCRIPT: &str = r#"$directory = Split-Path -Parent $file
if (-not (Test-Path -LiteralPath $directory)) { New-Item -ItemType Directory -Path $directory | Out-Null }
if (-not (Test-Path -LiteralPath $file)) {
  New-Item -ItemType File -Path $file | Out-Null
  if ($administrator) {
    icacls $file /inheritance:r /grant '*S-1-5-32-544:F' /grant '*S-1-5-18:F' | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "icacls exited with status $LASTEXITCODE" }
  }
}
$held = [IO.File]::ReadAllBytes($file)
if ($held.Length -ge 2 -and (($held[0] -eq 0xFF -and $held[1] -eq 0xFE) -or ($held[0] -eq 0xFE -and $held[1] -eq 0xFF))) {
  throw 'the file is written as UTF-16, which the SSH server reads no key from; nothing was added to it'
}
$text = $bytes.GetString($held)
if ($text.Contains($body)) { exit 0 }
$prefix = ''
if ($text.Length -gt 0 -and -not $text.EndsWith("`n")) { $prefix = "`r`n" }
[IO.File]::AppendAllText($file, $prefix + $line + "`r`n", (New-Object Text.UTF8Encoding $false))
"#;

/// Takes the line carrying the key body `$body` out of the file, every other line kept with its
/// own bytes and its own line ending, a last line with none included; exits with
/// `NOT_FOUND_STATUS`, having written `NOT_FOUND_ANSWER`, where there is no such file or no such
/// line. The file is written in place, so it stays the same file with its owner and access rights;
/// a copy of it is taken first, in the user's temporary folder, and is put back where the write
/// fails. Where that fails too, the copy is kept and the script says where it is
/// (`INCOMPLETE_ANSWER`) before it ends with `INCOMPLETE_STATUS`; otherwise the copy is deleted.
/// (`[IO.File]::Replace` would swap a new file in at once, but the file would then be another one,
/// with the owner of whoever ran the script.) Its bytes are read and written as such, and decoded and encoded in
/// between: `ReadAllText` would decode a file that starts with a byte order mark with that mark's
/// encoding and drop the mark, and writing the text back would lose both the mark and every byte
/// that is not text in that encoding.
const WINDOWS_REMOVE_SCRIPT: &str = r#"if (-not (Test-Path -LiteralPath $file)) { [Console]::Out.Write('no-such-line'); exit 3 }
$text = $bytes.GetString([IO.File]::ReadAllBytes($file))
if (-not $text.Contains($body)) { [Console]::Out.Write('no-such-line'); exit 3 }
$kept = New-Object Text.StringBuilder
foreach ($part in [regex]::Split($text, '(?<=\n)')) {
  if (-not $part.Contains($body)) { [void]$kept.Append($part) }
}
$copy = [IO.Path]::GetTempFileName()
[IO.File]::Copy($file, $copy, $true)
try {
  [IO.File]::WriteAllBytes($file, $bytes.GetBytes($kept.ToString()))
} catch {
  $failure = $_.Exception.Message
  try {
    [IO.File]::WriteAllBytes($file, [IO.File]::ReadAllBytes($copy))
  } catch {
    [Console]::Out.Write('file-incomplete utf8:' + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($copy)))
    [Console]::Error.Write('utf8:' + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($failure + ' ' + $_.Exception.Message)))
    exit 4
  }
  [IO.File]::Delete($copy)
  throw $failure
}
[IO.File]::Delete($copy)
"#;

/// A PowerShell expression whose value is `text`: the text is carried as base64 and decoded by
/// the script, so that nothing in it is read as PowerShell. A quoted literal would not do: a
/// device's name is the owner's to choose, and PowerShell ends a single-quoted string at any of
/// four typographic quotation marks as well as at `'` (a Mac is named `Ann’s MacBook Pro` by
/// default).
fn powershell_value(text: &str) -> String {
    use base64::Engine as _;
    format!(
        "[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{}'))",
        base64::engine::general_purpose::STANDARD.encode(text)
    )
}

/// The command that runs the PowerShell `body` between `WINDOWS_SCRIPT_HEAD` and
/// `WINDOWS_SCRIPT_TAIL` on a Windows host: one script, passed encoded so that neither the command
/// prompt nor PowerShell as the SSH server's shell reads any of it.
fn windows_command(body: &str) -> String {
    use base64::Engine as _;
    let script = format!("{WINDOWS_SCRIPT_HEAD}{body}{WINDOWS_SCRIPT_TAIL}");
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    format!(
        "powershell.exe -NoProfile -NonInteractive -EncodedCommand {}",
        base64::engine::general_purpose::STANDARD.encode(utf16)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `change` as a login would, with `home` as the home directory.
    #[cfg(unix)]
    fn run(home: &std::path::Path, change: KeyChange<'_>) -> std::process::Output {
        let _spawning = crate::spawning();
        std::process::Command::new("sh")
            .arg("-c")
            .arg(change_command(Shell::Posix, change))
            .env("HOME", home)
            .output()
            .unwrap()
    }

    #[cfg(unix)]
    fn install(home: &std::path::Path, line: &str, body: &str) -> std::process::Output {
        run(home, KeyChange::Add { line, body })
    }

    #[cfg(unix)]
    fn remove(home: &std::path::Path, body: &str) -> std::process::Output {
        run(home, KeyChange::Remove { body })
    }

    #[cfg(unix)]
    fn mode(path: &std::path::Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    /// A scratch home with an `.ssh` directory, and the path of its authorized keys.
    #[cfg(unix)]
    fn home(name: &str) -> (crate::config_dir::test_support::Scratch, std::path::PathBuf) {
        let scratch = crate::config_dir::test_support::Scratch::new("authorized-keys", name);
        std::fs::create_dir_all(scratch.0.join(".ssh")).unwrap();
        let file = scratch.0.join(".ssh/authorized_keys");
        (scratch, file)
    }

    /// What is left in the temporary directory `command` ran with, which a command that leaves
    /// nothing behind leaves empty.
    #[cfg(unix)]
    fn remove_with_tmp(
        home: &std::path::Path,
        tmp: &std::path::Path,
        body: &str,
    ) -> (std::process::Output, usize) {
        std::fs::create_dir_all(tmp).unwrap();
        let _spawning = crate::spawning();
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(change_command(Shell::Posix, KeyChange::Remove { body }))
            .env("HOME", home)
            .env("TMPDIR", tmp)
            .output()
            .unwrap();
        let left = std::fs::read_dir(tmp).unwrap().count();
        (output, left)
    }

    #[cfg(unix)]
    #[test]
    fn installing_makes_a_private_file_where_there_is_none() {
        let scratch = crate::config_dir::test_support::Scratch::new("authorized-keys", "fresh");
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
        let (scratch, file) = home("existing");
        let ssh = scratch.0.join(".ssh");
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o750)).unwrap();
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
    fn installing_finds_the_key_already_there_behind_a_line_that_is_not_text() {
        let (scratch, file) = home("install-bytes");
        // A comment in Latin-1 and a NUL come before the key that is already installed.
        let mut held = b"ssh-rsa AAAAother caf\xe9\n\0\n".to_vec();
        held.extend_from_slice(b"ssh-ed25519 AAAAmine Octoboard on Desk\n");
        std::fs::write(&file, &held).unwrap();
        assert!(install(
            &scratch.0,
            "ssh-ed25519 AAAAmine Octoboard on Desk",
            "AAAAmine"
        )
        .status
        .success());
        assert_eq!(std::fs::read(&file).unwrap(), held);
    }

    #[cfg(unix)]
    #[test]
    fn installing_fails_where_the_file_cannot_be_written() {
        use std::os::unix::fs::PermissionsExt;
        let (scratch, file) = home("readonly");
        std::fs::write(&file, "ssh-rsa AAAAother laptop\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o400)).unwrap();
        let output = install(&scratch.0, "ssh-ed25519 AAAAmine x", "AAAAmine");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(!output.status.success(), "{output:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_name_with_quotes_and_shell_syntax_reaches_the_file_as_written() {
        let scratch = crate::config_dir::test_support::Scratch::new("authorized-keys", "quoting");
        std::fs::create_dir_all(&scratch.0).unwrap();
        let line = "ssh-ed25519 AAAAbody Octoboard on $(touch pwned); `id` \"x\" 'y' Ann\u{2019}s";
        assert!(install(&scratch.0, line, "AAAAbody").status.success());
        let file = scratch.0.join(".ssh/authorized_keys");
        assert_eq!(std::fs::read_to_string(file).unwrap(), format!("{line}\n"));
        assert!(!scratch.0.join("pwned").exists());
    }

    #[cfg(unix)]
    #[test]
    fn removing_takes_out_the_key_s_line_alone_and_keeps_the_file_as_it_was_kept() {
        use std::os::unix::fs::PermissionsExt;
        let (scratch, file) = home("remove");
        // A comment, a blank line and another key of the same kind around ours.
        let before = "ssh-rsa AAAAother laptop\n# mine\n\n";
        let after = "ssh-ed25519 AAAAtheirs Octoboard on Other\nssh-ed25519 AAAAlast phone\n";
        let line = "ssh-ed25519 AAAAmine Octoboard on $(touch pwned) 'Desk'";
        std::fs::write(&file, format!("{before}{line}\n{after}")).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
        let (output, left) = remove_with_tmp(&scratch.0, &scratch.0.join("tmp"), "AAAAmine");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            format!("{before}{after}")
        );
        assert_eq!(mode(&file), 0o640);
        // Nothing of the command's own stays on the host.
        assert_eq!(left, 0);
        assert!(!scratch.0.join("pwned").exists());
    }

    #[cfg(unix)]
    #[test]
    fn removing_keeps_every_byte_of_lines_that_are_not_text() {
        let (scratch, file) = home("remove-bytes");
        // Someone else's key with a Latin-1 comment, a line holding a NUL, and lines ending in
        // CRLF, before and after ours: in a UTF-8 locale a text filter drops or cuts these.
        let before = b"ssh-rsa AAAAother caf\xe9 laptop\nssh-rsa AAAAnul a\0b\r\n".to_vec();
        let after = b"ssh-ed25519 AAAAcrlf phone\r\nssh-rsa AAAAtail \xff\xfe\n".to_vec();
        let mut held = before.clone();
        held.extend_from_slice(b"ssh-ed25519 AAAAmine Octoboard on Desk\n");
        held.extend_from_slice(&after);
        std::fs::write(&file, &held).unwrap();
        let _spawning = crate::spawning();
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(change_command(
                Shell::Posix,
                KeyChange::Remove { body: "AAAAmine" },
            ))
            .env("HOME", &scratch.0)
            // The locale a login usually has, in which the filter must still work on bytes.
            .env("LANG", "en_US.UTF-8")
            .env_remove("LC_ALL")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(std::fs::read(&file).unwrap(), [before, after].concat());
    }

    #[cfg(unix)]
    #[test]
    fn removing_leaves_a_last_line_of_someone_else_s_without_the_line_ending_it_never_had() {
        let (scratch, file) = home("remove-unterminated");
        let line = "ssh-ed25519 AAAAmine Octoboard on Desk";
        // Ours in the middle, and the file's last line, another key, has no line ending.
        std::fs::write(
            &file,
            format!("ssh-rsa AAAAother laptop\n{line}\nssh-ed25519 AAAAlast phone"),
        )
        .unwrap();
        assert!(remove(&scratch.0, "AAAAmine").status.success());
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "ssh-rsa AAAAother laptop\nssh-ed25519 AAAAlast phone"
        );
        // Ours last and unterminated: the line before it keeps its own ending.
        std::fs::write(&file, format!("ssh-rsa AAAAother laptop\n{line}")).unwrap();
        assert!(remove(&scratch.0, "AAAAmine").status.success());
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "ssh-rsa AAAAother laptop\n"
        );
        // Ours the only line: the file stays, empty.
        std::fs::write(&file, format!("{line}\n")).unwrap();
        assert!(remove(&scratch.0, "AAAAmine").status.success());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "");
    }

    #[cfg(unix)]
    #[test]
    fn removing_a_key_that_is_not_in_the_file_says_so_and_changes_nothing() {
        let (scratch, file) = home("remove-none");
        let held = "ssh-rsa AAAAother laptop\n";
        std::fs::write(&file, held).unwrap();
        let (output, left) = remove_with_tmp(&scratch.0, &scratch.0.join("tmp"), "AAAAmine");
        assert_eq!(output.status.code(), Some(NOT_FOUND_STATUS as i32));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), held);
        assert_eq!(left, 0);
        // A home with no such file has no line either, and nothing is made in it.
        let bare = crate::config_dir::test_support::Scratch::new("authorized-keys", "remove-bare");
        std::fs::create_dir_all(&bare.0).unwrap();
        assert_eq!(
            remove(&bare.0, "AAAAmine").status.code(),
            Some(NOT_FOUND_STATUS as i32)
        );
        assert!(!bare.0.join(".ssh").exists());
    }

    /// Runs the removal with a `cat` of the test's own first on the path, which fails as a full
    /// disk would: every time, or, with `once`, the first time only. What the command ended with
    /// and what is left in its temporary directory.
    #[cfg(unix)]
    fn remove_with_failing_cat(
        scratch: &crate::config_dir::test_support::Scratch,
        once: bool,
    ) -> (std::process::Output, Vec<std::path::PathBuf>) {
        use std::os::unix::fs::PermissionsExt;
        let (bin, tmp) = (scratch.0.join("bin"), scratch.0.join("tmp"));
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&tmp).unwrap();
        let failed = bin.join("failed");
        let script = if once {
            format!(
                "#!/bin/sh\nif [ -e '{0}' ]; then exec /bin/cat \"$@\"; fi\n: > '{0}'\nexit 1\n",
                failed.display()
            )
        } else {
            "#!/bin/sh\nexit 1\n".to_owned()
        };
        std::fs::write(bin.join("cat"), script).unwrap();
        std::fs::set_permissions(bin.join("cat"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let _spawning = crate::spawning();
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(change_command(
                Shell::Posix,
                KeyChange::Remove { body: "AAAAmine" },
            ))
            .env("HOME", &scratch.0)
            .env("TMPDIR", &tmp)
            .env("PATH", path)
            .output()
            .unwrap();
        let left = std::fs::read_dir(&tmp)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        (output, left)
    }

    #[cfg(unix)]
    #[test]
    fn a_write_that_fails_puts_the_file_s_content_back_and_leaves_nothing_behind() {
        let (scratch, file) = home("remove-write-fails");
        let text = "ssh-rsa AAAAother laptop\nssh-ed25519 AAAAmine Octoboard on Desk\nssh-ed25519 AAAAlast phone\n";
        std::fs::write(&file, text).unwrap();
        let (output, left) = remove_with_failing_cat(&scratch, true);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        // Whole again, every key in it, and the working copies gone.
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
        assert_eq!(left, Vec::<std::path::PathBuf>::new());
        assert_eq!(
            change_outcome(
                KeyChange::Remove { body: "AAAAmine" },
                Some(1),
                &output.stdout,
                &output.stderr
            ),
            Err(ChangeError::Failed(
                "changing the host's authorized keys ended with Some(1): ".into()
            ))
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_write_and_a_restore_that_both_fail_keep_the_only_whole_copy_and_say_where_it_is() {
        let (scratch, file) = home("remove-restore-fails");
        let text = "ssh-rsa AAAAother laptop\nssh-ed25519 AAAAmine Octoboard on Desk\nssh-ed25519 AAAAlast phone\n";
        std::fs::write(&file, text).unwrap();
        let (output, left) = remove_with_failing_cat(&scratch, false);
        assert_eq!(
            output.status.code(),
            Some(INCOMPLETE_STATUS as i32),
            "{output:?}"
        );
        // The file was emptied by the write that failed, and could not be put back.
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "");
        // What the command wrote and left, for a failure to show.
        let seen = format!(
            "stdout: {:?}\nstderr: {:?}\nleft in the temporary folder: {left:?}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        // The temporary directory is kept, and the copy in it holds every line the file had.
        assert_eq!(left.len(), 1, "{seen}");
        let copy = left[0].join("before");
        assert!(copy.is_file(), "{seen}");
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), text, "{seen}");
        // The command names that copy, and the application tells it apart from any other failure.
        let outcome = change_outcome(
            KeyChange::Remove { body: "AAAAmine" },
            output.status.code().map(|code| code as u32),
            &output.stdout,
            &output.stderr,
        );
        let Err(ChangeError::Incomplete {
            copy: named,
            detail,
        }) = outcome
        else {
            panic!("expected the file to be told as incomplete, got {outcome:?}\n{seen}");
        };
        // The path it names is that copy's, as the folder it was told to work in spells it.
        assert_eq!(std::path::PathBuf::from(&named), copy, "{seen}");
        assert!(
            detail.contains(".ssh/authorized_keys may be incomplete"),
            "{detail}\n{seen}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn removing_fails_where_the_file_cannot_be_written_and_leaves_it_whole_and_nothing_behind() {
        use std::os::unix::fs::PermissionsExt;
        let (scratch, file) = home("remove-readonly");
        let text = "ssh-rsa AAAAother laptop\nssh-ed25519 AAAAmine Octoboard on Desk\n";
        std::fs::write(&file, text).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o400)).unwrap();
        let (output, left) = remove_with_tmp(&scratch.0, &scratch.0.join("tmp"), "AAAAmine");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("cannot be written"),
            "{output:?}"
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
        assert_eq!(left, 0);
    }

    /// The script a Windows command carries, decoded.
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

    /// The text the script's line starting with `assignment` gives its variable, decoded from the
    /// base64 it is carried as; the line must hold nothing but that one expression.
    fn carried(script: &str, assignment: &str) -> String {
        use base64::Engine as _;
        let line = script
            .lines()
            .find(|line| line.starts_with(assignment))
            .unwrap_or_else(|| panic!("no line starting with {assignment:?} in {script}"));
        let encoded = line
            .strip_prefix(assignment)
            .and_then(|rest| {
                rest.strip_prefix("[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('")
            })
            .and_then(|rest| rest.strip_suffix("'))"))
            .unwrap_or_else(|| panic!("{line:?} is not one base64 value"));
        assert!(
            encoded
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=')),
            "{encoded}"
        );
        String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn a_windows_host_gets_one_encoded_powershell_script_whichever_its_shell() {
        let line = "ssh-ed25519 AAAAbody Octoboard on Ann's PC";
        let add = KeyChange::Add {
            line,
            body: "AAAAbody",
        };
        let cmd = change_command(Shell::Cmd, add);
        assert_eq!(cmd, change_command(Shell::PowerShell, add));
        assert!(cmd.is_ascii() && !cmd.contains('"') && !cmd.contains('\''));
        let script = decoded_script(&cmd);
        assert_eq!(carried(&script, "$line = "), line);
        assert_eq!(carried(&script, "$body = "), "AAAAbody");
        assert!(script.contains("IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)"));
        assert!(script.contains(r"'ssh\administrators_authorized_keys'"));
        assert!(script.contains("$env:ProgramData"));
        assert!(script.contains(r"'.ssh\authorized_keys'"));
        assert!(script.contains("/inheritance:r /grant '*S-1-5-32-544:F' /grant '*S-1-5-18:F'"));
        // What the file holds is read as bytes and never written again; only the line is added.
        assert!(script.contains("$held = [IO.File]::ReadAllBytes($file)"));
        assert!(script.contains("$text = $bytes.GetString($held)"));
        assert!(script.contains("AppendAllText($file, $prefix + $line + \"`r`n\", (New-Object Text.UTF8Encoding $false))"));
        // No call that decodes by a byte order mark it finds, and none that writes the file anew.
        assert!(!script.contains("ReadAllText"));
        assert!(!script.contains("WriteAllText") && !script.contains("WriteAllBytes"));
        // A file written as UTF-16 is refused before anything is appended to it.
        let refusal = script.find("the file is written as UTF-16").unwrap();
        assert!(refusal < script.find("AppendAllText").unwrap());
    }

    #[test]
    fn a_hostile_device_name_reaches_a_windows_host_as_data_and_never_as_script() {
        // Every character PowerShell ends a single-quoted string at, what it expands in a
        // double-quoted one, a statement separator, a line break, and the names the script itself
        // uses: none of it may appear in the script outside the base64 it is carried as.
        let name = "Ann\u{2019}s \u{2018}Mac\u{201A}\u{201B}' \"; $body; $(calc) `n {BODY} {LINE}\nexit 0 #";
        let line = format!("ssh-ed25519 AAAAbody Octoboard on {name}");
        let add = KeyChange::Add {
            line: &line,
            body: "AAAAbody",
        };
        for shell in [Shell::Cmd, Shell::PowerShell] {
            let cmd = change_command(shell, add);
            assert!(cmd.is_ascii() && !cmd.contains('"') && !cmd.contains('\''));
            let script = decoded_script(&cmd);
            // Both values arrive whole, the line's `{BODY}` included, and each on a line of its
            // own that is one base64 expression.
            assert_eq!(carried(&script, "$line = "), line);
            assert_eq!(carried(&script, "$body = "), "AAAAbody");
            for fragment in [
                "Ann", "Mac", "calc", "\u{2019}", "\u{2018}", "{BODY}", "{LINE}",
            ] {
                assert!(!script.contains(fragment), "{fragment} in {script}");
            }
            // The script is the fixed text around the two values and nothing else.
            let fixed: Vec<&str> = script
                .lines()
                .filter(|line| !line.starts_with("$line = ") && !line.starts_with("$body = "))
                .collect();
            let expected =
                format!("{WINDOWS_SCRIPT_HEAD}{WINDOWS_INSTALL_SCRIPT}{WINDOWS_SCRIPT_TAIL}");
            assert_eq!(fixed, expected.lines().collect::<Vec<_>>());
        }
    }

    #[test]
    fn a_windows_host_s_removal_and_its_file_question_share_the_install_s_choice_of_file() {
        let remove = KeyChange::Remove { body: "AAAA'body" };
        let cmd = change_command(Shell::Cmd, remove);
        assert_eq!(cmd, change_command(Shell::PowerShell, remove));
        assert!(cmd.is_ascii() && !cmd.contains('"') && !cmd.contains('\''));
        let script = decoded_script(&cmd);
        assert_eq!(carried(&script, "$body = "), "AAAA'body");
        assert!(script.contains(r"'ssh\administrators_authorized_keys'"));
        assert!(script.contains(r"'.ssh\authorized_keys'"));
        // Every other line is kept with its own line ending, and the file is read and written in
        // place as bytes, so no line is re-encoded and no byte order mark added or dropped;
        // nothing is made, and no access right changed.
        assert!(script.contains("$bytes = [Text.Encoding]::GetEncoding(28591)"));
        assert!(script.contains(r"[regex]::Split($text, '(?<=\n)')"));
        assert!(script.contains("$text = $bytes.GetString([IO.File]::ReadAllBytes($file))"));
        assert!(
            script.contains("[IO.File]::WriteAllBytes($file, $bytes.GetBytes($kept.ToString()))")
        );
        // No call that decodes by a byte order mark it finds, or writes one.
        assert!(!script.contains("ReadAllText") && !script.contains("WriteAllText"));
        assert!(!script.contains("UTF8Encoding"));
        assert!(!script.contains("New-Item") && !script.contains("icacls"));
        // A file or a line that is not there is told apart from a removal.
        assert_eq!(script.matches("exit 3").count(), 2);
        assert_eq!(NOT_FOUND_STATUS, 3);
        // Each is announced first in words, which arrive where the status does not, and nothing
        // else is written to the same place by the removal.
        assert_eq!(
            script
                .matches(&format!(
                    "{{ [Console]::Out.Write('{NOT_FOUND_ANSWER}'); exit 3 }}"
                ))
                .count(),
            2
        );
        // The one other thing the removal writes there is where it kept the file's earlier content,
        // should both the write and putting that content back fail; a copy it did not need is
        // deleted, after a write that held and after one that was put back.
        assert_eq!(script.matches("[Console]::Out.Write(").count(), 3);
        assert!(script.contains(&format!(
            "[Console]::Out.Write('{INCOMPLETE_ANSWER}utf8:' + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($copy)))"
        )));
        assert_eq!(script.matches("exit 4").count(), 1);
        assert_eq!(INCOMPLETE_STATUS, 4);
        let copied = script.find("[IO.File]::Copy($file, $copy, $true)").unwrap();
        let written = script
            .find("[IO.File]::WriteAllBytes($file, $bytes.GetBytes(")
            .unwrap();
        assert!(copied < written);
        assert_eq!(script.matches("[IO.File]::Delete($copy)").count(), 2);
        // Asking which file it is writes nothing at all.
        let asked = decoded_script(&windows_command(WINDOWS_FILE_SCRIPT));
        assert!(asked.contains("[Console]::Out.Write('utf8:' + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($file)))"));
        for writes in [
            "New-Item",
            "icacls",
            "AppendAllText",
            "WriteAllText",
            "WriteAllBytes",
        ] {
            assert!(!asked.contains(writes), "{writes}");
        }
        assert!(asked.contains(r"'ssh\administrators_authorized_keys'"));
    }

    #[test]
    fn a_posix_host_gets_sh_scripts_that_search_the_file_as_bytes() {
        let add = change_command(
            Shell::Posix,
            KeyChange::Add {
                line: "ssh-ed25519 AAAAbody x",
                body: "AAAAbody",
            },
        );
        assert!(add.starts_with("sh -c '"));
        assert!(add.contains("LC_ALL=C grep -a -qF"));
        let remove = change_command(Shell::Posix, KeyChange::Remove { body: "AAAAbody" });
        assert!(remove.starts_with("sh -c '"));
        assert!(remove.contains("LC_ALL=C grep -a -vF"));
        // No search of the file is left to the login's locale.
        for command in [&add, &remove] {
            assert_eq!(
                command.matches("grep ").count(),
                command.matches("LC_ALL=C grep -a ").count(),
                "{command}"
            );
        }
        assert_eq!(POSIX_FILE, "~/.ssh/authorized_keys");
    }

    #[test]
    fn a_file_given_in_place_of_the_login_s_changes_one_line_of_a_windows_script_and_no_more() {
        let file = r"C:\Users\Ann’s\AppData\Local\Temp\scratch\authorized_keys";
        for change in [
            KeyChange::Add {
                line: "ssh-ed25519 AAAAbody x",
                body: "AAAAbody",
            },
            KeyChange::Remove { body: "AAAAbody" },
        ] {
            let usual = decoded_script(&change_command(Shell::Cmd, change));
            let elsewhere = decoded_script(&change_command_at(Shell::Cmd, change, Some(file)));
            // The file is named after the script has chosen the login's, so it replaces that
            // choice, and before anything reads or writes the file.
            assert_eq!(carried(&elsewhere, "$file = "), file);
            let head_lines = WINDOWS_SCRIPT_HEAD.lines().count();
            let mut lines: Vec<&str> = elsewhere.lines().collect();
            assert!(lines
                .remove(head_lines)
                .starts_with("$file = [Text.Encoding]"));
            assert_eq!(lines, usual.lines().collect::<Vec<_>>());
            // A POSIX host's script has no such line: its tests give it another home instead.
            assert_eq!(
                change_command_at(Shell::Posix, change, Some(file)),
                change_command(Shell::Posix, change)
            );
        }
    }

    #[test]
    fn what_a_windows_host_says_is_read_whatever_code_page_its_console_writes_in() {
        use base64::Engine as _;
        let carried = |text: &str| {
            format!(
                "{CARRIED}{}",
                base64::engine::general_purpose::STANDARD.encode(text)
            )
        };
        // A path and a message in scripts a code page would lose arrive whole.
        let path = "C:\\Users\\\u{540D}\u{524D} Ann\u{2019}s\\.ssh\\authorized_keys";
        assert_eq!(host_said(carried(path).as_bytes()), path);
        assert_eq!(
            host_said(format!("  {}\r\n", carried("Zugriff verweigert: \u{fc}")).as_bytes()),
            "Zugriff verweigert: \u{fc}"
        );
        assert_eq!(host_said(carried("").as_bytes()), "");
        // The scripts mark what they write the way it is read.
        assert_eq!(CARRIED, "utf8:");
        assert!(WINDOWS_SCRIPT_TAIL
            .contains("[Console]::Error.Write('utf8:' + [Convert]::ToBase64String("));
        // What is not marked, a POSIX host's words or PowerShell's own, is taken as written, and
        // so is a mark with nothing readable behind it.
        assert_eq!(
            host_said(b" .ssh/authorized_keys cannot be written\n"),
            ".ssh/authorized_keys cannot be written"
        );
        assert_eq!(host_said(b"utf8:not base64!"), "utf8:not base64!");
        assert_eq!(host_said(b""), "");
    }

    #[test]
    fn a_line_that_was_not_there_is_told_apart_from_a_failed_removal_whatever_ran_the_script() {
        let remove = KeyChange::Remove { body: "AAAAbody" };
        let add = KeyChange::Add {
            line: "ssh-ed25519 AAAAbody x",
            body: "AAAAbody",
        };
        let failed = |status: Option<u32>, said: &str| {
            Err(ChangeError::Failed(format!(
                "changing the host's authorized keys ended with {status:?}: {said}"
            )))
        };
        assert_eq!(change_outcome(remove, Some(0), b"", b""), Ok(()));
        // The status as the script ends with it: a POSIX host, and the command prompt as the SSH
        // server's shell.
        assert_eq!(
            change_outcome(remove, Some(3), b"", b""),
            Err(ChangeError::NotFound)
        );
        assert_eq!(
            change_outcome(remove, Some(3), b"no-such-line", b""),
            Err(ChangeError::NotFound)
        );
        // PowerShell as the SSH server's shell ends with 1 for any status but 0 of the script it
        // ran: the script's words say which it was.
        assert_eq!(
            change_outcome(remove, Some(1), b"no-such-line", b""),
            Err(ChangeError::NotFound)
        );
        assert_eq!(
            change_outcome(remove, Some(1), b"  no-such-line\r\n", b""),
            Err(ChangeError::NotFound)
        );
        // A removal that failed is a failure, with what the host said.
        assert_eq!(
            change_outcome(remove, Some(1), b"", b"utf8:ZGVuaWVk"),
            failed(Some(1), "denied")
        );
        assert_eq!(
            change_outcome(remove, Some(1), b"no-such-line and more", b"boom"),
            failed(Some(1), "boom")
        );
        assert_eq!(
            change_outcome(remove, None, b"no-such-line", b""),
            failed(None, "")
        );
        // A file left possibly incomplete is told with where its earlier content is kept, as a
        // POSIX host writes it and as a Windows host carries it, whatever the status became.
        assert_eq!(
            change_outcome(
                remove,
                Some(4),
                b"file-incomplete /tmp/tmp.x/before",
                b"cat: full\n"
            ),
            Err(ChangeError::Incomplete {
                copy: "/tmp/tmp.x/before".into(),
                detail: "cat: full".into()
            })
        );
        assert_eq!(
            change_outcome(
                remove,
                Some(1),
                b"file-incomplete utf8:QzpcVGVtcFx0bXAxLnRtcA==",
                b"utf8:ZGVuaWVk"
            ),
            Err(ChangeError::Incomplete {
                copy: "C:\\Temp\\tmp1.tmp".into(),
                detail: "denied".into()
            })
        );
        // The status alone, with no word of where the copy is, is a failure like any other.
        assert_eq!(
            change_outcome(remove, Some(4), b"", b"boom"),
            failed(Some(4), "boom")
        );
        assert_eq!(
            change_outcome(add, Some(4), b"file-incomplete /x", b""),
            failed(Some(4), "")
        );
        // Adding has no such answer: a key that is not there is what it is for.
        assert_eq!(change_outcome(add, Some(0), b"", b""), Ok(()));
        assert_eq!(change_outcome(add, Some(3), b"", b""), failed(Some(3), ""));
        assert_eq!(
            change_outcome(add, Some(1), b"no-such-line", b""),
            failed(Some(1), "")
        );
    }

    #[test]
    fn why_a_change_was_not_made_reads_as_a_reason() {
        assert!(ChangeError::NotFound
            .to_string()
            .contains("no line carrying this key"));
        assert_eq!(ChangeError::Failed("boom".into()).to_string(), "boom");
        assert_eq!(
            ChangeError::Incomplete {
                copy: "/tmp/x/before".into(),
                detail: "full".into()
            }
            .to_string(),
            "the file may be incomplete; its earlier content is kept on the host in /tmp/x/before: full"
        );
    }
}

/// The install and removal scripts run for real on a live Windows host, which the stand-in SSH
/// server cannot do: it runs no PowerShell. Ignored unless asked for; see `HOST_VARIABLE`.
#[cfg(test)]
mod live_windows {
    use super::super::connect::shell_of;
    use super::super::device_key;
    use super::super::ssh::{self, ConnectError, Secret, Target};
    use super::*;

    /// The host to sign in to; the test checks nothing where it is not set.
    const HOST_VARIABLE: &str = "OCTOBOARD_TEST_WINDOWS_SSH_HOST";
    /// The SSH port, 22 where it is not set.
    const PORT_VARIABLE: &str = "OCTOBOARD_TEST_WINDOWS_SSH_PORT";
    const USER_VARIABLE: &str = "OCTOBOARD_TEST_WINDOWS_SSH_USER";
    /// A private key file without a passphrase that the user's login accepts. No password is ever
    /// read: one would end up in a shell history or a log.
    const KEY_FILE_VARIABLE: &str = "OCTOBOARD_TEST_WINDOWS_SSH_KEY_FILE";
    /// The host key's fingerprint; where it is not set, the key the host shows is taken and its
    /// fingerprint printed.
    const FINGERPRINT_VARIABLE: &str = "OCTOBOARD_TEST_WINDOWS_SSH_FINGERPRINT";

    /// The administrators' group and the system account, the only ones the OpenSSH server lets
    /// use an administrator's keys file.
    const ADMINISTRATORS_SID: &str = "S-1-5-32-544";
    const SYSTEM_SID: &str = "S-1-5-18";

    fn variable(name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|value| !value.is_empty())
    }

    /// Says that `what` held, one line per check.
    fn passed(what: &str) {
        println!("ok    {what}");
    }

    /// Fails the run with `what` unless `held`, and says it held otherwise.
    fn check(held: bool, what: &str) -> Result<(), String> {
        if !held {
            return Err(format!("FAILED {what}"));
        }
        passed(what);
        Ok(())
    }

    /// Where `expected` and `found` first differ, with the bytes of both around there in hex.
    fn difference(expected: &[u8], found: &[u8]) -> String {
        let at = expected
            .iter()
            .zip(found)
            .position(|(expected, found)| expected != found)
            .unwrap_or(expected.len().min(found.len()));
        let from = at.saturating_sub(16);
        let window = |bytes: &[u8]| {
            let shown = &bytes[from.min(bytes.len())..(at + 32).min(bytes.len())];
            shown
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        format!(
            "first difference at byte {at} (expected {} bytes, found {}); from byte {from}:\n  expected: {}\n  found:    {}",
            expected.len(),
            found.len(),
            window(expected),
            window(found)
        )
    }

    /// Fails the run with `what` unless `file` holds exactly `expected`, saying where the bytes
    /// first differ and showing both around there; says it held otherwise.
    async fn check_bytes(
        session: &Session,
        file: &str,
        expected: &[u8],
        what: &str,
    ) -> Result<(), String> {
        let Some(found) = bytes_of(session, file).await? else {
            return Err(format!("FAILED {what}\nthere is no such file"));
        };
        if found != expected {
            return Err(format!("FAILED {what}\n{}", difference(expected, &found)));
        }
        passed(what);
        Ok(())
    }

    #[test]
    fn a_failed_byte_check_says_where_the_bytes_first_differ() {
        assert_eq!(
            difference(b"abcd", b"abXd"),
            "first difference at byte 2 (expected 4 bytes, found 4); from byte 0:\n  expected: 61 62 63 64\n  found:    61 62 58 64"
        );
        // One that is the start of the other differs where the shorter ends.
        assert!(difference(b"ab", b"abc")
            .starts_with("first difference at byte 2 (expected 2 bytes, found 3)"));
        // Far into a long file, the window begins sixteen bytes before the difference.
        let long = vec![b'x'; 100];
        let mut other = long.clone();
        other[50] = b'y';
        assert!(difference(&long, &other).contains("from byte 34:"));
    }

    /// Runs the PowerShell `body` on the host, between the scripts' own head and tail, and gives
    /// what it left in `$out`, carried as base64 so that no code page of the host's touches it.
    /// A `body` never passes `powershell_value` to a command's parameter: PowerShell reads a bare
    /// expression there as text, so the value is put in a variable first.
    async fn host(session: &Session, body: &str) -> Result<String, String> {
        let script = format!(
            "$out = ''\n{body}[Console]::Out.Write('utf8:' + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes([string]$out)))\n"
        );
        let output = session
            .run(&windows_command(&script))
            .await
            .map_err(|err| err.to_string())?;
        if output.status != Some(0) {
            return Err(format!(
                "a script of the test ended with {:?}: {}\n{body}",
                output.status,
                host_said(&output.stderr)
            ));
        }
        Ok(host_said(&output.stdout))
    }

    /// The bytes of `file` on the host; `None` where there is no such file.
    async fn bytes_of(session: &Session, file: &str) -> Result<Option<Vec<u8>>, String> {
        use base64::Engine as _;
        let read = host(
            session,
            &format!(
                "$p = {}\nif (Test-Path -LiteralPath $p) {{ $out = '+' + [Convert]::ToBase64String([IO.File]::ReadAllBytes($p)) }} else {{ $out = '-' }}\n",
                powershell_value(file)
            ),
        )
        .await?;
        match read.strip_prefix('+') {
            Some(encoded) => base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map(Some)
                .map_err(|err| format!("the host's answer is not base64: {err}")),
            None => Ok(None),
        }
    }

    /// Writes `bytes` as the whole of `file` on the host, a new file where there is none.
    async fn write_bytes(session: &Session, file: &str, bytes: &[u8]) -> Result<(), String> {
        use base64::Engine as _;
        host(
            session,
            &format!(
                "$p = {}\n[IO.File]::WriteAllBytes($p, [Convert]::FromBase64String('{}'))\n",
                powershell_value(file),
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ),
        )
        .await
        .map(|_| ())
    }

    async fn delete(session: &Session, file: &str) -> Result<(), String> {
        host(
            session,
            &format!(
                "$p = {}\nif (Test-Path -LiteralPath $p) {{ Remove-Item -LiteralPath $p -Force }}\n",
                powershell_value(file)
            ),
        )
        .await
        .map(|_| ())
    }

    /// The whole security descriptor of `file`, as text: owner, group and every access rule, so
    /// that two readings that are equal mean nothing of it changed.
    async fn security_of(session: &Session, file: &str) -> Result<String, String> {
        host(
            session,
            &format!(
                "$p = {}\n$out = (Get-Acl -LiteralPath $p).Sddl\n",
                powershell_value(file)
            ),
        )
        .await
    }

    /// Who may use `file` and how, one `SID|rights|Allow or Deny|inherited` per rule, sorted.
    async fn access_of(session: &Session, file: &str) -> Result<Vec<String>, String> {
        let listed = host(
            session,
            &format!(
                "$p = {}\n$rules = @((Get-Acl -LiteralPath $p).Access | ForEach-Object {{ $_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value + '|' + [string]$_.FileSystemRights + '|' + [string]$_.AccessControlType + '|' + [string]$_.IsInherited }})\n$out = $rules -join ';'\n",
                powershell_value(file)
            ),
        )
        .await?;
        let mut rules: Vec<String> = listed
            .split(';')
            .filter(|rule| !rule.is_empty())
            .map(str::to_owned)
            .collect();
        rules.sort();
        Ok(rules)
    }

    /// The names of what the folder of `file` holds, sorted.
    async fn beside(session: &Session, file: &str) -> Result<Vec<String>, String> {
        let folder = file.rsplit_once('\\').map_or(file, |(folder, _)| folder);
        let listed = host(
            session,
            &format!(
                "$d = {}\n$out = @(Get-ChildItem -LiteralPath $d -Force | ForEach-Object {{ $_.Name }}) -join ';'\n",
                powershell_value(folder)
            ),
        )
        .await?;
        let mut names: Vec<String> = listed
            .split(';')
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        names.sort();
        Ok(names)
    }

    /// Signs in to the host the variables name; `None`, having said why, where they name none.
    async fn sign_in() -> Result<Option<Session>, String> {
        let (Some(host), Some(user), Some(key_file)) = (
            variable(HOST_VARIABLE),
            variable(USER_VARIABLE),
            variable(KEY_FILE_VARIABLE),
        ) else {
            println!(
                "skipped: set {HOST_VARIABLE}, {USER_VARIABLE} and {KEY_FILE_VARIABLE} (a key file without a passphrase that the user's login accepts) to run this against a live Windows OpenSSH host"
            );
            return Ok(None);
        };
        let port = match variable(PORT_VARIABLE) {
            Some(port) => port
                .parse()
                .map_err(|err| format!("{PORT_VARIABLE} is not a port: {err}"))?,
            None => 22,
        };
        let mut target = Target {
            host,
            port,
            user,
            secret: Secret::Key {
                path: ssh::expand_home(&key_file),
                passphrase: None,
            },
            pinned: variable(FINGERPRINT_VARIABLE),
        };
        let session = match ssh::connect(&target).await {
            // No fingerprint was given: the key the host shows is taken, and said, since this is
            // a machine the tester started for the test.
            Err(ConnectError::HostKeyUnknown { fingerprint }) => {
                println!(
                    "note  no {FINGERPRINT_VARIABLE} given: taking the host key {fingerprint} as shown"
                );
                target.pinned = Some(fingerprint);
                ssh::connect(&target).await
            }
            other => other,
        };
        // Every command it runs, the application's own included, waits as long as a test does.
        session
            .map(|session| Some(session.patient()))
            .map_err(|err| format!("cannot sign in to the host: {err:?}"))
    }

    /// Makes `change` on the scratch `file`, with the application's own script for it.
    async fn change_at(
        session: &Session,
        shell: Shell,
        file: &str,
        change: KeyChange<'_>,
    ) -> Result<(), ChangeError> {
        run_change(
            session,
            &change_command_at(shell, change, Some(file)),
            change,
        )
        .await
    }

    /// Every check, on the scratch `file` and beside it; fails at the first that does not hold.
    async fn checks(session: &Session, shell: Shell, file: &str) -> Result<(), String> {
        let administrator = host(session, "$out = $administrator\n").await? == "True";
        println!(
            "note  signed in as {}; the SSH server's shell is {shell:?}",
            if administrator {
                "an administrator"
            } else {
                "a standard user"
            }
        );

        // The file the owner is told of is the one that user's login reads.
        let asked = authorized_keys_file(session, shell).await?;
        let expected = if administrator {
            host(
                session,
                "$out = Join-Path $env:ProgramData 'ssh\\administrators_authorized_keys'\n",
            )
            .await?
        } else {
            host(session, "$out = Join-Path $HOME '.ssh\\authorized_keys'\n").await?
        };
        check(
            asked.eq_ignore_ascii_case(&expected) && asked.contains(":\\"),
            &format!("the file question answers the full path of this user's keys file ({asked})"),
        )?;

        // A key of the test's own, marked with a name that holds every character PowerShell could
        // read as its own, and text beyond ASCII and Latin-1.
        let key = device_key::generate()?;
        let body = device_key::public_body(&key)?;
        let mark = "Octoboard on Ann\u{2019}s \u{2018}PC\u{201A}\u{201B}' \"; $(calc) `n $body {BODY} %PATH% \u{540D}\u{524D} caf\u{e9}";
        let line = device_key::authorized_line(&key, mark)?;
        let add = KeyChange::Add {
            line: &line,
            body: &body,
        };
        let remove = KeyChange::Remove { body: &body };
        let ours = format!("{line}\r\n").into_bytes();

        // Add where there is no file.
        check(
            bytes_of(session, file).await?.is_none(),
            "the scratch file does not exist yet",
        )?;
        change_at(session, shell, file, add)
            .await
            .map_err(|err| format!("FAILED adding to a missing file: {err}"))?;
        check_bytes(session, file, &ours, "Add makes a missing file that holds the line alone, the device's name intact as UTF-8 with no byte order mark, ended by CRLF").await?;
        let made = access_of(session, file).await?;
        if administrator {
            let mut required = vec![
                format!("{ADMINISTRATORS_SID}|FullControl|Allow|False"),
                format!("{SYSTEM_SID}|FullControl|Allow|False"),
            ];
            required.sort();
            check(
                made == required,
                &format!("a file made for an administrator may be used by Administrators and SYSTEM alone, nothing inherited ({})", made.join("; ")),
            )?;
        } else {
            println!(
                "note  a file made for a standard user keeps the access rights of its folder ({})",
                made.join("; ")
            );
        }
        change_at(session, shell, file, add)
            .await
            .map_err(|err| format!("FAILED adding a second time: {err}"))?;
        check_bytes(
            session,
            file,
            &ours,
            "Add where the key is already there appends nothing",
        )
        .await?;

        // Add where there is a file: other lines, the last with no line ending, and access rights
        // of its own, which a file written afresh in the scratch folder has.
        delete(session, file).await?;
        let others = b"ssh-rsa AAAAother laptop\r\nssh-ed25519 AAAAlast phone".to_vec();
        write_bytes(session, file, &others).await?;
        let before = security_of(session, file).await?;
        change_at(session, shell, file, add)
            .await
            .map_err(|err| format!("FAILED adding to an existing file: {err}"))?;
        check_bytes(session, file, &[&others[..], b"\r\n", &ours[..]].concat(), "Add to an existing file ends its last line and appends the line, every byte before it as it was").await?;
        check(
            security_of(session, file).await? == before,
            "Add leaves an existing file's owner and access rights as they were",
        )?;

        // Remove from a file of lines that are not all text: LF and CRLF endings, bytes that are
        // not UTF-8, ours in the middle, and a last line with no line ending.
        let first =
            b"ssh-rsa AAAAlf laptop\nssh-rsa AAAAcrlf desk\r\nssh-rsa AAAAbytes caf\xe9 \xff\xfe\n"
                .to_vec();
        let last = b"ssh-ed25519 AAAAlast phone".to_vec();
        for (what, start) in [
            ("a file of LF and CRLF lines, a line that is not UTF-8 and a last line with no ending", &b""[..]),
            ("the same file starting with a UTF-8 byte order mark", &b"\xef\xbb\xbf"[..]),
        ] {
            let held = [start, &first[..], &ours[..], &last[..]].concat();
            write_bytes(session, file, &held).await?;
            let before = security_of(session, file).await?;
            change_at(session, shell, file, remove).await.map_err(|err| format!("FAILED removing from {what}: {err}"))?;
            check_bytes(session, file, &[start, &first[..], &last[..]].concat(), &format!("Remove takes exactly the line out of {what}, every other byte identical")).await?;
            check(
                security_of(session, file).await? == before,
                &format!("Remove leaves the owner and access rights of {what} as they were"),
            )?;
        }

        // Add where the file starts with a byte order mark and holds a line that is not UTF-8:
        // the key already there is found behind them, and a line added leaves them alone.
        let marked = [&b"\xef\xbb\xbf"[..], &first[..]].concat();
        write_bytes(session, file, &[&marked[..], &ours[..]].concat()).await?;
        change_at(session, shell, file, add).await.map_err(|err| {
            format!("FAILED adding where the key is behind a byte order mark: {err}")
        })?;
        check_bytes(
            session,
            file,
            &[&marked[..], &ours[..]].concat(),
            "Add finds the key already in a file that starts with a byte order mark and holds a line that is not UTF-8, and appends nothing",
        )
        .await?;
        write_bytes(session, file, &marked).await?;
        change_at(session, shell, file, add)
            .await
            .map_err(|err| format!("FAILED adding to a file with a byte order mark: {err}"))?;
        check_bytes(
            session,
            file,
            &[&marked[..], &ours[..]].concat(),
            "Add to such a file appends the line, the byte order mark and every byte before it as they were",
        )
        .await?;

        // Add where the file is written as UTF-16, as Windows PowerShell's `>` writes one: the
        // server reads no key from it, and a UTF-8 line appended would make it neither.
        let wide: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain(
                "ssh-rsa AAAAother laptop\r\n"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes),
            )
            .collect();
        write_bytes(session, file, &wide).await?;
        let refused = change_at(session, shell, file, add).await;
        check(
            matches!(&refused, Err(ChangeError::Failed(why)) if why.contains("UTF-16")),
            &format!("Add to a file written as UTF-16 is refused, saying why ({refused:?})"),
        )?;
        check_bytes(session, file, &wide, "and leaves that file as it was").await?;

        // Ours last, with no line ending of its own: the line before it keeps its own.
        write_bytes(session, file, &[&first[..], line.as_bytes()].concat()).await?;
        change_at(session, shell, file, remove)
            .await
            .map_err(|err| format!("FAILED removing a last line with no ending: {err}"))?;
        check_bytes(
            session,
            file,
            &first,
            "Remove takes out the line where it is the last and has no line ending",
        )
        .await?;

        // Ours the only line: the file stays, empty.
        write_bytes(session, file, &ours).await?;
        let before = security_of(session, file).await?;
        change_at(session, shell, file, remove)
            .await
            .map_err(|err| format!("FAILED removing the only line: {err}"))?;
        check_bytes(
            session,
            file,
            b"",
            "Remove leaves a file the line was the only one of in place, empty",
        )
        .await?;
        check(
            security_of(session, file).await? == before,
            "Remove leaves the emptied file's owner and access rights as they were",
        )?;
        let left = beside(session, file).await?;
        check(
            left == ["authorized_keys"],
            &format!(
                "the removals left no temporary or backup file beside the file ({})",
                left.join("; ")
            ),
        )?;

        // The line deleted by hand, or kept in another file: there is nothing to remove here, and
        // that is never told as a removal.
        write_bytes(session, file, &first).await?;
        let answered = change_at(session, shell, file, remove).await;
        check(
            answered == Err(ChangeError::NotFound),
            &format!("Remove where the file does not hold the line answers that it was not found, neither success nor a failure (it answered {answered:?})"),
        )?;
        check_bytes(session, file, &first, "and leaves that file as it was").await?;
        delete(session, file).await?;
        let answered = change_at(session, shell, file, remove).await;
        check(
            answered == Err(ChangeError::NotFound),
            &format!("Remove where there is no file answers that it was not found, neither success nor a failure (it answered {answered:?})"),
        )?;
        check(
            bytes_of(session, file).await?.is_none(),
            "and makes no file",
        )?;
        Ok(())
    }

    /// Against a real Windows host with OpenSSH Server, named by `HOST_VARIABLE`, `USER_VARIABLE`
    /// and `KEY_FILE_VARIABLE`. Every script it runs on a keys file is the application's own but
    /// for the one line that names a scratch file in the user's temporary folder
    /// (`change_command_at`), so the host's real authorized keys are only ever asked about, never
    /// written; the scratch folder is removed whatever the outcome. Run with
    /// `--ignored --nocapture`, once per shell the SSH server may run and per kind of user.
    #[tokio::test]
    #[ignore = "needs a live Windows OpenSSH host, see OCTOBOARD_TEST_WINDOWS_SSH_HOST"]
    async fn a_live_windows_host_keeps_its_authorized_keys_byte_for_byte() {
        let Some(session) = sign_in().await.expect("the sign-in") else {
            return;
        };
        let outcome = async {
            let shell = shell_of(&session).await.map_err(|err| err.to_string())?;
            if shell == Shell::Posix {
                return Err("the host answers as a POSIX shell: this is for a Windows host".to_owned());
            }
            let folder = host(
                &session,
                "$d = Join-Path ([IO.Path]::GetTempPath()) ('octoboard-keys-test-' + [Guid]::NewGuid().ToString('N'))\nNew-Item -ItemType Directory -Path $d | Out-Null\n$out = $d\n",
            )
            .await?;
            println!("note  working in {folder}; the host's real keys file is not written");
            let checked = checks(&session, shell, &format!("{folder}\\authorized_keys")).await;
            // Whatever the checks came to, nothing of the test stays on the host.
            let removed = host(
                &session,
                &format!(
                    "$d = {}\nRemove-Item -LiteralPath $d -Recurse -Force\n",
                    powershell_value(&folder)
                ),
            )
            .await
            .map(|_| ())
            .map_err(|err| format!("the scratch folder {folder} could not be removed: {err}"));
            checked.and(removed)
        }
        .await;
        session.disconnect().await;
        if let Err(failed) = outcome {
            panic!("{failed}");
        }
        println!("ok    every check held, and the scratch folder is gone");
    }
}
