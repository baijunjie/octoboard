use std::ffi::OsStr;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::{split_path, Arg, Dialect, Environment, ResolvedBinary, ShellCommand, Shim, Stderr};
use crate::platform::registry::{read_values, Hive, Value};

/// The registry key that holds the machine's environment variables.
const MACHINE_ENVIRONMENT: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
/// The registry key that holds the user's.
const USER_ENVIRONMENT: &str = "Environment";

/// What `PATHEXT` is when the environment names none.
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

/// How long `<binary> --version` may take.
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// The user's shell: PowerShell 7 (`pwsh.exe`), else Windows PowerShell, else `cmd.exe`, the first
/// of them on the `PATH`.
pub fn login_shell() -> ShellCommand {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let program = ["pwsh.exe", "powershell.exe", "cmd.exe"]
        .into_iter()
        .find_map(|name| find_file(&path, name))
        .unwrap_or_else(|| PathBuf::from("cmd.exe"));
    ShellCommand {
        program,
        args: Vec::new(),
    }
}

/// PowerShell 7 (`pwsh.exe`), else Windows PowerShell, the first on the `PATH`.
fn powershell() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    ["pwsh.exe", "powershell.exe"]
        .into_iter()
        .find_map(|name| find_file(&path, name))
}

/// The language of the shell that runs a hook command an agent registers by the path of a script
/// ([`script_command`](super::script_command)): PowerShell, which is what Codex starts for it.
pub fn hook_dialect() -> Dialect {
    Dialect::PowerShell
}

/// How long an agent lets a command hook run before it kills it, in seconds.
/// A hook's first start of a launch is cold here: Codex runs it through `pwsh -NoProfile -Command
/// "& '...\hook.cmd'"` (its own choice of command line, so `-NoLogo -NonInteractive` cannot be
/// added, and the registered text is already only the script call), and Claude's through Git
/// Bash, each scanned before it runs. Measured at b187b2aad under a non-ASCII home, the pwsh path
/// took 4.6 s cold and 0.6 to 1.3 s warm, so the 3 s of the warm hosts killed the first hook of a
/// launch (`SessionStart`) and lost it. 15 s is over three times the cold start. Raising it costs
/// nothing in the common case: Codex's and Claude's command hooks are registered `async`, so the
/// timeout bounds the hook process alone and the agent's turn does not wait for it; a hook that
/// never answers is merely killed later.
pub const COMMAND_HOOK_TIMEOUT_SECS: u32 = 15;

/// How many times as long as a later one an agent's first hook of a launch may take to report: the
/// hook starts its shell and the daemon's executable cold, each scanned before it runs and, on an
/// ARM64 machine, perhaps emulated. Claude Code's first `UserPromptSubmit` of a launch reported
/// 4 s after its Enter on an ARM64 machine, the later ones within a second.
pub const FIRST_HOOK_SLOWDOWN: u32 = 3;

/// The environment a program started from the desktop has: what this process inherited from the
/// user's session, with the variables the registry holds laid over it as Windows builds a new
/// session's. There is no shell to run, so nothing waits on `deadline`.
pub fn snapshot(_deadline: Duration) -> anyhow::Result<Environment> {
    let inherited = std::env::vars()
        .filter(|(name, _)| !name.is_empty() && !name.starts_with('='))
        .collect();
    Ok(merged_environment(
        inherited,
        &read_values(Hive::LocalMachine, MACHINE_ENVIRONMENT),
        &read_values(Hive::CurrentUser, USER_ENVIRONMENT),
    ))
}

/// `base` with the machine's variables laid over it, then the user's, which override them; `PATH`
/// is the machine's and the user's joined, in that order. A `REG_EXPAND_SZ` value has the variables
/// it names between percent signs replaced by their values as of when it is applied, so a user's
/// value can name a machine's; a name nothing defines stays as written.
fn merged_environment(
    base: Environment,
    machine: &[(String, Value)],
    user: &[(String, Value)],
) -> Environment {
    let mut env = base;
    let mut paths = Vec::new();
    for (name, value) in machine.iter().chain(user) {
        let text = if value.expandable {
            expand(&value.text, &env)
        } else {
            value.text.clone()
        };
        if name.eq_ignore_ascii_case("PATH") {
            paths.push(text);
        } else if !name.is_empty() {
            set(&mut env, name, text);
        }
    }
    if !paths.is_empty() {
        let joined = paths
            .iter()
            .map(|path| path.trim_end_matches(';'))
            .filter(|path| !path.is_empty())
            .collect::<Vec<_>>()
            .join(";");
        set(&mut env, "PATH", joined);
    }
    env
}

/// Sets `name`, which Windows matches without regard to case, keeping the spelling already there.
fn set(env: &mut Environment, name: &str, value: String) {
    let existing = env
        .keys()
        .find(|key| key.eq_ignore_ascii_case(name))
        .cloned();
    env.insert(existing.unwrap_or_else(|| name.to_owned()), value);
}

fn get<'a>(env: &'a Environment, name: &str) -> Option<&'a String> {
    env.iter()
        .find_map(|(key, value)| key.eq_ignore_ascii_case(name).then_some(value))
}

/// `text` with each `%NAME%` that `env` defines replaced by its value.
fn expand(text: &str, env: &Environment) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => match get(env, &after[..end]) {
                Some(value) => {
                    out.push_str(value);
                    rest = &after[end + 1..];
                }
                None => {
                    out.push('%');
                    rest = after;
                }
            },
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The extensions `env` makes a program of, in the order they are tried, each with its dot.
fn path_extensions(env: &Environment) -> Vec<String> {
    let listed = get(env, "PATHEXT").map_or(DEFAULT_PATHEXT, String::as_str);
    listed
        .split(';')
        .filter(|extension| extension.starts_with('.') && extension.len() > 1)
        .map(str::to_owned)
        .collect()
}

/// The file `name` names in the first absolute directory of `path` that has it, as a command
/// prompt finds it: a name that ends in one of `extensions` as it is, then with each of them added
/// in turn. A directory that is not absolute stands for whatever directory the process is in.
fn find_with_extensions(path: &OsStr, name: &str, extensions: &[String]) -> Option<PathBuf> {
    let lower = name.to_ascii_lowercase();
    let has_extension = extensions
        .iter()
        .any(|extension| lower.ends_with(&extension.to_ascii_lowercase()));
    split_path(path)
        .into_iter()
        .filter(|dir| dir.is_absolute())
        .find_map(|dir| {
            let as_named = has_extension.then(|| dir.join(name));
            let with_extension = extensions
                .iter()
                .map(|extension| dir.join(format!("{name}{extension}")));
            as_named
                .into_iter()
                .chain(with_extension)
                .find(|candidate| is_program_file(candidate))
        })
}

/// The file called exactly `name` in the first absolute directory of `path` that has it.
fn find_file(path: &OsStr, name: &str) -> Option<PathBuf> {
    split_path(path)
        .into_iter()
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|candidate| is_program_file(candidate))
}

/// Whether `path` is a file, not a directory. A link is judged by its own entry: an app execution
/// alias (`wt.exe` in `WindowsApps`) is a reparse point that opening cannot follow.
fn is_program_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_dir())
}

/// Resolves a bare binary name to the file `env`'s `PATH` and `PATHEXT` find for it. A `.cmd` or
/// `.bat` hit is the program itself: the standard library starts a batch file through `cmd /d /c`
/// and quotes its arguments for it, which a caller's own leading arguments could not.
pub fn resolve_binary(name: &str, env: &Environment) -> Option<ResolvedBinary> {
    find_with_extensions(OsStr::new(get(env, "PATH")?), name, &path_extensions(env))
        .map(ResolvedBinary::new)
}

/// The architectures (as Rust names them: `x86_64`, `aarch64`) of the programs this machine runs,
/// its native one first: an ARM64 machine runs x64 programs too, emulated, whatever the daemon
/// itself was built for. Only the daemon's own where the machine cannot be asked.
pub fn runnable_architectures() -> Vec<&'static str> {
    use windows_sys::Win32::System::SystemInformation::{
        IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, IsWow64Process2};
    let (mut process, mut native): (IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE) = (0, 0);
    // SAFETY: the pseudo handle of the current process is valid, and both outputs are written.
    if unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, &mut native) } == 0 {
        return vec![std::env::consts::ARCH];
    }
    match native {
        IMAGE_FILE_MACHINE_AMD64 => vec!["x86_64"],
        IMAGE_FILE_MACHINE_ARM64 => vec!["aarch64", "x86_64"],
        _ => vec![std::env::consts::ARCH],
    }
}

/// What `binary` is to the package `package` (`@scope/name`): a package manager's shim of it
/// (npm's, pnpm's, Yarn's, Bun's or Volta's), with the directory the package is installed at, a
/// script shim whose package was found nowhere, or no shim (see `package_shims`).
pub fn shim_package(binary: &ResolvedBinary, package: &str) -> Shim {
    super::package_shims::shim_of(&binary.program, package)
}

/// The shell that runs a command string, the string to follow its arguments: PowerShell for what
/// is written for it; for what is written for `sh`, Git for Windows' bash, which is what the
/// agents run their own commands with there too, and `cmd` where Git is not installed.
pub fn command_runner(dialect: Dialect) -> ShellCommand {
    if dialect == Dialect::PowerShell {
        if let Some(program) = powershell() {
            return ShellCommand {
                program,
                args: vec!["-NoProfile".into(), "-Command".into()],
            };
        }
    }
    if let Some(bash) = git_bash() {
        return ShellCommand {
            program: bash,
            args: vec!["-c".into()],
        };
    }
    ShellCommand {
        program: std::env::var_os("ComSpec")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("cmd.exe")),
        args: vec!["/d".into(), "/s".into(), "/c".into()],
    }
}

/// Git for Windows' `bash.exe`: in its default install location, else beside the `git.exe` on the
/// `PATH`. Not a `bash.exe` found on the `PATH`, which is the WSL launcher in `System32`.
fn git_bash() -> Option<PathBuf> {
    let mut installed = ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|root| PathBuf::from(root).join(r"Git\bin\bash.exe"))
        .chain(
            std::env::var_os("LOCALAPPDATA")
                .map(|root| PathBuf::from(root).join(r"Programs\Git\bin\bash.exe")),
        );
    installed.find(|bash| bash.is_file()).or_else(|| {
        let git = find_file(&std::env::var_os("PATH")?, "git.exe")?;
        git.ancestors()
            .skip(1)
            .take(3)
            .map(|root| root.join(r"bin\bash.exe"))
            .find(|bash| bash.is_file())
    })
}

/// Writes next to `path`, with the extension `.cmd`, a batch file that runs `exe` with `args`, and
/// returns its path to register as the command: Windows runs a batch file by its extension. The
/// batch file's own directory is private through the data directory's access rules, since it may
/// carry a key.
///
/// A value is quoted for the program's own command line and for the batch file's parsing: a
/// percent sign is doubled, and a quotation mark, which neither can carry inside a quoted word,
/// or a line break is refused. A path or value outside ASCII is held by a batch file only once
/// the console code page is UTF-8.
pub fn write_runnable(
    path: &Path,
    exe: &Path,
    args: &[Arg<'_>],
    stderr: Stderr,
) -> io::Result<PathBuf> {
    let mut line = batch_quote(&exe.to_string_lossy())?;
    for arg in args {
        line.push(' ');
        match arg {
            Arg::Word(word) => line.push_str(&batch_literal(word)),
            Arg::Value(value) => line.push_str(&batch_quote(value)?),
        }
    }
    if stderr == Stderr::Discard {
        line.push_str(" 2>nul");
    }
    let mut lines = vec!["@echo off".to_owned()];
    if !line.is_ascii() {
        lines.push("chcp 65001 >nul".to_owned());
    }
    lines.push(line);
    lines.push(String::new());
    let script = path.with_extension("cmd");
    std::fs::write(&script, lines.join("\r\n"))?;
    Ok(script)
}

/// `value` as one argument of a program started from a batch file: in quotation marks, its
/// trailing backslashes doubled so that they do not escape the closing one.
fn batch_quote(value: &str) -> io::Result<String> {
    if value.contains(['"', '\r', '\n', '\0']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a batch file cannot hold a quotation mark or a line break in an argument",
        ));
    }
    let trailing = value.len() - value.trim_end_matches('\\').len();
    Ok(format!(
        "\"{}{}\"",
        batch_literal(value),
        "\\".repeat(trailing)
    ))
}

/// Writes next to `path`, with the extension `.cmd`, a batch file that appends the one argument it
/// is given, as a line, to `log`, and returns its path: what a program that opens a URL by running
/// `$BROWSER <url>` is pointed at. Windows runs a batch file by its extension.
///
/// The argument is taken as the whole argument line (`%*`) with its quotes dropped, which a URL
/// never holds: `%1` ends an unquoted argument at a `=`, `;` or `,`. It goes through a variable
/// whose value is read only after delayed expansion is on, since `%*` is pasted into the line
/// before it is parsed, which a `&` in a URL would end. A log beside the script is named from the
/// script's own directory (`%~dp0`) rather than spelled out, so that a path the batch file's code
/// page cannot hold still reaches it.
pub fn write_argument_recorder(path: &Path, log: &Path) -> io::Result<PathBuf> {
    let script = path.with_extension("cmd");
    let log_name = log
        .file_name()
        .and_then(OsStr::to_str)
        .filter(|name| name.is_ascii());
    let log_target = match log_name {
        Some(name) if log.parent() == script.parent() => format!("%~dp0{}", batch_literal(name)),
        _ => batch_literal(&log.to_string_lossy()),
    };
    let body = [
        "@echo off".to_owned(),
        "setlocal DisableDelayedExpansion".to_owned(),
        format!("set \"octoboard_log={log_target}\""),
        "set octoboard_url=%*".to_owned(),
        "setlocal EnableDelayedExpansion".to_owned(),
        "if defined octoboard_url set \"octoboard_url=!octoboard_url:\"=!\"".to_owned(),
        ">>\"!octoboard_log!\" echo(!octoboard_url!".to_owned(),
        String::new(),
    ]
    .join("\r\n");
    std::fs::write(&script, body)?;
    Ok(script)
}

/// `text` as a batch file holds a literal: a percent sign is doubled.
fn batch_literal(text: &str) -> String {
    text.replace('%', "%%")
}

/// No interpreter is named: the dispatcher's shebang is left to Git for Windows' `sh`, the one
/// git itself runs a hook with.
pub fn posix_interpreter() -> Option<PathBuf> {
    None
}

/// `path` as the shell of the git hooks writes and compares a path: Git for Windows' shell and git
/// itself spell a path with forward slashes.
pub fn posix_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// What `binary --version` prints on its standard output, for a binary whose version its install
/// does not tell, run in `environment` (the launch's own). Asked of a binary once while it stays
/// the same file: the answer is kept by its path and modification time.
pub fn probe_version(binary: &ResolvedBinary, environment: &Environment) -> Option<String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use std::time::SystemTime;

    /// What each binary, as of its modification time, printed.
    type Answers = Mutex<HashMap<(PathBuf, SystemTime), Option<String>>>;
    static ANSWERS: OnceLock<Answers> = OnceLock::new();
    let modified = std::fs::metadata(&binary.program).ok()?.modified().ok()?;
    let key = (binary.program.clone(), modified);
    let answers = ANSWERS.get_or_init(Default::default);
    if let Some(answer) = answers.lock().ok()?.get(&key) {
        return answer.clone();
    }
    let answer = run_version(binary, environment);
    answers.lock().ok()?.insert(key, answer.clone());
    answer
}

/// Runs the probe leading a session of its own, so that what it starts — the `node.exe` behind an
/// npm `.cmd` shim, which holds the output pipe too — is ended with it rather than left holding
/// the pipe, and the reader on it, for good.
fn run_version(binary: &ResolvedBinary, environment: &Environment) -> Option<String> {
    use crate::platform::proc::{lead_session, ChildHandle, ProcessId, Reach};

    let mut command = binary.command();
    command
        .arg("--version")
        .env_clear()
        .envs(environment)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    lead_session(&mut command);
    let mut child = command.spawn().ok()?;
    let handle = match ChildHandle::started(ProcessId(child.id()), Reach::Session, None) {
        Ok(handle) => handle,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };
    let Some(mut stdout) = child.stdout.take() else {
        handle.kill();
        handle.kill_leftovers(&[]);
        let _ = child.wait();
        return None;
    };
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = (&mut stdout).take(4096).read_to_end(&mut text);
        let _ = sender.send(text);
    });
    let deadline = Instant::now() + VERSION_TIMEOUT;
    let text = receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok();
    // A binary that printed and exited is reaped; one that did not is ended rather than left, and
    // so is whatever either left on its console.
    if text.is_none() {
        handle.kill();
    }
    handle.kill_leftovers(&[]);
    let _ = child.wait();
    Some(String::from_utf8_lossy(&text?).into_owned()).filter(|text| !text.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use std::os::windows::process::CommandExt;

    use super::*;
    use crate::test_support::ScratchDir;

    fn value(text: &str, expandable: bool) -> Value {
        Value {
            text: text.to_owned(),
            expandable,
        }
    }

    fn pair(name: &str, text: &str, expandable: bool) -> (String, Value) {
        (name.to_owned(), value(text, expandable))
    }

    #[test]
    fn the_registry_is_merged_by_windows_rules() {
        let base = Environment::from([
            ("Path".to_owned(), r"C:\stale".to_owned()),
            ("SystemRoot".to_owned(), r"C:\Windows".to_owned()),
            ("KEPT".to_owned(), "inherited".to_owned()),
        ]);
        let machine = [
            pair("Path", r"%SystemRoot%\system32;C:\machine;", true),
            pair("Shared", "machine", false),
            pair("MACHINE_ONLY", "m", false),
        ];
        let user = [
            pair("PATH", r"C:\Users\me\bin", true),
            pair("shared", "user", false),
            pair("Mixed", r"%MACHINE_ONLY%-%systemroot%-%UNDEFINED%", true),
            pair("Literal", "%SystemRoot%", false),
        ];
        let env = merged_environment(base, &machine, &user);
        assert_eq!(
            env.get("Path").map(String::as_str),
            Some(r"C:\Windows\system32;C:\machine;C:\Users\me\bin")
        );
        assert_eq!(
            env.keys()
                .filter(|key| key.eq_ignore_ascii_case("path"))
                .count(),
            1
        );
        assert_eq!(env.get("Shared").map(String::as_str), Some("user"));
        assert_eq!(
            env.get("Mixed").map(String::as_str),
            Some(r"m-C:\Windows-%UNDEFINED%")
        );
        assert_eq!(env.get("Literal").map(String::as_str), Some("%SystemRoot%"));
        assert_eq!(env.get("KEPT").map(String::as_str), Some("inherited"));
        assert_eq!(env.get("MACHINE_ONLY").map(String::as_str), Some("m"));
    }

    #[test]
    fn the_snapshot_reads_this_machines_registry() {
        let env = snapshot(Duration::from_secs(1)).unwrap();
        let path = get(&env, "PATH").expect("a PATH");
        assert!(
            path.to_ascii_lowercase().contains("system32"),
            "the machine's PATH is in it: {path}"
        );
        assert!(get(&env, "PATHEXT").is_some_and(|ext| ext.to_ascii_uppercase().contains(".CMD")));
        assert!(env.keys().all(|name| !name.starts_with('=')));
    }

    #[test]
    fn a_binary_is_found_by_pathext_order_and_a_shim_runs_with_its_arguments_intact() {
        let dir = ScratchDir::new("resolve-shim");
        let bin = dir.join("bin dir");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("tool.cmd"), "@echo off\r\necho %*\r\n").unwrap();
        std::fs::write(bin.join("tool.exe"), "").unwrap();
        std::fs::write(bin.join("only.bat"), "@echo off\r\necho bat %1\r\n").unwrap();
        let env = |pathext: &str| {
            Environment::from([
                ("PATH".to_owned(), format!(r"relative;{}", bin.display())),
                ("PathExt".to_owned(), pathext.to_owned()),
            ])
        };

        // File names are compared without regard to case, as the file system does.
        let found = |name: &str, pathext: &str| {
            resolve_binary(name, &env(pathext))
                .map(|binary| binary.program.to_string_lossy().to_lowercase())
        };
        let lower = |path: PathBuf| Some(path.to_string_lossy().to_lowercase());
        assert_eq!(found("tool", ".EXE;.CMD"), lower(bin.join("tool.exe")));
        assert_eq!(found("tool", ".CMD;.EXE"), lower(bin.join("tool.cmd")));
        assert_eq!(found("tool.cmd", ".EXE"), None);
        assert_eq!(found("tool.cmd", ".EXE;.CMD"), lower(bin.join("tool.cmd")));
        assert_eq!(found("TOOL", ".cmd"), lower(bin.join("tool.cmd")));
        assert_eq!(found("missing", ".EXE;.CMD"), None);
        assert_eq!(resolve_binary("tool", &Environment::new()), None);

        let shim = resolve_binary("tool", &env(".CMD")).unwrap();
        let output = shim
            .command()
            .args(["plain", "two words", "a&b", "c|d"])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            r#"plain "two words" "a&b" "c|d""#
        );
        let bat = resolve_binary("only", &env(".BAT")).unwrap();
        let output = bat.command().arg("x y").output().unwrap();
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            r#"bat "x y""#
        );
    }

    #[test]
    fn the_login_shell_is_a_shell_found_on_the_path() {
        let shell = login_shell();
        assert!(shell.program.is_file(), "{shell:?}");
        let name = shell
            .program
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_lowercase();
        assert!(["pwsh.exe", "powershell.exe", "cmd.exe"].contains(&name.as_str()));
    }

    #[test]
    fn the_command_runner_runs_a_posix_command_string() {
        let runner = command_runner(Dialect::Posix);
        let output = std::process::Command::new(&runner.program)
            .args(&runner.args)
            .arg("echo 'two words' && exit 3")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(3));
        assert!(String::from_utf8_lossy(&output.stdout).contains("two words"));
    }

    #[test]
    fn the_argument_recorder_appends_its_argument_whole() {
        let dir = ScratchDir::new("argument-recorder");
        let home = dir.join("it's é 白 here");
        std::fs::create_dir_all(&home).unwrap();
        let (script, log) = (home.join("record.sh"), home.join("urls"));
        let script = write_argument_recorder(&script, &log).unwrap();
        assert_eq!(script, home.join("record.cmd"));
        let urls = [
            "https://a/?x=1&y=2",
            "https://b/?q=%20%41&s=100%",
            "https://c/#a^b!c|d<e>f(g)",
        ];
        for url in urls {
            // Whole line in one more pair of quotes, which `/s` strips, as a program that runs a
            // batch file with quoted arguments does.
            let status = std::process::Command::new("cmd")
                .args(["/d", "/s", "/c"])
                .raw_arg(format!("\"\"{}\" \"{url}\"\"", script.display()))
                .status()
                .unwrap();
            assert!(status.success());
        }
        // Unquoted, as a program quoting only what holds a space passes it.
        let bare = "https://e/?a=1;b=2,c=3";
        let status = std::process::Command::new("cmd")
            .args(["/d", "/s", "/c"])
            .raw_arg(format!("\"\"{}\" {bare}\"", script.display()))
            .status()
            .unwrap();
        assert!(status.success());
        let recorded = std::fs::read_to_string(&log).unwrap();
        assert_eq!(
            recorded.lines().collect::<Vec<_>>(),
            [urls.as_slice(), &[bare]].concat()
        );

        // A log elsewhere is named as it is.
        let elsewhere = dir.join("elsewhere 100%");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let log = elsewhere.join("urls");
        let script = write_argument_recorder(&home.join("other"), &log).unwrap();
        let status = std::process::Command::new("cmd")
            .args(["/d", "/s", "/c"])
            .raw_arg(format!(
                "\"\"{}\" \"https://d/?a=1&b=2\"\"",
                script.display()
            ))
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(
            std::fs::read_to_string(&log).unwrap().trim(),
            "https://d/?a=1&b=2"
        );
    }

    #[test]
    fn a_path_is_spelled_with_forward_slashes_for_the_hooks_shell() {
        assert_eq!(
            posix_path(Path::new(r"C:\Users\it's\.octoboard\githooks")),
            "C:/Users/it's/.octoboard/githooks"
        );
    }

    #[test]
    fn a_runnable_is_a_batch_file_that_runs_its_command_line_with_values_whole() {
        let dir = ScratchDir::new("write-runnable");
        let home = dir.join("it's é 白 here");
        std::fs::create_dir_all(&home).unwrap();
        let cmd = home.join("octo cmd.exe");
        std::fs::copy(r"C:\Windows\System32\cmd.exe", &cmd).unwrap();
        let written = write_runnable(
            &home.join("hook"),
            &cmd,
            &[
                Arg::Word("/d"),
                Arg::Word("/c"),
                Arg::Word("echo"),
                Arg::Value("a b&c|d 100%"),
                Arg::Value(r"end\"),
                Arg::Word("5%"),
            ],
            Stderr::Discard,
        )
        .unwrap();
        assert_eq!(written, home.join("hook.cmd"));
        let output = std::process::Command::new(&written)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            r#""a b&c|d 100%" "end\\" 5%"#
        );

        for value in ["a\"b", "a\nb"] {
            let refused = write_runnable(
                &home.join("refused"),
                &cmd,
                &[Arg::Value(value)],
                Stderr::Keep,
            );
            assert_eq!(
                refused.unwrap_err().kind(),
                io::ErrorKind::InvalidInput,
                "{value:?}"
            );
        }
    }

    #[test]
    fn a_binarys_version_is_what_it_prints() {
        let dir = ScratchDir::new("probe-version");
        let shim = dir.join("agent.cmd");
        std::fs::write(&shim, "@echo off\r\necho agent-cli 1.2.3\r\n").unwrap();
        let environment: Environment = std::env::vars().collect();
        let printed = probe_version(&ResolvedBinary::new(&shim), &environment);
        assert_eq!(printed.as_deref().map(str::trim), Some("agent-cli 1.2.3"));
        assert_eq!(
            probe_version(&ResolvedBinary::new(dir.join("missing.exe")), &environment),
            None
        );
    }
}
