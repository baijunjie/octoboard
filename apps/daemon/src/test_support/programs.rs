//! The small programs a test starts when what it needs is a process that behaves a certain way,
//! not a particular tool: each is spelled with what every host has, `sh` on Unix and PowerShell
//! (`powershell -NoProfile`) on Windows. A test that needs more than these — a script that
//! composes a shell's job control, traps and redirections — is a test of a POSIX shell, and says
//! so.

use std::ffi::OsString;

/// A program and its arguments, to be started as a plain child or on a terminal.
#[derive(Debug, Clone)]
pub(crate) struct Program {
    program: OsString,
    args: Vec<OsString>,
}

impl Program {
    /// `program` run with `args`, as spelled.
    #[cfg(windows)]
    pub(crate) fn of(program: impl Into<OsString>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(OsString::from).collect(),
        }
    }

    /// The program as a child of the test process.
    pub(crate) fn command(&self) -> std::process::Command {
        let mut command = std::process::Command::new(&self.program);
        command.args(&self.args);
        command
    }

    /// The program as the process of a terminal.
    pub(crate) fn term_command(&self) -> crate::platform::term::Command {
        let mut command = crate::platform::term::Command::new(&self.program);
        command.args(&self.args);
        command
    }

    /// The program as PowerShell's `Start-Process` takes it: the program, and its arguments as one
    /// command line, quoted as the C runtime splits one (`Start-Process` joins a list of them
    /// without quoting any).
    #[cfg(windows)]
    pub(crate) fn command_line_args(&self) -> (String, String) {
        let quoted = |arg: &OsString| {
            let arg = arg.to_string_lossy();
            if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
                return arg.into_owned();
            }
            let mut quoted = String::from('"');
            let mut backslashes = 0;
            for c in arg.chars() {
                match c {
                    '\\' => backslashes += 1,
                    '"' => {
                        quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                        backslashes = 0;
                    }
                    _ => {
                        quoted.extend(std::iter::repeat_n('\\', backslashes));
                        backslashes = 0;
                    }
                }
                if c != '\\' {
                    quoted.push(c);
                }
            }
            quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
            quoted.push('"');
            quoted
        };
        let args = self.args.iter().map(quoted).collect::<Vec<_>>().join(" ");
        (self.program.to_string_lossy().into_owned(), args)
    }

    /// The program as a command of a Git Bash script, every word quoted and none of them turned
    /// from a POSIX path into a Windows one, as Git Bash otherwise does for a program of Windows'.
    #[cfg(windows)]
    pub(crate) fn posix_line(&self) -> String {
        let quoted =
            |word: &OsString| format!("'{}'", word.to_string_lossy().replace('\'', r"'\''"));
        std::iter::once("MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*'".to_string())
            .chain(std::iter::once(&self.program).chain(&self.args).map(quoted))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Waits `seconds`, then exits successfully.
pub(crate) fn sleep(seconds: u32) -> Program {
    host::sleep(seconds)
}

/// Writes `text` and a line break to its standard output, then exits successfully. `text` is
/// written as given: nothing in it is expanded.
pub(crate) fn echo(text: &str) -> Program {
    host::echo(text)
}

/// Copies its standard input to its standard output, byte for byte, until the input closes, then
/// exits successfully: a process that lives as long as its input is held open.
pub(crate) fn hold_stdin() -> Program {
    host::hold_stdin()
}

/// Ignores the hang-up of its terminal, writes `ready` and a line break once it does, then waits
/// `seconds`. On Unix the hang-up is `SIGHUP`; a Windows console process cannot refuse the close of
/// its console beyond the grace the host gives, so there it ignores Ctrl-C, the one interruption it
/// can.
pub(crate) fn ignore_hangup(seconds: u32) -> Program {
    host::ignore_hangup(seconds)
}

/// Writes the value of the environment variable `name` to its standard output, nothing where it is
/// not set, with no line break, then exits successfully. `name` is a plain identifier.
pub(crate) fn print_env(name: &str) -> Program {
    host::print_env(name)
}

/// `script` as PowerShell's `-EncodedCommand` takes it: its UTF-16 in base64, which no quoting
/// rule of a command line can change.
pub(crate) fn encoded_powershell(script: &str) -> String {
    use base64::Engine as _;
    let utf16: Vec<u8> = script
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    base64::engine::general_purpose::STANDARD.encode(utf16)
}

#[cfg(unix)]
mod host {
    use super::Program;

    fn sh(script: String) -> Program {
        Program {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), script.into(), "sh".into()],
        }
    }

    /// `text` as one word of a shell script, quoted so nothing in it is expanded.
    fn quoted(text: &str) -> String {
        format!("'{}'", text.replace('\'', r"'\''"))
    }

    pub(super) fn sleep(seconds: u32) -> Program {
        sh(format!("exec sleep {seconds}"))
    }

    pub(super) fn echo(text: &str) -> Program {
        sh(format!("printf '%s\\n' {}", quoted(text)))
    }

    pub(super) fn hold_stdin() -> Program {
        sh("exec cat".to_string())
    }

    pub(super) fn ignore_hangup(seconds: u32) -> Program {
        sh(format!("trap '' HUP; echo ready; exec sleep {seconds}"))
    }

    pub(super) fn print_env(name: &str) -> Program {
        sh(format!("printf '%s' \"${{{name}-}}\""))
    }
}

#[cfg(windows)]
mod host {
    use super::Program;

    /// `script` run by Windows PowerShell, which every Windows host has. The script travels
    /// base64-encoded (`-EncodedCommand`), so no quoting rule of the command line can change it.
    pub(super) fn powershell(script: String) -> Program {
        Program {
            program: "powershell.exe".into(),
            args: vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-EncodedCommand".into(),
                super::encoded_powershell(&script).into(),
            ],
        }
    }

    /// `text` as a PowerShell string literal, in which nothing is expanded.
    fn quoted(text: &str) -> String {
        format!("'{}'", text.replace('\'', "''"))
    }

    pub(super) fn sleep(seconds: u32) -> Program {
        powershell(format!("Start-Sleep -Seconds {seconds}"))
    }

    pub(super) fn echo(text: &str) -> Program {
        powershell(format!("[Console]::Out.Write({} + \"`n\")", quoted(text)))
    }

    pub(super) fn hold_stdin() -> Program {
        powershell(
            "$out = [Console]::OpenStandardOutput(); \
             [Console]::OpenStandardInput().CopyTo($out); $out.Flush()"
                .to_string(),
        )
    }

    pub(super) fn ignore_hangup(seconds: u32) -> Program {
        // Without a console (a plain child with redirected handles) there is no Ctrl-C to ignore.
        powershell(format!(
            "try {{ [Console]::TreatControlCAsInput = $true }} catch {{ }}; \
             [Console]::Out.Write(\"ready`n\"); Start-Sleep -Seconds {seconds}"
        ))
    }

    pub(super) fn print_env(name: &str) -> Program {
        powershell(format!(
            "[Console]::Out.Write([Environment]::GetEnvironmentVariable({}))",
            quoted(name)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stdout_of(program: Program) -> String {
        let output = program.command().output().expect("the program starts");
        assert!(output.status.success(), "{program:?}: {output:?}");
        String::from_utf8(output.stdout).expect("UTF-8 output")
    }

    #[test]
    fn each_program_does_what_it_says() {
        assert_eq!(
            stdout_of(echo("a 'b' $HOME %PATH%")),
            "a 'b' $HOME %PATH%\n"
        );
        assert_eq!(stdout_of(sleep(0)), "");

        let mut env = print_env("OCTOBOARD_TEST_PROGRAM_VALUE").command();
        env.env("OCTOBOARD_TEST_PROGRAM_VALUE", "set value");
        let output = env.output().expect("the program starts");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "set value");
        let mut unset = print_env("OCTOBOARD_TEST_PROGRAM_VALUE").command();
        unset.env_remove("OCTOBOARD_TEST_PROGRAM_VALUE");
        assert_eq!(unset.output().expect("the program starts").stdout, b"");
    }

    #[test]
    fn holding_stdin_copies_it_until_it_closes() {
        use std::io::Write as _;
        let mut child = hold_stdin()
            .command()
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("the program starts");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"held\x00\xff")
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"held\x00\xff");
    }

    #[test]
    fn a_program_ignoring_the_hang_up_says_it_is_ready() {
        let mut child = ignore_hangup(30)
            .command()
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("the program starts");
        let mut line = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(child.stdout.take().unwrap()),
            &mut line,
        )
        .unwrap();
        child.kill().ok();
        child.wait().ok();
        assert_eq!(line.trim_end(), "ready");
    }
}
