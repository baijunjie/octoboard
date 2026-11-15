//! What the application runs on the remote machine, and how it reads the answers: where it looks
//! for `octoboardd`, the commands it runs there (`address --json`, `proxy` and `start`), and what
//! their exit statuses and output mean. A command run over SSH is handed to the remote user's shell
//! without a login shell's `PATH`, so the places Octoboard installs to are tried by path; the
//! commands are formed for the shell the host's SSH server runs them through, which is told apart
//! by a probe (`Shell`): a POSIX one on a Linux or macOS host, the command prompt or PowerShell on
//! a Windows one.

use serde::Deserialize;

use crate::menu::APP_NAME;

/// The exit status a POSIX shell gives a command it cannot find.
const NOT_FOUND: u32 = 127;
/// `octoboardd`'s exit status when no daemon runs.
const NO_DAEMON: u32 = 3;
/// `octoboardd`'s exit status when the running daemon has no owner endpoint.
const NO_OWNER_ENDPOINT: u32 = 4;
/// `octoboardd`'s exit status when the running daemon's owner endpoint cannot be reached.
const ENDPOINT_UNREACHABLE: u32 = 5;
/// `octoboardd`'s exit status when it is the desktop application's and the application is not
/// running.
const APPLICATION_NOT_RUNNING: u32 = 6;

/// The protocol generation the bundled UI speaks.
pub const LOCAL_GENERATION: u32 =
    match u32::from_str_radix(env!("OCTOBOARD_PROTOCOL_GENERATION"), 10) {
        Ok(generation) => generation,
        Err(_) => panic!("OCTOBOARD_PROTOCOL_GENERATION is not a number"),
    };

/// The application's release version, which a refused daemon's is named beside.
pub const LOCAL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The command a probe runs to tell the remote shell: each shell prints something else for it
/// (see `Shell::classify`).
pub const SHELL_PROBE: &str = "echo %OS% $env:OS";

/// The shell a host's SSH server runs a command through. A Linux or macOS host's is a POSIX one; a
/// Windows host's is the command prompt unless its OpenSSH server is set to PowerShell.
// `PowerShell` is the shell's own name, not a variant named after the enum.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shell {
    #[default]
    Posix,
    Cmd,
    PowerShell,
}

impl Shell {
    /// The shell that printed `output` for `SHELL_PROBE`: the command prompt expands `%OS%` and
    /// leaves `$env:OS` as it is, PowerShell the other way round, and a POSIX shell neither
    /// (anything unrecognised is taken for one, as an empty answer is).
    pub fn classify(output: &str) -> Self {
        if !output.contains("Windows_NT") {
            Self::Posix
        } else if output.contains("$env:OS") {
            Self::Cmd
        } else {
            Self::PowerShell
        }
    }

    pub fn is_windows(self) -> bool {
        self != Self::Posix
    }
}

/// Where `octoboardd` is looked for, in order, when the entry names no path, by what `shell` is:
/// on a POSIX host the remote user's `PATH` as a command run over SSH gets it, the install script's
/// directory, the conventional system-wide one, and the macOS application's own files; on a Windows
/// host the application's per-user install and its per-machine one (a Windows host has no other
/// `octoboardd` than the application's, and no `PATH` lookup is made).
pub fn candidates(shell: Shell) -> Vec<String> {
    if shell.is_windows() {
        return vec![
            format!(r"%LOCALAPPDATA%\{APP_NAME}\octoboardd.exe"),
            format!(r"%ProgramFiles%\{APP_NAME}\octoboardd.exe"),
        ];
    }
    vec![
        "octoboardd".to_owned(),
        "~/.local/bin/octoboardd".to_owned(),
        "/usr/local/bin/octoboardd".to_owned(),
        format!("/Applications/{APP_NAME}.app/Contents/MacOS/octoboardd"),
        format!("~/Applications/{APP_NAME}.app/Contents/MacOS/octoboardd"),
    ]
}

/// `path` as one word of a POSIX shell command: single-quoted, a leading `~/` left to the shell as
/// `$HOME`.
pub fn shell_word(path: &str) -> String {
    let quote = |text: &str| format!("'{}'", text.replace('\'', r"'\''"));
    match path.strip_prefix("~/") {
        Some(rest) => format!("\"$HOME\"/{}", quote(rest)),
        None => quote(path),
    }
}

/// `path` as one word of a PowerShell command: single-quoted with `'` doubled, so it is taken as
/// written; a path naming environment variables as `%NAME%` is double-quoted with them as
/// `$env:NAME`, and what else PowerShell would expand in it escaped.
fn powershell_word(path: &str) -> String {
    if !path.contains('%') {
        return format!("'{}'", path.replace('\'', "''"));
    }
    let mut word = String::from("\"");
    let mut rest = path;
    while let Some(start) = rest.find('%') {
        let (before, after) = rest.split_at(start);
        word.push_str(&powershell_escaped(before));
        match after[1..].find('%') {
            Some(end)
                if end > 0
                    && after[1..=end]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_') =>
            {
                word.push_str("$env:");
                word.push_str(&after[1..=end]);
                rest = &after[end + 2..];
            }
            _ => {
                word.push('%');
                rest = &after[1..];
            }
        }
    }
    word.push_str(&powershell_escaped(rest));
    word.push('"');
    word
}

/// `text` with what a double-quoted PowerShell string expands escaped.
fn powershell_escaped(text: &str) -> String {
    text.replace('`', "``")
        .replace('$', "`$")
        .replace('"', "`\"")
}

/// The `octoboardd` a connection found: where it is, and the shell its commands are formed for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    pub shell: Shell,
    /// The path as it was looked for: a candidate or the entry's own.
    pub path: String,
}

impl Remote {
    pub fn new(shell: Shell, path: impl Into<String>) -> Self {
        Self {
            shell,
            path: path.into(),
        }
    }

    /// The command that runs `octoboardd` with `arguments`. On Windows a missing file exits 127, as
    /// a POSIX shell exits for a command it cannot find, and the program's own status passes
    /// through; the command prompt's form does not begin with a double quote, which `cmd /c` would
    /// strip from the line's ends.
    fn command(&self, arguments: &str) -> String {
        match self.shell {
            Shell::Posix => format!("{} {arguments}", shell_word(&self.path)),
            Shell::Cmd => {
                let path = format!("\"{}\"", self.path);
                format!("if exist {path} ({path} {arguments}) else (exit {NOT_FOUND})")
            }
            Shell::PowerShell => {
                let path = powershell_word(&self.path);
                format!(
                    "if (Test-Path -LiteralPath {path}) {{ & {path} {arguments}; exit $LASTEXITCODE }} \
                     else {{ exit {NOT_FOUND} }}"
                )
            }
        }
    }

    /// The command that prints the running daemon's owner key, generation and version.
    pub fn address_command(&self) -> String {
        self.command("address --json")
    }

    /// The command that carries one connection to the daemon's owner endpoint.
    pub fn proxy_command(&self) -> String {
        self.command("proxy")
    }

    /// The command that starts the daemon in the background on the server form's port.
    pub fn start_command(&self) -> String {
        self.command("start")
    }

    /// Reads a run of `address --json` as `interpret_address` does, except that a Windows host's
    /// status 3 (no daemon) is the application not running: a Windows host has only the
    /// application's daemon, which an older build answers that way.
    pub fn read_address(&self, status: Option<u32>, stdout: &[u8], stderr: &[u8]) -> Address {
        match interpret_address(status, stdout, stderr) {
            Address::NoDaemon if self.shell.is_windows() => {
                Address::ApplicationNotRunning(String::new())
            }
            other => other,
        }
    }
}

/// The address a browser opens the daemon at through a loopback `port` that reaches its owner
/// endpoint, signed in with the owner `key`: the address `octoboardd address` prints for a browser
/// (`browser_address` in `apps/daemon/src/browser/mod.rs`), on this port.
pub fn browser_address(port: u16, key: &str) -> String {
    format!("http://127.0.0.1:{port}/?key={key}")
}

/// What `octoboardd address --json` states of the running daemon.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Answer {
    pub owner_key: String,
    pub generation: u32,
    pub daemon_version: String,
}

/// What a run of `address --json` found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Address {
    Answer(Answer),
    /// The shell found no such command: try the next place.
    NotFound,
    NoDaemon,
    /// The `octoboardd` is the desktop application's, and the application is not running.
    ApplicationNotRunning(String),
    NoOwnerEndpoint(String),
    EndpointUnreachable(String),
    Failed(String),
}

/// Reads a run of `address --json` from its exit status (`None` when the remote side gave none) and
/// its output.
pub fn interpret_address(status: Option<u32>, stdout: &[u8], stderr: &[u8]) -> Address {
    let reason = || without_keys(String::from_utf8_lossy(stderr).trim());
    match status {
        Some(0) => {
            let line = String::from_utf8_lossy(stdout);
            match serde_json::from_str::<Answer>(line.trim()) {
                Ok(answer)
                    if !answer.owner_key.is_empty()
                        && answer.owner_key.bytes().all(|b| b.is_ascii_hexdigit()) =>
                {
                    Address::Answer(answer)
                }
                Ok(_) => Address::Failed("the daemon stated an unusable owner key".into()),
                // Named without anything it holds: the line may carry the owner key (the JSON
                // itself, or a browser's address from a daemon that does not know `--json`), and a
                // parse error repeats string values; the detail reaches the page.
                Err(_) => Address::Failed(
                    "the daemon's answer could not be read; it may be older than this application"
                        .into(),
                ),
            }
        }
        Some(NOT_FOUND) => Address::NotFound,
        Some(NO_DAEMON) => Address::NoDaemon,
        Some(NO_OWNER_ENDPOINT) => Address::NoOwnerEndpoint(reason()),
        Some(ENDPOINT_UNREACHABLE) => Address::EndpointUnreachable(reason()),
        Some(APPLICATION_NOT_RUNNING) => Address::ApplicationNotRunning(reason()),
        Some(status) => Address::Failed(match reason() {
            reason if reason.is_empty() => format!("exited with status {status}"),
            reason => reason,
        }),
        None => Address::Failed(match reason() {
            reason if reason.is_empty() => "ended without an exit status".into(),
            reason => reason,
        }),
    }
}

/// The shortest run of hexadecimal digits taken for a key: the owner key is 64, and anything this
/// long is no word a message needs.
const KEY_SHAPED: usize = 16;

/// `text` with every run of `KEY_SHAPED` or more hexadecimal digits replaced by `[key]`: what the
/// remote side printed may name the owner key (a foreground daemon prints it with its address), and
/// a failure's detail is handed to the page, which is never given a secret.
pub fn without_keys(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.len() >= KEY_SHAPED {
            out.push_str("[key]");
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in text.chars() {
        if c.is_ascii_hexdigit() {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// A daemon of another protocol generation than the bundled UI's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationMismatch {
    pub remote_generation: u32,
    pub local_generation: u32,
    pub remote_version: String,
    pub local_version: String,
}

/// Refuses a daemon whose generation is not `local`'s.
pub fn check_generation(answer: &Answer, local: u32) -> Result<(), GenerationMismatch> {
    if answer.generation == local {
        return Ok(());
    }
    Err(GenerationMismatch {
        remote_generation: answer.generation,
        local_generation: local,
        remote_version: answer.daemon_version.clone(),
        local_version: LOCAL_VERSION.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_install_places_are_tried_after_the_path_and_the_home_is_left_to_the_shell() {
        let candidates = candidates(Shell::Posix);
        assert_eq!(candidates[0], "octoboardd");
        assert_eq!(candidates[1], "~/.local/bin/octoboardd");
        assert!(candidates.contains(&format!(
            "/Applications/{APP_NAME}.app/Contents/MacOS/octoboardd"
        )));
        assert!(candidates.contains(&format!(
            "~/Applications/{APP_NAME}.app/Contents/MacOS/octoboardd"
        )));
        assert_eq!(
            shell_word("~/.local/bin/octoboardd"),
            "\"$HOME\"/'.local/bin/octoboardd'"
        );
        assert_eq!(shell_word("/opt/my dir/it's"), r"'/opt/my dir/it'\''s'");
    }

    #[test]
    fn a_windows_host_is_tried_at_the_two_install_places_alone() {
        let expected = vec![
            format!(r"%LOCALAPPDATA%\{APP_NAME}\octoboardd.exe"),
            format!(r"%ProgramFiles%\{APP_NAME}\octoboardd.exe"),
        ];
        assert_eq!(candidates(Shell::Cmd), expected);
        assert_eq!(candidates(Shell::PowerShell), expected);
    }

    #[test]
    fn the_probe_tells_the_three_shells_apart() {
        assert_eq!(Shell::classify("Windows_NT $env:OS\r\n"), Shell::Cmd);
        assert_eq!(Shell::classify("%OS%\r\nWindows_NT\r\n"), Shell::PowerShell);
        assert_eq!(Shell::classify("%OS% :OS\n"), Shell::Posix);
        assert_eq!(Shell::classify(""), Shell::Posix);
        assert!(!Shell::Posix.is_windows());
        assert!(Shell::Cmd.is_windows() && Shell::PowerShell.is_windows());
    }

    #[test]
    fn a_posix_host_gets_each_command_with_a_quoted_word() {
        let remote = Remote::new(Shell::Posix, "octoboardd");
        assert_eq!(remote.address_command(), "'octoboardd' address --json");
        assert_eq!(remote.proxy_command(), "'octoboardd' proxy");
        assert_eq!(remote.start_command(), "'octoboardd' start");
        assert_eq!(
            Remote::new(Shell::Posix, "~/a b/it's").proxy_command(),
            "\"$HOME\"/'a b/it'\\''s' proxy"
        );
    }

    #[test]
    fn the_command_prompt_gets_a_guarded_quoted_path_that_exits_127_when_missing() {
        let path = r"C:\Program Files (x86)\it's\octoboardd.exe";
        let remote = Remote::new(Shell::Cmd, path);
        assert_eq!(
            remote.address_command(),
            format!(r#"if exist "{path}" ("{path}" address --json) else (exit 127)"#)
        );
        assert_eq!(
            remote.proxy_command(),
            format!(r#"if exist "{path}" ("{path}" proxy) else (exit 127)"#)
        );
        assert_eq!(
            Remote::new(Shell::Cmd, r"%LOCALAPPDATA%\Octoboard\octoboardd.exe").start_command(),
            r#"if exist "%LOCALAPPDATA%\Octoboard\octoboardd.exe" ("%LOCALAPPDATA%\Octoboard\octoboardd.exe" start) else (exit 127)"#
        );
        assert!(remote.start_command().contains(r#"" start) else"#));
    }

    #[test]
    fn powershell_gets_a_literal_path_single_quoted_and_an_environment_one_double_quoted() {
        let remote = Remote::new(
            Shell::PowerShell,
            r"C:\Program Files (x86)\it's\octoboardd.exe",
        );
        assert_eq!(
            remote.address_command(),
            r"if (Test-Path -LiteralPath 'C:\Program Files (x86)\it''s\octoboardd.exe') { & 'C:\Program Files (x86)\it''s\octoboardd.exe' address --json; exit $LASTEXITCODE } else { exit 127 }"
        );
        let remote = Remote::new(
            Shell::PowerShell,
            r"%LOCALAPPDATA%\Octoboard\octoboardd.exe",
        );
        let word = r#""$env:LOCALAPPDATA\Octoboard\octoboardd.exe""#;
        assert_eq!(
            remote.proxy_command(),
            format!("if (Test-Path -LiteralPath {word}) {{ & {word} proxy; exit $LASTEXITCODE }} else {{ exit 127 }}")
        );
        assert_eq!(
            powershell_word(r"%ProgramFiles%\a$b`c%"),
            r#""$env:ProgramFiles\a`$b``c%""#
        );
    }

    #[test]
    fn the_application_not_running_is_status_6_and_a_windows_hosts_status_3() {
        assert_eq!(
            interpret_address(Some(6), b"", b"the application is not running\n"),
            Address::ApplicationNotRunning("the application is not running".into())
        );
        let posix = Remote::new(Shell::Posix, "octoboardd");
        assert_eq!(posix.read_address(Some(3), b"", b""), Address::NoDaemon);
        assert_eq!(
            posix.read_address(Some(6), b"", b"closed"),
            Address::ApplicationNotRunning("closed".into())
        );
        for shell in [Shell::Cmd, Shell::PowerShell] {
            let remote = Remote::new(shell, "x");
            assert_eq!(
                remote.read_address(Some(3), b"", b"not running"),
                Address::ApplicationNotRunning(String::new())
            );
            assert_eq!(remote.read_address(Some(127), b"", b""), Address::NotFound);
        }
    }

    #[test]
    fn the_browser_address_is_the_one_octoboardd_address_prints() {
        assert_eq!(
            browser_address(4100, "0a1b"),
            "http://127.0.0.1:4100/?key=0a1b"
        );
    }

    #[test]
    fn the_answer_is_one_line_of_json_with_a_hex_key() {
        let line = br#"{"owner_key":"0a1B","generation":4,"daemon_version":"1.2.3"}
"#;
        assert_eq!(
            interpret_address(Some(0), line, b""),
            Address::Answer(Answer {
                owner_key: "0a1B".into(),
                generation: 4,
                daemon_version: "1.2.3".into(),
            })
        );
        assert!(matches!(
            interpret_address(
                Some(0),
                br#"{"owner_key":"zz","generation":4,"daemon_version":"1"}"#,
                b""
            ),
            Address::Failed(_)
        ));
        assert!(matches!(
            interpret_address(Some(0), b"http://127.0.0.1:3369/?key=aa", b""),
            Address::Failed(_)
        ));
    }

    #[test]
    fn the_exit_statuses_tell_a_missing_command_no_daemon_and_no_endpoint_apart() {
        assert_eq!(
            interpret_address(Some(127), b"", b"sh: not found"),
            Address::NotFound
        );
        assert_eq!(
            interpret_address(Some(3), b"", b"not running"),
            Address::NoDaemon
        );
        assert_eq!(
            interpret_address(Some(4), b"", b"no owner endpoint\n"),
            Address::NoOwnerEndpoint("no owner endpoint".into())
        );
        assert_eq!(
            interpret_address(Some(5), b"", b"refused"),
            Address::EndpointUnreachable("refused".into())
        );
        assert_eq!(
            interpret_address(Some(2), b"", b"error: unexpected argument '--json'"),
            Address::Failed("error: unexpected argument '--json'".into())
        );
        assert_eq!(
            interpret_address(Some(1), b"", b""),
            Address::Failed("exited with status 1".into())
        );
        assert!(matches!(
            interpret_address(None, b"", b""),
            Address::Failed(_)
        ));
    }

    #[test]
    fn a_daemon_of_another_generation_is_refused_naming_both_versions() {
        let answer = |generation| Answer {
            owner_key: "aa".into(),
            generation,
            daemon_version: "9.9.9".into(),
        };
        assert_eq!(check_generation(&answer(4), 4), Ok(()));
        assert_eq!(
            check_generation(&answer(3), 4),
            Err(GenerationMismatch {
                remote_generation: 3,
                local_generation: 4,
                remote_version: "9.9.9".into(),
                local_version: LOCAL_VERSION.into(),
            })
        );
        assert!(check_generation(&answer(5), 4).is_err());
    }

    #[test]
    fn an_unreadable_answer_never_repeats_the_owner_key_it_may_hold() {
        // A failure's detail is handed to the page (`ConnectOutcome::Failed`), and no
        // remote-daemons command returns a secret to a page: whatever the daemon printed on stdout
        // may hold the owner key (the JSON line itself, or the address a browser opens with
        // `?key=`).
        let key = "0a1b2c3d4e5f60718293a4b5c6d7e8f9";
        for stdout in [
            format!(r#"{{"owner_key":"{key}","generation":"five","daemon_version":"1"}}"#),
            format!(r#"{{"owner_key":"{key}","generation":5}}"#),
            format!("http://127.0.0.1:3369/?key={key}"),
        ] {
            match interpret_address(Some(0), stdout.as_bytes(), b"") {
                Address::Failed(detail) => {
                    assert!(!detail.contains(key), "{stdout} leaked into: {detail}");
                }
                other => panic!("{stdout} was read as {other:?}"),
            }
        }
    }

    #[test]
    fn a_key_shaped_value_is_taken_out_of_what_the_remote_side_printed() {
        let key = "0a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f9";
        assert_eq!(
            without_keys(&format!(
                "octoboardd listening on 127.0.0.1:3369 with key {key}"
            )),
            "octoboardd listening on 127.0.0.1:3369 with key [key]"
        );
        assert_eq!(
            without_keys(&format!("http://127.0.0.1:3369/?key={key}&x=1")),
            "http://127.0.0.1:3369/?key=[key]&x=1"
        );
        // Short hexadecimal words, ports and versions are left alone.
        assert_eq!(
            without_keys("exit 127 at 0.9.0, face cafe"),
            "exit 127 at 0.9.0, face cafe"
        );
        assert_eq!(
            interpret_address(Some(4), b"", format!("refused {key}").as_bytes()),
            Address::NoOwnerEndpoint("refused [key]".into())
        );
    }

    #[test]
    fn an_answer_with_no_key_or_a_key_that_is_not_hex_is_refused() {
        for key in ["", "  ", "0x0a", "ab cd", "ab\ncd"] {
            let line = format!(
                r#"{{"owner_key":{},"generation":5,"daemon_version":"1"}}"#,
                serde_json::json!(key)
            );
            assert!(
                matches!(
                    interpret_address(Some(0), line.as_bytes(), b""),
                    Address::Failed(_)
                ),
                "{key:?}"
            );
        }
    }

    #[test]
    fn the_answer_is_read_whatever_surrounds_its_line() {
        let line = "\r\n  {\"owner_key\":\"ab\",\"generation\":5,\"daemon_version\":\"1\"}\r\n\r\n";
        assert!(matches!(
            interpret_address(Some(0), line.as_bytes(), b"a warning on stderr"),
            Address::Answer(_)
        ));
    }

    #[test]
    fn a_failure_is_explained_by_what_the_remote_side_said() {
        let said = |status, stderr: &[u8]| interpret_address(Some(status), b"", stderr);
        assert_eq!(
            said(1, b"  permission denied\n"),
            Address::Failed("permission denied".into())
        );
        // The remote side's own words about no daemon, no endpoint and a stopped application stay
        // as the reason; a status with nothing said is named by its number.
        assert_eq!(said(4, b""), Address::NoOwnerEndpoint(String::new()));
        assert_eq!(said(5, b""), Address::EndpointUnreachable(String::new()));
        assert_eq!(said(6, b""), Address::ApplicationNotRunning(String::new()));
        assert_eq!(
            interpret_address(None, b"", b"killed"),
            Address::Failed("killed".into())
        );
    }

    #[test]
    fn the_places_of_a_windows_host_have_no_posix_quoting_and_name_the_applications_files() {
        for shell in [Shell::Cmd, Shell::PowerShell] {
            for place in candidates(shell) {
                assert!(place.ends_with(r"\octoboardd.exe"), "{place}");
                assert!(place.contains(APP_NAME), "{place}");
            }
        }
        // A POSIX host is tried by the path the remote user's own PATH gives first, then by place.
        let posix = candidates(Shell::Posix);
        assert_eq!(posix.first().map(String::as_str), Some("octoboardd"));
        assert!(posix
            .iter()
            .skip(1)
            .all(|place| place.ends_with("/octoboardd")));
    }

    #[test]
    fn the_browser_is_sent_to_the_loopback_port_the_application_holds() {
        let address = browser_address(51234, "ab");
        assert!(address.starts_with("http://127.0.0.1:51234/"), "{address}");
        assert!(address.ends_with("?key=ab"), "{address}");
    }
}
