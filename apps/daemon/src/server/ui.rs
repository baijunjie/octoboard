//! The browser UI the daemon serves itself in the server form: the build of `packages/ui`, embedded
//! in the binary at build time (the `embedded-ui` feature, see `build.rs`) and served same-origin
//! with the protocol routes, so a browser reaching the daemon's port loads the UI and the UI finds
//! the daemon at its own origin.
//!
//! A build without the feature, the desktop form's sidecar among them, embeds nothing and serves
//! nothing here.
//!
//! The files are the published application's, holding nothing secret, so they are served without
//! the key: a page's script, stylesheet and font requests carry no `?key=`. The `Host` and `Origin`
//! rules still apply to them ([`crate::server::access`]).

use axum::extract::Request;
use axum::http::{header, Method, StatusCode};
use axum::response::{IntoResponse, Response};

/// A build's files as `(path relative to the build directory, contents)`, the path with `/`
/// separators and no leading one.
pub(crate) type Files = &'static [(&'static str, &'static [u8])];

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded_ui.rs"));
}

/// The files this binary embeds; none without the `embedded-ui` feature.
pub(crate) const EMBEDDED: Files = embedded::FILES;

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
    files.iter().copied().find(|(name, _)| *name == wanted)
}

/// The router's fallback: the embedded file a request names, or `404`. Reached only once the
/// guard has let the request through.
pub(super) async fn serve(files: Files, request: Request) -> Response {
    let Some((name, contents)) = file(files, request.method(), request.uri().path()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // The server leaves out the body of a response to `HEAD` itself, keeping its length.
    (
        [
            (header::CONTENT_TYPE, content_type(name)),
            (header::CACHE_CONTROL, cache_control(name)),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        contents,
    )
        .into_response()
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
/// never changes; the page itself names the current ones and is checked again on every load.
fn cache_control(name: &str) -> &'static str {
    if name.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;

    const FILES: Files = &[
        ("index.html", b"<html>"),
        ("assets/index-1a2b.js", b"js"),
        ("assets/font.woff2", b"font"),
    ];

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
            assert_eq!(file(&[], &Method::GET, path), None, "{path}");
        }
    }

    #[test]
    fn a_build_without_a_page_has_no_root() {
        let files: Files = &[("assets/a.js", b"js")];
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
        assert_eq!(answer(&[], Method::GET, "/").await.0, StatusCode::NOT_FOUND);
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
    async fn hashed_assets_are_cached_for_good_and_the_rest_is_revalidated() {
        const FILES: Files = &[
            ("index.html", b""),
            ("assets/index-1a2b.js", b""),
            ("assets/nested/chunk.css", b""),
            ("favicon.ico", b""),
            ("assets.js", b""),
            ("other/assets/x.js", b""),
        ];
        for (path, expected) in [
            ("/", "no-cache"),
            ("/index.html", "no-cache"),
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
        const FILES: Files = &[
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
        ];
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
}
