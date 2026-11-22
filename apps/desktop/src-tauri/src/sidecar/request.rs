//! The shell's HTTP requests to its own daemon on the loopback port, with the launch key: asking
//! it to stop (`stop.rs`) and the clean-up's routes (`crate::system_changes::daemon`). Written out
//! over a socket rather than through a client the rest of the shell has no use for.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use super::DaemonAddress;

/// A step of a request, each given a time of its own ([`post`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Connect,
    Send,
    Read,
}

/// How much of the answer a request reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    /// The status line alone: the request is answered once the daemon says whether it took it.
    StatusLine,
    /// Everything up to the end of the connection, the body included.
    Whole,
}

/// What the daemon answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// The status line, up to its line end; empty where the daemon answered nothing.
    pub status_line: String,
    /// The body; empty where only the status line was read.
    pub body: String,
}

impl Answer {
    /// The status code, where the status line has one.
    pub fn status(&self) -> Option<u16> {
        self.status_line.split_whitespace().nth(1)?.parse().ok()
    }

    /// Whether the status is a success (2xx).
    pub fn succeeded(&self) -> bool {
        self.status()
            .is_some_and(|status| (200..300).contains(&status))
    }

    /// The answer as `response`, a whole HTTP response, reads.
    fn of(response: &str) -> Self {
        let (head, body) = response.split_once("\r\n\r\n").unwrap_or((response, ""));
        Self {
            status_line: head.lines().next().unwrap_or("").trim().to_owned(),
            body: body.to_owned(),
        }
    }
}

/// `POST`s to the daemon at `address` on `path` with its launch key, `json` as the body where
/// there is one (else none), and reads as much of the answer as `reply` says. `wait` is asked for
/// each step's time in turn, the connect's before connecting, then the read's and the send's once
/// connected, before anything is sent: a caller with one deadline for the whole request gives each
/// what is left of it at that point, so the read is given no less than the send. A time of zero,
/// which a socket refuses, is taken as a moment.
pub fn post(
    address: &DaemonAddress,
    path: &str,
    json: Option<&str>,
    reply: Reply,
    wait: impl Fn(Step) -> Duration,
) -> std::io::Result<Answer> {
    let wait = |step| wait(step).max(Duration::from_millis(1));
    let target = SocketAddr::from((Ipv4Addr::LOCALHOST, address.port));
    let mut stream = TcpStream::connect_timeout(&target, wait(Step::Connect))?;
    stream.set_read_timeout(Some(wait(Step::Read)))?;
    stream.set_write_timeout(Some(wait(Step::Send)))?;
    let body = match json {
        Some(json) => format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
            json.len()
        ),
        None => "Content-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
    };
    write!(
        stream,
        "POST {path}?key={} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{body}",
        address.key, address.port
    )?;
    match reply {
        Reply::StatusLine => {
            let mut status_line = String::new();
            BufReader::new(stream).read_line(&mut status_line)?;
            Ok(Answer {
                status_line: status_line.trim().to_owned(),
                body: String::new(),
            })
        }
        Reply::Whole => {
            let mut response = String::new();
            stream.read_to_string(&mut response)?;
            Ok(Answer::of(&response))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A daemon that answers one request with `response` and hands back the request it read.
    fn answer_once(response: &'static str) -> (u16, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut request = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(value) = line.strip_prefix("Content-Length: ") {
                    length = value.trim().parse().unwrap();
                }
                request.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).unwrap();
            request.push_str(&String::from_utf8(body).unwrap());
            reader.get_mut().write_all(response.as_bytes()).unwrap();
            request
        });
        (port, served)
    }

    fn address(port: u16) -> DaemonAddress {
        DaemonAddress {
            port,
            key: "ab12".to_owned(),
        }
    }

    const LONG: Duration = Duration::from_secs(30);

    /// A request with a body says it is JSON and how long, one without says it has none, and
    /// both carry the launch key to the daemon's port and ask for the connection to be closed.
    #[test]
    fn a_request_carries_the_key_and_its_body_to_the_daemons_port() {
        let (port, served) = answer_once("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{\"a\":1}");
        let answer = post(&address(port), "/x", Some("{}"), Reply::Whole, |_| LONG).unwrap();
        assert_eq!(answer.body, "{\"a\":1}");
        assert_eq!(answer.status(), Some(200));
        assert_eq!(
            served.join().unwrap(),
            format!(
                "POST /x?key=ab12 HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: \
                 application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
            )
        );

        let (port, served) = answer_once("HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n");
        let answer = post(&address(port), "/y", None, Reply::StatusLine, |_| LONG).unwrap();
        assert_eq!(answer.status_line, "HTTP/1.1 202 Accepted");
        assert!(answer.succeeded() && answer.body.is_empty());
        assert_eq!(
            served.join().unwrap(),
            format!(
                "POST /y?key=ab12 HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\n\
                 Connection: close\r\n\r\n"
            )
        );
    }

    /// Only a 2xx status succeeds, and an answer of nothing has no status.
    #[test]
    fn only_a_success_status_succeeds() {
        assert!(Answer::of("HTTP/1.1 200 OK\r\n\r\n").succeeded());
        let refused = Answer::of("HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\n\r\n");
        assert_eq!(refused.status_line, "HTTP/1.1 401 Unauthorized");
        assert!(!refused.succeeded());
        let nothing = Answer::of("");
        assert_eq!((nothing.status(), nothing.status_line.as_str()), (None, ""));
    }

    /// Each step is given the time asked for it, in the order connect, read, send: a read that
    /// nothing answers ends at the read's time, not the connect's.
    #[test]
    fn each_step_gets_its_own_time() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let asked = std::sync::Mutex::new(Vec::new());
        let started = std::time::Instant::now();
        let read = post(&address(port), "/x", None, Reply::StatusLine, |step| {
            asked.lock().unwrap().push(step);
            match step {
                Step::Read => Duration::from_millis(200),
                _ => LONG,
            }
        });
        assert!(read.is_err());
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(
            *asked.lock().unwrap(),
            [Step::Connect, Step::Read, Step::Send]
        );
        drop(listener);
    }
}
