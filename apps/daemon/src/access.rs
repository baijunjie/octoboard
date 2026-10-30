//! Who may talk to the daemon: a middleware that turns away requests a browser page on another
//! site could make.
//!
//! Browsers apply no CORS check to a WebSocket handshake, and send a cross-origin "simple" POST
//! without a preflight, so any page the user opens can reach a loopback port and drive the daemon
//! through it. Two things keep such a page out, and a request needs both.
//!
//! The first is the `Origin` and `Host` rules. A browser always states the page's `Origin` on both
//! kinds of request, so one that states it has to come from a place where the daemon's own user
//! is already running something, and a request with none is left to the key. The `Host` has to be
//! one the daemon is meant to be reached by too, otherwise DNS rebinding (`evil.example` resolving
//! to 127.0.0.1) would make a hostile page's `Origin` and `Host` agree. Those rules alone still
//! admit any page served from `localhost` (a dev server running untrusted code) and every Tauri
//! application, which share `tauri://localhost`.
//!
//! The second is the key: a secret generated for each daemon launch ([`generate_key`]) that every
//! request carries as its `key` query parameter. The daemon hands it only to what it starts or
//! to the application that started it (see `apps/daemon/PROTOCOL.md`, "Who may connect"), so a
//! page that merely knows the port cannot supply it. The daemon's own arguments and environment
//! do not hold it, but its children's do (the agent CLIs' command lines, the `--key` of the
//! `octoboardd` helpers, `OCTOBOARD_DAEMON_KEY`), and so do the owner-only files of a session's
//! scratch directory. It targets web pages, not local processes: any process of the same user
//! can read it, and could drive the daemon directly anyway.

use std::io::Read;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderName, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// How many random bytes a key is made of.
const KEY_BYTES: usize = 32;

/// A fresh key: [`KEY_BYTES`] bytes from the operating system's random source, as lower-case hex.
/// The daemon is unix-only, so the source is `/dev/urandom`.
pub fn generate_key() -> std::io::Result<String> {
    let mut bytes = [0u8; KEY_BYTES];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Answers 403 — before any handler or WebSocket upgrade runs — to a request that
/// [`request_allowed`] turns away, and 401 to one that does not carry `key` (see [`key_matches`]).
pub async fn guard(State(key): State<Arc<str>>, request: Request, next: Next) -> Response {
    let origin = single_header(request.headers(), &header::ORIGIN);
    let host = single_header(request.headers(), &header::HOST);
    match (origin, host) {
        (Ok(origin), Ok(host)) if request_allowed(origin, host) => {}
        _ => return (StatusCode::FORBIDDEN, "forbidden origin or host").into_response(),
    }
    if !key_matches(request.uri().query(), &key) {
        return (StatusCode::UNAUTHORIZED, "missing or wrong key").into_response();
    }
    next.run(request).await
}

/// Whether the query string carries `key` as its first `key` parameter. The key is lower-case hex,
/// which needs no percent-decoding, and the comparison takes as long for a near miss as for a
/// miss, so the time it takes tells a caller nothing about the key.
fn key_matches(query: Option<&str>, key: &str) -> bool {
    let supplied = query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .find_map(|pair| pair.strip_prefix("key="));
    let Some(supplied) = supplied else {
        return false;
    };
    let (supplied, key) = (supplied.as_bytes(), key.as_bytes());
    // The length is not secret: every key the daemon makes has the same one.
    supplied.len() == key.len()
        && supplied
            .iter()
            .zip(key)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

/// A header that is present twice, or is not text, which is as good as hostile.
struct Malformed;

/// A header's value, `None` when it is absent.
fn single_header<'a>(
    headers: &'a HeaderMap,
    name: &HeaderName,
) -> Result<Option<&'a str>, Malformed> {
    let mut values = headers.get_all(name).iter();
    match (values.next(), values.next()) {
        (None, _) => Ok(None),
        (Some(value), None) => value.to_str().map(Some).map_err(|_| Malformed),
        (Some(_), Some(_)) => Err(Malformed),
    }
}

/// The `Origin` and `Host` decision, from the header values (`None` for an absent header). The key
/// is checked separately, by [`key_matches`].
///
/// - No `Origin`: allowed, provided the `Host` is allowed.
/// - An `Origin`: allowed only if it is an application origin (see [`is_application_origin`]), a
///   loopback origin, or the same origin as the `Host` — and the `Host` is allowed. `Origin: null`
///   and anything unparseable are neither.
/// - A missing `Host` is never allowed: every client the daemon has speaks HTTP/1.1 and sends one.
fn request_allowed(origin: Option<&str>, host: Option<&str>) -> bool {
    let Some(host) = host.and_then(Authority::parse) else {
        return false;
    };
    if !host_allowed(&host) {
        return false;
    }
    let Some(origin) = origin else {
        return true;
    };
    let Some(origin) = Origin::parse(origin) else {
        return false;
    };
    is_application_origin(&origin) || is_loopback_origin(&origin) || origin.is_same_as(&host)
}

/// The hosts the daemon may be addressed as. The one place to widen when the daemon can be
/// reached from another machine: it is bound to loopback only today, so no other `Host` can be
/// legitimate, and the same-origin rule in [`request_allowed`] then adds nothing to the loopback
/// one.
fn host_allowed(host: &Authority) -> bool {
    is_loopback(&host.host)
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

/// A page served over HTTP(S) from this machine. Such an origin is accepted so that the UI served
/// by a dev server works; it is the key that keeps an untrusted page on that origin out.
fn is_loopback_origin(origin: &Origin) -> bool {
    matches!(origin.scheme.as_str(), "http" | "https") && is_loopback(&origin.authority.host)
}

/// The origins the packaged application's webview reports: `tauri://localhost` on macOS and
/// Linux, `http://tauri.localhost` (`https://` when the app opts into that scheme) on Windows.
fn is_application_origin(origin: &Origin) -> bool {
    origin.authority.port.is_none()
        && matches!(
            (origin.scheme.as_str(), origin.authority.host.as_str()),
            ("tauri", "localhost") | ("http" | "https", "tauri.localhost")
        )
}

/// `host[:port]`, the host lower-cased and an IPv6 host keeping its brackets.
#[derive(Debug, PartialEq)]
struct Authority {
    host: String,
    port: Option<u16>,
}

impl Authority {
    fn parse(text: &str) -> Option<Self> {
        let (host, port) = if text.starts_with('[') {
            let end = text.find(']')? + 1;
            match &text[end..] {
                "" => (&text[..end], None),
                rest => (&text[..end], Some(rest.strip_prefix(':')?)),
            }
        } else {
            match text.split_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (text, None),
            }
        };
        let valid_host = !host.is_empty()
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '[' | ']' | ':'));
        if !valid_host {
            return None;
        }
        let port = match port {
            None => None,
            Some(port) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
                Some(port.parse().ok()?)
            }
            Some(_) => return None,
        };
        Some(Self {
            host: host.to_ascii_lowercase(),
            port,
        })
    }
}

/// `scheme://host[:port]`, with the scheme lower-cased. Browsers send an origin with no path.
struct Origin {
    scheme: String,
    authority: Authority,
}

impl Origin {
    fn parse(text: &str) -> Option<Self> {
        let (scheme, authority) = text.split_once("://")?;
        let scheme_ok = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
        if !scheme_ok {
            return None;
        }
        Some(Self {
            scheme: scheme.to_ascii_lowercase(),
            authority: Authority::parse(authority)?,
        })
    }

    /// Whether this origin is where `host` is: the same host and the same port, an absent port
    /// standing for the scheme's default.
    fn is_same_as(&self, host: &Authority) -> bool {
        let default_port = match self.scheme.as_str() {
            "http" => 80,
            "https" => 443,
            _ => return false,
        };
        self.authority.host == host.host
            && self.authority.port.unwrap_or(default_port) == host.port.unwrap_or(default_port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(origin: Option<&str>, host: &str) -> bool {
        request_allowed(origin, Some(host))
    }

    #[test]
    fn a_request_with_no_origin_is_allowed_on_a_loopback_host() {
        for host in [
            "127.0.0.1:4000",
            "localhost:4000",
            "[::1]:4000",
            "LOCALHOST:4000",
            "localhost",
        ] {
            assert!(allowed(None, host), "{host}");
        }
    }

    #[test]
    fn a_foreign_or_missing_host_is_refused_whatever_the_origin() {
        // DNS rebinding: the page's origin and the host it reached agree, and neither is loopback.
        assert!(!allowed(
            Some("http://evil.example:4000"),
            "evil.example:4000"
        ));
        assert!(!allowed(None, "evil.example:4000"));
        assert!(!allowed(Some("http://localhost:5174"), "evil.example:4000"));
        assert!(!allowed(None, "localhost.evil.example:4000"));
        assert!(!allowed(None, "127.0.0.1.evil.example"));
        assert!(!allowed(None, "[::2]:4000"));
        assert!(!request_allowed(None, None));
        assert!(!request_allowed(Some("http://localhost:5174"), None));
    }

    #[test]
    fn a_malformed_host_is_refused() {
        for host in [
            "",
            "localhost:",
            "localhost:http",
            "localhost:99999",
            "local host",
            "[::1",
        ] {
            assert!(!allowed(None, host), "{host:?}");
        }
        assert!(!allowed(None, "localhost@evil.example"));
    }

    #[test]
    fn a_loopback_origin_is_allowed_on_any_port_and_either_scheme() {
        for origin in [
            "http://localhost:5173",
            "http://localhost:5174",
            "https://localhost:8443",
            "http://127.0.0.1:3000",
            "http://[::1]:3000",
            "http://localhost",
            "HTTP://LocalHost:5173",
        ] {
            assert!(allowed(Some(origin), "127.0.0.1:4000"), "{origin}");
        }
    }

    #[test]
    fn the_application_webview_origin_is_allowed() {
        for origin in [
            "tauri://localhost",
            "TAURI://Localhost",
            "http://tauri.localhost",
            "https://tauri.localhost",
        ] {
            assert!(allowed(Some(origin), "127.0.0.1:4000"), "{origin}");
        }
        // Only the exact origins the webview reports, not a lookalike.
        assert!(!allowed(Some("tauri://localhost:1420"), "127.0.0.1:4000"));
        assert!(!allowed(Some("tauri://evil.example"), "127.0.0.1:4000"));
        assert!(!allowed(Some("tauri://tauri.localhost"), "127.0.0.1:4000"));
        assert!(!allowed(
            Some("http://evil.tauri.localhost"),
            "127.0.0.1:4000"
        ));
    }

    #[test]
    fn a_foreign_origin_is_refused() {
        for origin in [
            "https://evil.example",
            "http://evil.example:4000",
            "http://localhost.evil.example",
            "http://127.0.0.1.evil.example",
            "http://evil.example/",
            "http://localhost@evil.example",
            "http://localhost:5174@evil.example",
            "http://localhost:5174/path",
            "file://localhost",
            "null",
            "",
            "localhost",
            "://localhost",
            "http://",
            "http://localhost:",
            "http://[::1",
            "http://localhost:99999",
        ] {
            assert!(!allowed(Some(origin), "127.0.0.1:4000"), "{origin:?}");
        }
    }

    #[test]
    fn the_same_origin_rule_compares_host_and_port_with_the_default_port_implied() {
        let same = |origin: &str, host: &str| {
            let origin = Origin::parse(origin).expect("a well-formed origin");
            origin.is_same_as(&Authority::parse(host).expect("a well-formed host"))
        };
        assert!(same("http://example.test", "example.test:80"));
        assert!(same("http://example.test:80", "EXAMPLE.test"));
        assert!(same("https://example.test", "example.test:443"));
        assert!(same("http://example.test:4000", "example.test:4000"));
        assert!(same("http://[::1]:4000", "[::1]:4000"));
        assert!(!same("http://example.test", "example.test:443"));
        assert!(!same("https://example.test", "example.test:80"));
        assert!(!same("http://example.test:4000", "example.test:4001"));
        assert!(!same("http://example.test", "other.test"));
        assert!(!same("tauri://example.test", "example.test"));
    }

    #[test]
    fn an_origin_matching_a_host_that_is_not_allowed_still_gets_nowhere() {
        // Same-origin alone is not enough: the host has to be allowed first.
        assert!(!allowed(Some("http://example.test"), "example.test"));
    }

    #[test]
    fn an_ipv6_host_is_compared_with_its_brackets() {
        assert_eq!(
            Authority::parse("[::1]:80"),
            Some(Authority {
                host: "[::1]".to_string(),
                port: Some(80)
            })
        );
        assert_eq!(Authority::parse("[::1]x"), None);
        assert!(allowed(Some("http://[::1]"), "[::1]:80"));
    }

    #[test]
    fn a_generated_key_is_64_lower_case_hex_characters_and_differs_each_time() {
        let key = generate_key().unwrap();
        assert_eq!(key.len(), 64);
        assert!(key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
        assert_ne!(key, generate_key().unwrap());
    }

    #[test]
    fn only_the_daemons_key_in_the_query_is_accepted() {
        let key = "0123abcd";
        assert!(key_matches(Some("key=0123abcd"), key));
        assert!(key_matches(Some("a=1&key=0123abcd&b=2"), key));
        assert!(key_matches(Some("key=0123abcd&key=other"), key));
        assert!(!key_matches(None, key));
        assert!(!key_matches(Some(""), key));
        assert!(!key_matches(Some("key="), key));
        assert!(!key_matches(Some("key=0123abce"), key));
        assert!(!key_matches(Some("key=0123abc"), key));
        assert!(!key_matches(Some("key=0123abcd0"), key));
        assert!(!key_matches(Some("key=other&key=0123abcd"), key));
        assert!(!key_matches(Some("xkey=0123abcd"), key));
        assert!(!key_matches(Some("KEY=0123abcd"), key));
        assert!(!key_matches(Some("key"), key));
    }

    #[test]
    fn a_repeated_or_non_text_header_is_malformed() {
        let value = |headers: &HeaderMap| match single_header(headers, &header::ORIGIN) {
            Ok(value) => Some(value.map(str::to_string)),
            Err(Malformed) => None,
        };
        let mut headers = HeaderMap::new();
        assert_eq!(value(&headers), Some(None));
        headers.append(header::ORIGIN, "http://localhost".parse().unwrap());
        assert_eq!(value(&headers), Some(Some("http://localhost".to_string())));
        headers.append(header::ORIGIN, "http://localhost".parse().unwrap());
        assert_eq!(value(&headers), None);

        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            axum::http::HeaderValue::from_bytes(b"http://\xff").unwrap(),
        );
        assert_eq!(value(&headers), None);
    }
}
