//! The browser UI the daemon serves itself: the build of `packages/ui`, embedded in the binary at
//! build time (the `embedded-ui` feature, see `build.rs`), gzip-compressed, and served same-origin
//! with the protocol routes, so a browser reaching the daemon loads the UI and the UI finds the
//! daemon at its own origin. It is served on the browser's way in ([`crate::browser`]: the owner
//! socket, or on Windows the desktop daemon's browser port), and on the TCP port only of a daemon
//! that follows no application on a host that has no such sockets: a daemon that follows the
//! desktop application serves none on its port on any system.
//!
//! A build without the feature embeds nothing and serves nothing here.
//!
//! The files are the published application's, holding nothing secret, so they are served without
//! the key: a page's script, stylesheet and font requests carry no `?key=`. The `Host` and `Origin`
//! rules still apply to them ([`crate::server::access`]).

use std::io::Read;

use axum::extract::Request;
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};

/// A build's files as `(path relative to the build directory, contents)`, the path with `/`
/// separators and no leading one, and whether the contents are gzip-compressed, as the embedded
/// ones are.
#[derive(Clone, Copy)]
pub(crate) struct Files {
    entries: &'static [(&'static str, &'static [u8])],
    gzipped: bool,
}

impl Files {
    /// No files: a listener that serves no UI.
    pub(crate) const NONE: Files = Files::plain(&[]);

    /// Files stored as they are served.
    pub(crate) const fn plain(entries: &'static [(&'static str, &'static [u8])]) -> Self {
        Self {
            entries,
            gzipped: false,
        }
    }

    /// Files stored gzip-compressed.
    #[cfg(test)]
    pub(crate) const fn gzipped(entries: &'static [(&'static str, &'static [u8])]) -> Self {
        Self {
            entries,
            gzipped: true,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded_ui.rs"));
}

/// The files this binary embeds, compressed; none without the `embedded-ui` feature.
pub(crate) const EMBEDDED: Files = Files {
    entries: embedded::FILES,
    gzipped: true,
};

/// The file a request for `path` gets, if a `GET` or `HEAD` of it would get one: the page itself
/// for `/`, otherwise the file at exactly that path.
pub(crate) fn file(
    files: Files,
    method: &Method,
    path: &str,
) -> Option<(&'static str, &'static [u8])> {
    if method != Method::GET && method != Method::HEAD {
        return None;
    }
    let wanted = match path {
        "/" => "index.html",
        path => path.strip_prefix('/')?,
    };
    files
        .entries
        .iter()
        .copied()
        .find(|(name, _)| *name == wanted)
}

/// The router's fallback: the embedded file a request names, or `404`. Reached only once the guard
/// has let the request through. A compressed file goes out as it is stored, marked as gzip, to a
/// client that takes gzip, which every browser does, and is decompressed for one that does not.
pub(super) async fn serve(files: Files, request: Request) -> Response {
    let Some((name, contents)) = file(files, request.method(), request.uri().path()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // The server leaves out the body of a response to `HEAD` itself, keeping its length.
    let mut response = (
        [
            (header::CONTENT_TYPE, content_type(name)),
            (header::CACHE_CONTROL, cache_control(name)),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            // The page's address carries its key: it is sent as nobody's referrer, whatever the
            // page itself says, and the page is shown in no frame another site could draw it in.
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CONTENT_SECURITY_POLICY, "frame-ancestors 'none'"),
        ],
        body(files, contents, request.headers()),
    )
        .into_response();
    if files.gzipped {
        let headers = response.headers_mut();
        headers.insert(header::VARY, HeaderValue::from_static("accept-encoding"));
        if accepts_gzip(request.headers()) {
            headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        }
    }
    response
}

/// The bytes sent for `contents`: as they are stored, unless they are compressed and the client
/// does not take gzip.
fn body(files: Files, contents: &'static [u8], headers: &HeaderMap) -> axum::body::Body {
    if !files.gzipped || accepts_gzip(headers) {
        return axum::body::Body::from(contents);
    }
    let mut plain = Vec::new();
    match flate2::read::GzDecoder::new(contents).read_to_end(&mut plain) {
        Ok(_) => axum::body::Body::from(plain),
        // The build compressed it: a failure here is a defect of the build, never of the request.
        Err(err) => {
            tracing::error!(%err, "an embedded UI file does not decompress");
            axum::body::Body::empty()
        }
    }
}

/// Whether the request's `Accept-Encoding` takes gzip: it names `gzip` or `*` without `q=0`.
fn accepts_gzip(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|coding| {
            let mut parts = coding.split(';').map(str::trim);
            let name = parts.next().unwrap_or_default();
            let refused = parts.any(|parameter| {
                parameter
                    .strip_prefix("q=")
                    .and_then(|q| q.parse::<f32>().ok())
                    .is_some_and(|q| q == 0.0)
            });
            (name.eq_ignore_ascii_case("gzip") || name == "*") && !refused
        })
}

fn content_type(name: &str) -> &'static str {
    match name.rsplit_once('.').map(|(_, extension)| extension) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("wasm") => "application/wasm",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

/// The bundler names what it writes under `assets/` after a hash of its contents, so such a file
/// never changes. The page itself names the current ones and is never stored: it is fetched afresh
/// on every load, a reload after an update included, so it is never stale, and a browser keeps no
/// copy on disk filed under its address, which carries the key. The other files are checked again
/// on every load.
fn cache_control(name: &str) -> &'static str {
    if name.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else if name == "index.html" {
        "no-store"
    } else {
        "no-cache"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;

    const FILES: Files = Files::plain(&[
        ("index.html", b"<html>"),
        ("assets/index-1a2b.js", b"js"),
        ("assets/font.woff2", b"font"),
    ]);

    /// What `serve` answers to `method` `path` over `files`: status, the headers, and the body.
    async fn answer(
        files: Files,
        method: Method,
        path: &str,
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .body(Body::empty())
            .expect("a request");
        let response = serve(files, request).await;
        let (parts, body) = response.into_parts();
        let body = to_bytes(body, usize::MAX).await.expect("a body");
        (parts.status, parts.headers, body.to_vec())
    }

    fn header_of(headers: &axum::http::HeaderMap, name: header::HeaderName) -> &str {
        headers
            .get(name)
            .expect("the header is set")
            .to_str()
            .expect("a text value")
    }

    #[test]
    fn the_root_is_the_page_and_other_paths_name_a_file_exactly() {
        let get = |path| file(FILES, &Method::GET, path).map(|(name, _)| name);
        assert_eq!(get("/"), Some("index.html"));
        assert_eq!(get("/index.html"), Some("index.html"));
        assert_eq!(get("/assets/index-1a2b.js"), Some("assets/index-1a2b.js"));
        for missing in [
            "",
            "/assets",
            "/assets/",
            "//index.html",
            "/assets/../index.html",
            "/./index.html",
            "/nope",
            "index.html",
        ] {
            assert_eq!(get(missing), None, "{missing}");
        }
    }

    #[test]
    fn a_file_comes_with_its_contents() {
        assert_eq!(
            file(FILES, &Method::GET, "/assets/index-1a2b.js"),
            Some(("assets/index-1a2b.js", b"js".as_slice()))
        );
    }

    #[test]
    fn an_unknown_path_is_not_answered_with_the_page() {
        assert_eq!(file(FILES, &Method::GET, "/settings/projects"), None);
    }

    #[test]
    fn a_build_without_files_has_nothing_to_serve() {
        for path in ["/", "/index.html", "/assets/index-1a2b.js"] {
            assert_eq!(file(Files::NONE, &Method::GET, path), None, "{path}");
        }
    }

    #[test]
    fn a_build_without_a_page_has_no_root() {
        let files = Files::plain(&[("assets/a.js", b"js")]);
        assert_eq!(file(files, &Method::GET, "/"), None);
        assert!(file(files, &Method::GET, "/assets/a.js").is_some());
    }

    #[test]
    fn only_get_and_head_are_served() {
        assert!(file(FILES, &Method::GET, "/").is_some());
        assert!(file(FILES, &Method::HEAD, "/").is_some());
        assert!(file(FILES, &Method::HEAD, "/assets/index-1a2b.js").is_some());
        for method in [
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
            Method::TRACE,
            Method::CONNECT,
        ] {
            assert!(file(FILES, &method, "/").is_none(), "{method}");
            assert!(
                file(FILES, &method, "/assets/index-1a2b.js").is_none(),
                "{method}"
            );
        }
    }

    #[tokio::test]
    async fn a_file_is_answered_with_its_contents_type_and_no_sniffing() {
        let (status, headers, body) = answer(FILES, Method::GET, "/").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, b"<html>");
        assert_eq!(
            header_of(&headers, header::CONTENT_TYPE),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            header_of(&headers, header::X_CONTENT_TYPE_OPTIONS),
            "nosniff"
        );
    }

    #[tokio::test]
    async fn a_path_that_names_no_file_is_answered_with_not_found() {
        for (method, path) in [
            (Method::GET, "/nope"),
            (Method::GET, "/assets/"),
            (Method::POST, "/"),
            (Method::DELETE, "/index.html"),
        ] {
            let (status, headers, body) = answer(FILES, method.clone(), path).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}");
            assert!(headers.get(header::CACHE_CONTROL).is_none(), "{path}");
            assert!(!body.starts_with(b"<html>"), "{method} {path}");
        }
        assert_eq!(
            answer(Files::NONE, Method::GET, "/").await.0,
            StatusCode::NOT_FOUND
        );
    }

    /// Every file, the page above all, is sent as nobody's referrer and drawn in no frame.
    #[tokio::test]
    async fn every_file_refuses_referrers_and_frames() {
        for path in ["/", "/index.html", "/assets/index-1a2b.js"] {
            let (_, headers, _) = answer(FILES, Method::GET, path).await;
            assert_eq!(
                header_of(&headers, header::REFERRER_POLICY),
                "no-referrer",
                "{path}"
            );
            assert_eq!(
                header_of(&headers, header::CONTENT_SECURITY_POLICY),
                "frame-ancestors 'none'",
                "{path}"
            );
        }
    }

    #[tokio::test]
    async fn head_gets_the_headers_of_get() {
        let (get_status, get_headers, _) = answer(FILES, Method::GET, "/assets/font.woff2").await;
        let (head_status, head_headers, _) =
            answer(FILES, Method::HEAD, "/assets/font.woff2").await;
        assert_eq!(head_status, get_status);
        assert_eq!(head_headers, get_headers);
    }

    #[tokio::test]
    async fn hashed_assets_are_cached_for_good_the_page_never_and_the_rest_is_revalidated() {
        const FILES: Files = Files::plain(&[
            ("index.html", b""),
            ("assets/index-1a2b.js", b""),
            ("assets/nested/chunk.css", b""),
            ("favicon.ico", b""),
            ("assets.js", b""),
            ("other/assets/x.js", b""),
        ]);
        for (path, expected) in [
            ("/", "no-store"),
            ("/index.html", "no-store"),
            ("/favicon.ico", "no-cache"),
            ("/assets.js", "no-cache"),
            ("/other/assets/x.js", "no-cache"),
            (
                "/assets/index-1a2b.js",
                "public, max-age=31536000, immutable",
            ),
            (
                "/assets/nested/chunk.css",
                "public, max-age=31536000, immutable",
            ),
        ] {
            let (_, headers, _) = answer(FILES, Method::GET, path).await;
            assert_eq!(
                header_of(&headers, header::CACHE_CONTROL),
                expected,
                "{path}"
            );
        }
    }

    #[tokio::test]
    async fn each_kind_of_file_a_build_writes_has_its_type() {
        const FILES: Files = Files::plain(&[
            ("index.html", b""),
            ("assets/a.js", b""),
            ("assets/a.mjs", b""),
            ("assets/a.css", b""),
            ("assets/a.json", b""),
            ("assets/a.wasm", b""),
            ("assets/a.svg", b""),
            ("assets/a.png", b""),
            ("favicon.ico", b""),
            ("assets/a.woff2", b""),
            ("assets/a.woff", b""),
            ("assets/a.ttf", b""),
            ("assets/unknown", b""),
            ("assets/a.map", b""),
        ]);
        for (path, expected) in [
            ("/index.html", "text/html; charset=utf-8"),
            ("/assets/a.js", "text/javascript; charset=utf-8"),
            ("/assets/a.mjs", "text/javascript; charset=utf-8"),
            ("/assets/a.css", "text/css; charset=utf-8"),
            ("/assets/a.json", "application/json"),
            ("/assets/a.wasm", "application/wasm"),
            ("/assets/a.svg", "image/svg+xml"),
            ("/assets/a.png", "image/png"),
            ("/favicon.ico", "image/x-icon"),
            ("/assets/a.woff2", "font/woff2"),
            ("/assets/a.woff", "font/woff"),
            ("/assets/a.ttf", "font/ttf"),
            ("/assets/unknown", "application/octet-stream"),
            ("/assets/a.map", "application/octet-stream"),
        ] {
            let (_, headers, _) = answer(FILES, Method::GET, path).await;
            assert_eq!(
                header_of(&headers, header::CONTENT_TYPE),
                expected,
                "{path}"
            );
        }
    }

    /// Gzip of `text`, as the build stores an embedded file.
    fn gzipped(text: &[u8]) -> &'static [u8] {
        use std::io::Write;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(text).unwrap();
        Box::leak(encoder.finish().unwrap().into_boxed_slice())
    }

    /// A compressed file goes out as stored to a client that takes gzip, marked so, and
    /// decompressed to one that does not; both are told the answer varies by what they take.
    #[tokio::test]
    async fn a_compressed_file_is_sent_compressed_to_a_client_that_takes_gzip() {
        let stored = gzipped(b"<html>page</html>");
        let files = Files::gzipped(Box::leak(Box::new([("index.html", stored)])));
        let ask = |encoding: Option<&'static str>| async move {
            let mut request = Request::builder().method(Method::GET).uri("/");
            if let Some(encoding) = encoding {
                request = request.header(header::ACCEPT_ENCODING, encoding);
            }
            let response = serve(files, request.body(Body::empty()).unwrap()).await;
            let (parts, body) = response.into_parts();
            let body = to_bytes(body, usize::MAX).await.unwrap().to_vec();
            (parts.headers, body)
        };
        for taking in ["gzip, deflate, br", "br;q=1.0, gzip;q=0.8", "*", "GZIP"] {
            let (headers, body) = ask(Some(taking)).await;
            assert_eq!(
                header_of(&headers, header::CONTENT_ENCODING),
                "gzip",
                "{taking}"
            );
            assert_eq!(
                header_of(&headers, header::VARY),
                "accept-encoding",
                "{taking}"
            );
            assert_eq!(body, stored, "{taking}");
        }
        for refusing in [None, Some("identity"), Some("gzip;q=0"), Some("br")] {
            let (headers, body) = ask(refusing).await;
            assert!(
                headers.get(header::CONTENT_ENCODING).is_none(),
                "{refusing:?}"
            );
            assert_eq!(
                header_of(&headers, header::VARY),
                "accept-encoding",
                "{refusing:?}"
            );
            assert_eq!(body, b"<html>page</html>", "{refusing:?}");
        }
    }

    /// A plain file is never marked compressed, whatever the client takes.
    #[tokio::test]
    async fn a_plain_file_is_sent_as_it_is() {
        let request = Request::builder()
            .uri("/")
            .header(header::ACCEPT_ENCODING, "gzip")
            .body(Body::empty())
            .unwrap();
        let response = serve(FILES, request).await;
        assert!(response.headers().get(header::CONTENT_ENCODING).is_none());
        assert!(response.headers().get(header::VARY).is_none());
    }
}
