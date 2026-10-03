//! A minimal blocking HTTP/1.1 POST, just enough for `mcp_stdio` to forward `tools/call` to the
//! daemon's `/mcp/:session/:tool` endpoint. Hand-rolled instead of pulling in `reqwest` because
//! every request here stays on `127.0.0.1` — this prototype has no use for a TLS stack, URL
//! normalization (IDNA/ICU), or any of the other weight a general HTTP client carries, and the
//! IDNA crates reqwest drags in currently require a newer rustc than this machine has.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::Value;

const TIMEOUT: Duration = Duration::from_secs(10);

/// POSTs `body` as JSON to `http://<host>:<port><path>` and returns `(status_is_success, body)`.
/// `url` must be a plain `http://host:port/path` string — everything this prototype talks to.
pub fn post_json(url: &str, body: &Value) -> Result<(bool, Value)> {
    let rest = url
        .strip_prefix("http://")
        .context("only http:// URLs are supported")?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path = format!("/{path}");

    let payload = serde_json::to_vec(body)?;
    let mut stream =
        TcpStream::connect(authority).with_context(|| format!("connecting to {authority}"))?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;

    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    stream.write_all(request.as_bytes())?;
    stream.write_all(&payload)?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;

    let header_end = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .context("malformed HTTP response: no header/body separator")?;
    let header_text = String::from_utf8_lossy(&response[..header_end]);
    let status_line = header_text
        .lines()
        .next()
        .context("malformed HTTP response: no status line")?;
    let status_code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .context("malformed HTTP response: no status code")?;

    let body_bytes = &response[header_end + 4..];
    if body_bytes.is_empty() {
        if !(200..300).contains(&status_code) {
            bail!("daemon returned {status_code} with an empty body");
        }
        return Ok((true, Value::Null));
    }
    let parsed: Value =
        serde_json::from_slice(body_bytes).context("daemon response body was not JSON")?;
    Ok(((200..300).contains(&status_code), parsed))
}
