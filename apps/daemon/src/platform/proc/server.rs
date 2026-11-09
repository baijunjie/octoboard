//! A server the caller starts as a plain child process, with no terminal: leading a session of its
//! own ([`Reach::Session`]: on Windows a job of its own nested in the launch's, with no console
//! window), inside a launch's containment when given and in the crash table, its standard input
//! closed and its output either read for what it reports or discarded. The caller waits for it to
//! show it is ready and ends it with a polite request, a grace and a kill of what it left. What it
//! serves, and how its readiness shows, are the caller's.

use std::io::{self, BufRead, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::{crash, stop_group, ChildHandle, Containment, Polite, ProcessId, Reach};

/// How often a server is looked at while it is waited for.
const READY_POLL: Duration = Duration::from_millis(20);

/// How a started server shows it is ready.
#[derive(Debug, Clone, Copy)]
pub enum Readiness<'a> {
    /// The path exists: a socket, or a link to one, that the server makes once it listens.
    PathExists(&'a Path),
    /// A line of the server's output holds a non-zero port number right after this text.
    PortAfter(&'a str),
}

/// A running server, ended when dropped (at once, without a grace) unless [`ServerChild::end`]
/// ended it already.
pub struct ServerChild {
    child: Child,
    handle: ChildHandle,
    crash: Option<crash::Entry>,
    /// The lines the server prints, while they are read for its readiness.
    lines: Option<mpsc::Receiver<String>>,
    /// What the server is called in the log.
    label: String,
    ended: bool,
}

impl ServerChild {
    /// Starts `command` as a server in `containment`, its output piped and read when
    /// `read_output` (for a port it reports) and discarded otherwise. `label` names it in the log.
    /// The command's own standard streams and process creation flags are overridden.
    pub fn start(
        mut command: Command,
        read_output: bool,
        containment: Option<&Containment>,
        label: &str,
    ) -> io::Result<Self> {
        let output = || {
            if read_output {
                Stdio::piped()
            } else {
                Stdio::null()
            }
        };
        command
            .stdin(Stdio::null())
            .stdout(output())
            .stderr(output());
        Reach::Session.lead(&mut command);
        let mut child = command.spawn()?;
        let handle = match ChildHandle::started(ProcessId(child.id()), Reach::Session, containment)
        {
            Ok(handle) => handle,
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(err);
            }
        };
        let crash = crash::register(&handle, true);
        if crash.is_none() {
            tracing::warn!(
                process = label,
                "the crash cleanup is full; this server would outlive a crash"
            );
        }
        let lines = output_lines(&mut child);
        Ok(Self {
            child,
            handle,
            crash,
            lines,
            label: label.to_string(),
            ended: false,
        })
    }

    pub fn pid(&self) -> ProcessId {
        self.handle.pid()
    }

    /// Whether the server has exited, found out without reaping it; true once it was ended.
    pub fn exited(&self) -> bool {
        self.ended || self.handle.exited_unreaped() == Some(true)
    }

    /// Waits up to `within` for the server to show it is ready as `readiness` says, and returns
    /// the port it reported, for [`Readiness::PortAfter`]. Fails when the server exits first, when
    /// the time passes, and for a port asked of a server whose output is not read. What it prints
    /// once it is ready is drained and dropped. **Blocks.**
    pub fn wait_ready(
        &mut self,
        readiness: Readiness<'_>,
        within: Duration,
    ) -> io::Result<Option<u16>> {
        let deadline = Instant::now() + within;
        loop {
            if self.exited() {
                return Err(io::Error::other("the server exited before it was ready"));
            }
            match readiness {
                Readiness::PathExists(path) => {
                    if path.exists() {
                        return Ok(None);
                    }
                }
                Readiness::PortAfter(prefix) => {
                    let lines = self
                        .lines
                        .as_ref()
                        .ok_or_else(|| io::Error::other("the server's output is not read"))?;
                    let mut found = None;
                    while let Ok(line) = lines.try_recv() {
                        if let Some(port) = port_after(&line, prefix) {
                            found = Some(port);
                            break;
                        }
                    }
                    if let Some(port) = found {
                        // From now on the readers only drain the pipes.
                        self.lines = None;
                        return Ok(Some(port));
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the server was not ready in time",
                ));
            }
            std::thread::sleep(READY_POLL);
        }
    }

    /// Ends the server: a polite request to its process group, up to `grace` for it to exit, then
    /// a kill of it and of what it left in its session, and its reap. A host with no polite request
    /// to send kills it at once. Done once; a later call does nothing. **Blocks** for up to
    /// `grace` and the reap.
    pub fn end(&mut self, grace: Duration) {
        if self.ended {
            return;
        }
        self.ended = true;
        let pid = self.handle.pid();
        // The leader is kept unreaped until its session has been swept, so its id cannot be reused.
        if self.handle.exited_unreaped() == Some(false)
            && stop_group(pid, Polite::Terminate).delivered()
        {
            let deadline = Instant::now() + grace;
            while self.handle.exited_unreaped() == Some(false) && Instant::now() < deadline {
                std::thread::sleep(READY_POLL);
            }
        }
        if self.handle.exited_unreaped().is_some() {
            self.handle.kill();
            self.handle.kill_leftovers(&[]).log(pid, &self.label);
        }
        self.crash.take();
        let _ = self.child.wait();
    }
}

impl Drop for ServerChild {
    fn drop(&mut self) {
        self.end(Duration::ZERO);
    }
}

/// The non-zero port number right after `prefix` in `line`, if there is one.
pub fn port_after(line: &str, prefix: &str) -> Option<u16> {
    let (_, rest) = line.split_once(prefix)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|port| *port != 0)
}

/// The lines the server prints on stdout and stderr, where they were piped. The pipes are read to
/// their end, so a server that goes on printing never blocks on them; a line that is not UTF-8 is
/// read as best it can be.
fn output_lines(child: &mut Child) -> Option<mpsc::Receiver<String>> {
    let (sender, lines) = mpsc::channel();
    let readers: Vec<Box<dyn Read + Send>> = [
        child
            .stdout
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .collect();
    if readers.is_empty() {
        return None;
    }
    for reader in readers {
        let sender = sender.clone();
        std::thread::spawn(move || read_lines(reader, &sender));
    }
    Some(lines)
}

/// Sends each line `reader` gives to `lines` until it ends. Once nobody listens, what follows is
/// only drained.
fn read_lines(reader: impl Read, lines: &mpsc::Sender<String>) {
    let mut reader = io::BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                let _ = lines.send(String::from_utf8_lossy(&line).into_owned());
            }
        }
    }
}

/// Every line `reader` gives, as a server's output is read.
#[cfg(test)]
pub(crate) fn read_lines_for_test(reader: impl Read) -> Vec<String> {
    let (sender, lines) = mpsc::channel();
    read_lines(reader, &sender);
    drop(sender);
    lines.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PREFIX: &str = "listening on: ws://127.0.0.1:";

    #[test]
    fn the_port_right_after_the_prefix_is_read_and_a_zero_or_absent_one_is_not() {
        assert_eq!(
            port_after("  listening on: ws://127.0.0.1:4321\n", PREFIX),
            Some(4321)
        );
        assert_eq!(
            port_after("listening on: ws://127.0.0.1:80 and 90", PREFIX),
            Some(80)
        );
        assert_eq!(port_after("listening on: ws://127.0.0.1:0\n", PREFIX), None);
        assert_eq!(port_after("listening on: ws://127.0.0.1:\n", PREFIX), None);
        assert_eq!(port_after("something else 8080\n", PREFIX), None);
        assert_eq!(port_after("p:99999", "p:"), None, "not a port number");
    }

    #[cfg(unix)]
    mod process {
        use super::super::*;
        use crate::test_support::{
            assert_stops_running, is_running, wait_for_pid, ScratchDir, PATIENCE,
        };

        const PREFIX: &str = "listening on: ws://127.0.0.1:";

        fn sh(script: &str) -> Command {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", script]);
            command
        }

        #[test]
        fn a_server_is_ready_once_its_path_exists_and_ending_it_reaps_it() {
            let dir = ScratchDir::new("server-child-path");
            let path = dir.join("socket");
            let script = format!("touch '{}'; exec sleep 60", path.display());
            let mut server = ServerChild::start(sh(&script), false, None, "test server").unwrap();
            let pid = server.pid().0 as i32;
            let ready = server.wait_ready(Readiness::PathExists(&path), PATIENCE);
            assert_eq!(ready.unwrap(), None);
            assert!(!server.exited());
            server.end(Duration::from_secs(2));
            assert!(server.exited());
            assert!(!is_running(pid));
            // Ended once: a later end, and the drop, do nothing more.
            server.end(Duration::ZERO);
        }

        #[test]
        fn the_port_a_server_prints_after_the_prefix_is_reported_when_it_is_ready() {
            let script =
                format!("echo starting; echo '{PREFIX}0'; echo '{PREFIX}4321'; exec sleep 60");
            let mut server = ServerChild::start(sh(&script), true, None, "test server").unwrap();
            let ready = server.wait_ready(Readiness::PortAfter(PREFIX), PATIENCE);
            assert_eq!(ready.unwrap(), Some(4321));
            assert!(!server.exited());
            server.end(Duration::from_secs(2));
        }

        #[test]
        fn a_server_that_exits_before_it_is_ready_fails_the_wait() {
            let dir = ScratchDir::new("server-child-exits");
            let mut server = ServerChild::start(sh("exit 3"), false, None, "test server").unwrap();
            let never = dir.join("never");
            let error = server
                .wait_ready(Readiness::PathExists(&never), PATIENCE)
                .unwrap_err();
            assert!(
                error.to_string().contains("exited before it was ready"),
                "{error}"
            );
            assert!(server.exited());
        }

        #[test]
        fn a_server_that_is_not_ready_in_time_fails_the_wait_and_is_still_running() {
            let dir = ScratchDir::new("server-child-late");
            let mut server =
                ServerChild::start(sh("exec sleep 60"), false, None, "test server").unwrap();
            let never = dir.join("never");
            let error = server
                .wait_ready(Readiness::PathExists(&never), Duration::from_millis(300))
                .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            assert!(!server.exited());
            server.end(Duration::from_secs(2));
            assert!(server.exited());
        }

        #[test]
        fn a_port_cannot_be_asked_of_a_server_whose_output_is_not_read() {
            let mut server =
                ServerChild::start(sh("exec sleep 60"), false, None, "test server").unwrap();
            let error = server
                .wait_ready(Readiness::PortAfter(PREFIX), PATIENCE)
                .unwrap_err();
            assert!(error.to_string().contains("not read"), "{error}");
            server.end(Duration::from_secs(2));
        }

        #[test]
        fn ending_a_server_ends_what_it_started_in_its_session_too() {
            let dir = ScratchDir::new("server-child-leftovers");
            let file = dir.join("leftover");
            let script = format!(
                "sleep 60 & echo $! > '{file}.tmp'; mv '{file}.tmp' '{file}'; exec sleep 60",
                file = file.display()
            );
            let mut server = ServerChild::start(sh(&script), false, None, "test server").unwrap();
            let leftover = wait_for_pid(&file);
            server.end(Duration::from_secs(2));
            assert_stops_running(leftover);
        }
    }
}
