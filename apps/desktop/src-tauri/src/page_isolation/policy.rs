//! Which web requests the window's webview refuses, as a decision on the request alone. The
//! WebView2 filter (`windows.rs`) asks it for every `http(s)` request; it lives apart from that
//! module so its tests run on every target.

use tauri::Url;

/// The hosts the webview itself serves from on Windows, where wry exposes a custom protocol as
/// `http(s)://<name>.localhost`: the app's own assets (`tauri`) and the IPC channel (`ipc`). The
/// asset protocol is not enabled in this application, so `asset.localhost` is not among them. Exact
/// hosts, not every `*.localhost`: the rest resolve to the loopback address, where anything may listen.
const APP_HOSTS: [&str; 2] = ["tauri.localhost", "ipc.localhost"];

/// Whether the request for `uri` is refused. Only `http(s)` is judged: every other scheme (`data:`,
/// `blob:`, `ws:`) is not what this guards and passes. An address that does not parse is refused.
///
/// Image loads (`is_image`) pass, for the Markdown documents' remote images. `allow_dev_server`
/// also lets `localhost` through on any port, which a development build loads its UI from (the Vite
/// dev server).
pub fn is_blocked(uri: &str, is_image: bool, allow_dev_server: bool) -> bool {
    let Ok(url) = Url::parse(uri) else {
        return true;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    if is_image {
        return false;
    }
    match url.host_str() {
        Some(host) if url.port().is_none() && APP_HOSTS.contains(&host) => false,
        Some("localhost") => !allow_dev_server,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_web_request_that_is_not_an_image_is_blocked() {
        for uri in [
            "https://example.com/",
            "http://example.com/tracker.js",
            "https://example.com:8443/api",
            "http://127.0.0.1:3000/",
            "https://[::1]/",
        ] {
            assert!(is_blocked(uri, false, false), "{uri}");
        }
    }

    #[test]
    fn an_image_load_passes() {
        assert!(!is_blocked("https://example.com/logo.png", true, false));
        assert!(!is_blocked("http://example.com/logo.png", true, false));
    }

    #[test]
    fn the_applications_own_hosts_pass_over_either_scheme() {
        for uri in [
            "http://tauri.localhost/index.html?port=1",
            "https://tauri.localhost/assets/app.js",
            "http://ipc.localhost/plugin%3Anotification%7Cnotify",
        ] {
            assert!(!is_blocked(uri, false, false), "{uri}");
        }
    }

    #[test]
    fn only_the_exact_application_hosts_pass() {
        for uri in [
            "http://asset.localhost/file.png",
            "http://other.localhost/",
            "http://tauri.localhost.example.com/",
            "http://tauri.localhost@example.com/",
            "http://example.com/tauri.localhost/",
            "http://tauri.localhost:8080/",
        ] {
            assert!(is_blocked(uri, false, false), "{uri}");
        }
    }

    #[test]
    fn the_dev_server_passes_only_where_allowed() {
        for uri in [
            "http://localhost:5174/",
            "http://localhost/",
            "http://localhost:5174/@vite/client",
        ] {
            assert!(is_blocked(uri, false, false), "{uri}");
            assert!(!is_blocked(uri, false, true), "{uri}");
        }
        for uri in [
            "http://localhost.example.com:5174/",
            "http://127.0.0.1:5174/",
        ] {
            assert!(is_blocked(uri, false, true), "{uri}");
        }
    }

    #[test]
    fn other_schemes_are_not_judged() {
        for uri in [
            "ws://127.0.0.1:4000/",
            "data:text/plain,hello",
            "blob:http://tauri.localhost/1",
        ] {
            assert!(!is_blocked(uri, false, false), "{uri}");
        }
    }

    #[test]
    fn an_address_that_does_not_parse_is_blocked() {
        assert!(is_blocked("not a url", false, true));
    }
}
