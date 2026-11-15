//! `connection`: prints the connection details another device sets itself up from
//! ([`crate::connection_details`]) as one line of text and as a QR code of that same line, the host
//! being the one given, else the address the SSH session running the command reached this machine
//! at, else the machine's own name.

use std::io::IsTerminal;
use std::path::Path;

use anyhow::Result;

use crate::connection_details::{
    of_this_machine_at, qr_modules, text_of, Details, DEFAULT_SSH_PORT,
};
use crate::platform::fs;

/// The address and port this machine was reached at by the SSH session running the command, from
/// `SSH_CONNECTION`'s value (`<client> <client port> <server> <server port>`).
fn reached_at(ssh_connection: &str) -> Option<(String, u16)> {
    let fields: Vec<&str> = ssh_connection.split_whitespace().collect();
    let [_, _, host, port] = fields[..] else {
        return None;
    };
    Some((host.to_owned(), port.parse().ok().filter(|port| *port > 0)?))
}

/// Where the host in the details came from, for the note that says how to correct it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostSource {
    Given,
    SshSession,
    MachineName,
}

/// The host and port to put in the details, and where the host came from: `host` and `port` as
/// the owner gave them, else the address the SSH session `ssh_connection` reached this machine at,
/// else the machine's own `machine_name` at the default SSH port. `None` when there is nothing to
/// go by.
fn chosen_host(
    host: Option<String>,
    port: Option<u16>,
    ssh_connection: Option<&str>,
    machine_name: Option<&str>,
) -> Option<(String, u16, HostSource)> {
    let session = ssh_connection.and_then(reached_at);
    if let Some(host) = host
        .map(|host| host.trim().to_owned())
        .filter(|host| !host.is_empty())
    {
        let port = port
            .or(session.map(|(_, port)| port))
            .unwrap_or(DEFAULT_SSH_PORT);
        return Some((host, port, HostSource::Given));
    }
    if let Some((host, session_port)) = session {
        return Some((host, port.unwrap_or(session_port), HostSource::SshSession));
    }
    let name = machine_name?.to_owned();
    Some((
        name,
        port.unwrap_or(DEFAULT_SSH_PORT),
        HostSource::MachineName,
    ))
}

/// The QR code of `text`, as lines of terminal text, two rows of modules to a line, with a quiet
/// zone of two modules around it. Dark modules are drawn with block characters; with `color`, every
/// line is set to black on bright white, so that the code reads the same on a dark terminal.
fn qr_lines(text: &str, color: bool) -> Result<Vec<String>> {
    const QUIET: usize = 2;
    let modules = qr_modules(text)?;
    let width = modules.len();
    let size = width + 2 * QUIET;
    let dark = |x: usize, y: usize| {
        x >= QUIET && y >= QUIET && x < QUIET + width && y < QUIET + width && {
            modules[y - QUIET][x - QUIET]
        }
    };
    let mut lines = Vec::with_capacity(size.div_ceil(2));
    for y in (0..size).step_by(2) {
        let mut line = String::with_capacity(size * 3 + 16);
        if color {
            line.push_str("\x1b[30;107m");
        }
        for x in 0..size {
            line.push(match (dark(x, y), dark(x, y + 1)) {
                (false, false) => ' ',
                (true, false) => '▀',
                (false, true) => '▄',
                (true, true) => '█',
            });
        }
        if color {
            line.push_str("\x1b[0m");
        }
        lines.push(line);
    }
    Ok(lines)
}

/// Prints the connection details on stdout, the QR code first unless `text_only`, then the text,
/// and on stderr where the host came from and how to correct it. `host` and `ssh_port` replace
/// the host and port the command would choose.
pub(super) fn print_details(
    host: Option<String>,
    ssh_port: Option<u16>,
    text_only: bool,
) -> Result<i32> {
    let machine_name = fs::host_name().and_then(text_of);
    let ssh_connection = std::env::var("SSH_CONNECTION").ok();
    let Some((host, port, source)) = chosen_host(
        host,
        ssh_port,
        ssh_connection.as_deref(),
        machine_name.as_deref(),
    ) else {
        eprintln!(
            "this machine's name could not be read: give the host other devices reach it at with \
             `octoboardd connection --host <host>`"
        );
        return Ok(1);
    };
    let name = machine_name.filter(|name| *name != host);
    let octoboardd = super::launch_path().ok();
    let Some(details) = of_this_machine_at(host, port, name, octoboardd.as_deref()) else {
        eprintln!("the name of the user this command runs as could not be read");
        return Ok(1);
    };
    let key_dir = fs::ssh_host_key_dir();
    let text = details.to_text();
    if !text_only {
        for line in qr_lines(&text, std::io::stdout().is_terminal())? {
            println!("{line}");
        }
        println!();
    }
    println!("{text}");
    for note in notes(&details, source, &key_dir) {
        eprintln!("{note}");
    }
    Ok(0)
}

/// What `connection` says on stderr beside the details: where the host came from and how to
/// correct it, and what a missing host key or path means.
fn notes(details: &Details, source: HostSource, key_dir: &Path) -> Vec<String> {
    let correct = "if other devices reach this machine at another name or address, give it with \
                   `octoboardd connection --host <host>` (and `--ssh-port <port>`)";
    let mut notes = Vec::new();
    match source {
        HostSource::Given => {}
        HostSource::SshSession => notes.push(format!(
            "the host is {}, the address this SSH session reached the machine at; {correct}",
            details.host
        )),
        HostSource::MachineName => notes.push(format!(
            "the host is {}, the name this machine gives itself, which other devices may not know; \
             {correct}",
            details.host
        )),
    }
    if details.fingerprint.is_none() {
        notes.push(format!(
            "no SSH host key could be read in {}: the application shows the host key at the first \
             connection for you to confirm",
            key_dir.display()
        ));
    }
    if details.octoboardd.is_none() {
        notes.push(
            "the path of this octoboardd could not be told: the application looks in the places \
             Octoboard installs to"
                .to_owned(),
        );
    }
    notes.push(
        "paste the text into the Connection details field when adding a remote daemon in \
         Octoboard; the QR code holds the same text"
            .to_owned(),
    );
    notes
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
    fn the_host_is_the_one_given_else_the_ssh_sessions_else_the_machines_name() {
        let session = Some("192.0.2.9 51000 198.51.100.7 2200");
        assert_eq!(
            chosen_host(Some(" box.example ".into()), None, session, Some("box")),
            Some(("box.example".into(), 2200, HostSource::Given))
        );
        assert_eq!(
            chosen_host(Some("box.example".into()), Some(22), session, Some("box")),
            Some(("box.example".into(), 22, HostSource::Given))
        );
        assert_eq!(
            chosen_host(None, None, session, Some("box")),
            Some(("198.51.100.7".into(), 2200, HostSource::SshSession))
        );
        assert_eq!(
            chosen_host(None, None, Some("garbled"), Some("box")),
            Some(("box".into(), 22, HostSource::MachineName))
        );
        assert_eq!(
            chosen_host(Some(" ".into()), Some(2022), None, Some("box")),
            Some(("box".into(), 2022, HostSource::MachineName))
        );
        assert_eq!(chosen_host(None, None, None, None), None);
    }

    #[test]
    fn the_qr_code_is_square_with_a_quiet_zone_and_colored_only_when_asked() {
        let text = details().to_text();
        let plain = qr_lines(&text, false).unwrap();
        let width = plain[0].chars().count();
        assert_eq!(plain.len(), width.div_ceil(2));
        assert!(plain.iter().all(|line| line.chars().count() == width));
        assert!(plain[0].chars().all(|c| c == ' '));
        assert!(plain.iter().all(|line| line.starts_with("  ")));
        assert!(plain.iter().all(|line| !line.contains('\x1b')));
        let colored = qr_lines(&text, true).unwrap();
        assert!(colored
            .iter()
            .all(|line| line.starts_with("\x1b[30;107m") && line.ends_with("\x1b[0m")));
    }

    #[test]
    fn the_qr_code_holds_the_text_unchanged() {
        let text = details().to_text();
        let code = qrcode::QrCode::new(text.as_bytes()).unwrap();
        let lines = qr_lines(&text, false).unwrap();
        // The lines draw exactly the code's modules, two rows to a line.
        let width = code.width();
        let colors = code.to_colors();
        for y in 0..width {
            for x in 0..width {
                let line: Vec<char> = lines[(y + 2) / 2].chars().collect();
                let c = line[x + 2];
                let top = (y + 2) % 2 == 0;
                let drawn = match c {
                    '█' => true,
                    '▀' => top,
                    '▄' => !top,
                    _ => false,
                };
                assert_eq!(drawn, colors[y * width + x] == qrcode::Color::Dark);
            }
        }
    }

    #[test]
    fn the_notes_say_where_the_host_came_from_and_what_is_missing() {
        let dir = Path::new("/etc/ssh");
        let given = notes(&details(), HostSource::Given, dir);
        assert_eq!(given.len(), 1, "{given:?}");
        let missing = Details {
            fingerprint: None,
            octoboardd: None,
            ..details()
        };
        let named = notes(&missing, HostSource::MachineName, dir);
        assert!(named[0].contains("server.example") && named[0].contains("--host"));
        assert!(named[1].contains("/etc/ssh"));
        assert!(named[2].contains("places Octoboard installs to"));
        let session = notes(&details(), HostSource::SshSession, dir);
        assert!(session[0].contains("this SSH session"));
    }

    /// The address and port the SSH session reached the machine at are the third and fourth fields
    /// of `SSH_CONNECTION`; a value that is not those four fields, or whose port is no port, says
    /// nothing.
    #[test]
    fn the_session_says_where_it_reached_the_machine_in_the_third_and_fourth_fields() {
        assert_eq!(
            reached_at("192.0.2.9 51000 198.51.100.7 2200"),
            Some(("198.51.100.7".to_owned(), 2200))
        );
        assert_eq!(
            reached_at("  2001:db8::1 51000 2001:db8::2 22 \n"),
            Some(("2001:db8::2".to_owned(), 22))
        );
        for unreadable in [
            "",
            "garbled",
            "192.0.2.9 51000 198.51.100.7",
            "192.0.2.9 51000 198.51.100.7 22 extra",
            "192.0.2.9 51000 198.51.100.7 0",
            "192.0.2.9 51000 198.51.100.7 65536",
            "192.0.2.9 51000 198.51.100.7 ssh",
        ] {
            assert_eq!(reached_at(unreadable), None, "{unreadable:?}");
        }
    }

    /// A port given with a host given and none with the session's: the session's port stays where
    /// there is one; a session that cannot be read leaves the default.
    #[test]
    fn a_given_host_keeps_the_sessions_port_and_a_given_port_wins_over_it() {
        let session = Some("192.0.2.9 51000 198.51.100.7 2200");
        assert_eq!(
            chosen_host(None, Some(2022), session, Some("box")),
            Some(("198.51.100.7".into(), 2022, HostSource::SshSession))
        );
        assert_eq!(
            chosen_host(Some("box.example".into()), None, Some("garbled"), None),
            Some(("box.example".into(), 22, HostSource::Given))
        );
        assert_eq!(
            chosen_host(
                None,
                None,
                Some("192.0.2.9 51000 198.51.100.7 0"),
                Some("box")
            ),
            Some(("box".into(), 22, HostSource::MachineName)),
            "a session whose port is no port is not used"
        );
    }
}
