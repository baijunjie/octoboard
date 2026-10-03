//! `octoboardd hook` — what the per-session hook script execs. It reads the agent's hook payload
//! from stdin and posts it to the daemon.
//!
//! Its only hard requirement is to be invisible when anything goes wrong: every one of the three
//! agents renders a failing hook to the user (an error block on Claude Code, a scrollback line on
//! Grok, a history cell on Codex), so this exits 0 unconditionally and writes nothing to stdout or
//! stderr. Stdout silence also matters for correctness, not just cosmetics: a `PreToolUse` hook on
//! Grok that emits JSON can deny a tool call, and on Claude Code stdout can steer the turn.
//!
//! The HTTP request is hand-rolled rather than done with a client crate: it only ever goes to
//! `127.0.0.1`, so a TLS stack and URL normalisation would be weight with no purpose, and the IDNA
//! chain a general client pulls in raises the Rust toolchain floor past this project's.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Hard deadline for the whole round trip. Comfortably inside the hook timeouts the adapters set,
/// so a daemon that is gone or wedged costs the turn a fraction of a second rather than the hook's
/// full timeout.
const TIMEOUT: Duration = Duration::from_millis(1500);

/// Runs the hook and returns. Never fails: the caller exits 0 regardless.
pub fn run(session: &str, port: u16) {
    let mut payload = Vec::new();
    if std::io::stdin().read_to_end(&mut payload).is_err() {
        return;
    }
    let _ = post(session, port, &payload);
}

fn post(session: &str, port: u16, payload: &[u8]) -> std::io::Result<()> {
    let authority = format!("127.0.0.1:{port}");
    let address = authority.parse().map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "unusable daemon address")
    })?;
    let mut stream = TcpStream::connect_timeout(&address, TIMEOUT)?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    stream.set_read_timeout(Some(TIMEOUT))?;

    let request = format!(
        "POST /hook/{session} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    stream.write_all(request.as_bytes())?;
    stream.write_all(payload)?;
    stream.flush()?;

    // The response is read but not inspected: the daemon never steers the agent through it, and
    // reading it keeps the connection from being closed before the request was consumed.
    let mut response = [0u8; 64];
    let _ = stream.read(&mut response);
    Ok(())
}
