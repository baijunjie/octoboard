//! What a listener does with a request's body so that no answer goes out while request bytes are
//! still to come, shared by the daemon's own listener and the holders'.
//!
//! Answered from the head alone, a response races the rest of a request that a caller writes after
//! its head (`crate::loopback::exchange` does): hyper closes a connection whose body it found
//! unread, and body bytes that reach it after hyper last looked and before it closes make the
//! kernel reset the connection rather than end it, so the caller reads the reset in place of the
//! answer it was already sent. Two layers close the gap, and what a request costs the listener
//! depends on who it is:
//!
//! - [`drain_unread`] goes outermost. Whatever answers, a refusal or a fallback included, the body
//!   the answer left unread is read and thrown away before the answer goes out, so a caller that
//!   was refused costs the daemon no memory however much it sends.
//! - [`buffer`] goes inside the guard, around the routes, so that only a request the guard let in
//!   is held: its whole body is taken before any route runs, one that answers without taking it
//!   included.
//!
//! The limit is [`REQUEST_BODY_LIMIT`]; to read past it would let one caller keep a connection
//! busy for as long as it keeps writing, so a body over it is answered `413` without the rest being
//! read, and a caller that sends more than that anyway is outside the guarantee above.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Poll;

use axum::body::{Body, BodyDataStream, Bytes};
use axum::extract::Request;
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;

/// The most of a request's body a listener takes: axum's default limit on the body an extractor
/// takes, so any body a route would accept is read in full.
pub const REQUEST_BODY_LIMIT: usize = 2 * 1024 * 1024;

/// The body of a request handed on by [`drain_unread`], which still has what the one it was made
/// from has not yet given.
type Unread = Arc<Mutex<Option<BodyDataStream>>>;

/// A middleware that answers only once the request's body has been read: what the rest of the
/// stack leaves unread is read and discarded, up to [`REQUEST_BODY_LIMIT`] in all and never
/// accumulated, before the response it made is returned. Wrap it around a finished router, outside
/// the guard, so that a refusal or a route added later cannot end up outside it.
///
/// The request is rebuilt from its parts, so its extensions survive: a WebSocket upgrade, which
/// carries no body, goes through as it came. A body whose stated length is over the limit is not
/// read at all.
pub async fn drain_unread(request: Request, next: Next) -> Response {
    let (parts, body) = request.into_parts();
    let hopeless = stated_over_limit(&parts.headers);
    let unread: Unread = Arc::new(Mutex::new(Some(body.into_data_stream())));
    let taken = Arc::new(AtomicUsize::new(0));
    let handed = Body::from_stream(futures_util::stream::poll_fn({
        let (unread, taken) = (unread.clone(), taken.clone());
        move |cx| {
            let mut unread = unread.lock().unwrap_or_else(|err| err.into_inner());
            let Some(stream) = unread.as_mut() else {
                return Poll::Ready(None);
            };
            let polled = stream.poll_next_unpin(cx);
            match &polled {
                Poll::Ready(Some(Ok(chunk))) => {
                    taken.fetch_add(chunk.len(), Ordering::Relaxed);
                }
                // Over, one way or another: never polled again, whatever the source does then.
                Poll::Ready(None | Some(Err(_))) => *unread = None,
                Poll::Pending => {}
            }
            polled
        }
    }));
    let response = next.run(Request::from_parts(parts, handed)).await;
    let stream = unread.lock().unwrap_or_else(|err| err.into_inner()).take();
    if let (Some(mut stream), false) = (stream, hopeless) {
        let mut seen = taken.load(Ordering::Relaxed);
        // Whatever ends it, a body that breaks off included: there is no one left to tell.
        // `taken` can already be over the limit, so it is checked before each read.
        while seen <= REQUEST_BODY_LIMIT {
            let Some(Ok(chunk)) = stream.next().await else {
                break;
            };
            seen += chunk.len();
        }
    }
    response
}

/// A middleware that reads each request's whole body before the request goes on, so that no route
/// answers while request bytes are still to come. Put it inside the guard: a body is held only for
/// a request the guard let in, and [`drain_unread`], outside it, reads a refused one without
/// holding it.
///
/// A body over [`REQUEST_BODY_LIMIT`] is answered `413` at once, from its `Content-Length` where it
/// states one and otherwise as the limit is passed; a body that breaks off or is malformed is
/// answered `400`.
pub async fn buffer(request: Request, next: Next) -> Response {
    let (parts, body) = request.into_parts();
    if stated_over_limit(&parts.headers) {
        return too_large();
    }
    match read_whole(body).await {
        Ok(bytes) => {
            next.run(Request::from_parts(parts, Body::from(bytes)))
                .await
        }
        Err(Unreadable::TooLarge) => too_large(),
        Err(Unreadable::Broken) => {
            (StatusCode::BAD_REQUEST, "unreadable request body").into_response()
        }
    }
}

/// Why a body was not taken whole.
enum Unreadable {
    /// It is over [`REQUEST_BODY_LIMIT`].
    TooLarge,
    /// Its framing is malformed, or the caller went away mid-body.
    Broken,
}

async fn read_whole(body: Body) -> Result<Bytes, Unreadable> {
    let mut stream = body.into_data_stream();
    let mut whole = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| Unreadable::Broken)?;
        if whole.len() + chunk.len() > REQUEST_BODY_LIMIT {
            return Err(Unreadable::TooLarge);
        }
        whole.extend_from_slice(&chunk);
    }
    Ok(whole.into())
}

/// Whether the request states a body length over the limit; a length that is no number is not
/// judged here, and the body is read as it comes.
fn stated_over_limit(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|length| length > REQUEST_BODY_LIMIT)
}

fn too_large() -> Response {
    (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::body_first::{status_once_the_body_is_in, Pending};

    /// What a router behind the layers sees and answers, driven in memory so that the body's
    /// framing is the test's own: how many requests reached the handler, and the body of the last.
    #[derive(Clone, Default)]
    struct Behind {
        reached: Arc<AtomicUsize>,
        body: Arc<Mutex<Option<Bytes>>>,
    }

    impl Behind {
        fn reached(&self) -> usize {
            self.reached.load(Ordering::SeqCst)
        }

        fn app(&self) -> axum::Router {
            use axum::routing::post;

            let behind = self.clone();
            axum::Router::new()
                .route(
                    "/",
                    post(move |body: Bytes| {
                        let behind = behind.clone();
                        async move {
                            behind.reached.fetch_add(1, Ordering::SeqCst);
                            *behind.body.lock().unwrap() = Some(body);
                            StatusCode::OK
                        }
                    }),
                )
                .layer(axum::middleware::from_fn(buffer))
                .layer(axum::middleware::from_fn(drain_unread))
        }

        /// Sends `request` through both layers to a route that stores the body it is handed and
        /// answers `200`, and returns the status.
        async fn status_of(&self, request: Request) -> StatusCode {
            use hyper::service::Service;

            hyper_util::service::TowerToHyperService::new(self.app())
                .call(request)
                .await
                .unwrap()
                .status()
        }
    }

    fn post_to_root() -> axum::http::request::Builder {
        Request::builder().method("POST").uri("/")
    }

    /// A body that arrives in `chunks`, with no length stated, as a chunked request's does.
    fn chunked(chunks: Vec<Vec<u8>>) -> Body {
        Body::from_stream(futures_util::stream::iter(
            chunks
                .into_iter()
                .map(|chunk| Ok::<_, std::io::Error>(Bytes::from(chunk))),
        ))
    }

    /// What the layers are for, on a router of the shapes a listener has: a route that answers
    /// without taking its body, a fallback, and a refusal in a layer between them. None answers
    /// before the body is in, a refusal reads it without the route being reached, and a request's
    /// extensions reach the handler.
    #[tokio::test]
    async fn no_route_answers_before_the_request_body_is_read() {
        use axum::extract::Extension;
        use axum::middleware::Next;
        use axum::routing::post;

        #[derive(Clone)]
        struct Marker;

        let reached = Arc::new(AtomicUsize::new(0));
        let app = || {
            let reached = reached.clone();
            axum::Router::new()
                .route(
                    "/answers",
                    post(move |Extension(_): Extension<Marker>| async move {
                        reached.fetch_add(1, Ordering::SeqCst);
                        StatusCode::ACCEPTED
                    }),
                )
                .fallback(|| async { StatusCode::NOT_FOUND })
                .layer(axum::middleware::from_fn(buffer))
                .layer(axum::middleware::from_fn(
                    |request: Request, next: Next| async move {
                        if request.uri().path() == "/refused" {
                            return StatusCode::UNAUTHORIZED.into_response();
                        }
                        next.run(request).await
                    },
                ))
                .layer(axum::middleware::from_fn(drain_unread))
                .layer(axum::middleware::from_fn(
                    |mut request: Request, next: Next| async move {
                        request.extensions_mut().insert(Marker);
                        next.run(request).await
                    },
                ))
        };
        for (route, status) in [
            ("POST /answers", StatusCode::ACCEPTED),
            ("POST /refused", StatusCode::UNAUTHORIZED),
            ("POST /nowhere", StatusCode::NOT_FOUND),
        ] {
            let before = reached.load(Ordering::SeqCst);
            let answered = status_once_the_body_is_in(app(), route, &[], || {
                assert_eq!(reached.load(Ordering::SeqCst), before, "{route}");
            })
            .await;
            assert_eq!(answered, status, "{route}");
        }
        assert_eq!(reached.load(Ordering::SeqCst), 1);
    }

    /// A refused request's body is read as it comes, a chunk at a time, up to its end, and the
    /// refusal goes out only then.
    #[tokio::test]
    async fn a_refused_body_is_read_to_its_end_before_the_refusal_goes_out() {
        let app = axum::Router::new()
            .fallback(|| async { StatusCode::UNAUTHORIZED })
            .layer(axum::middleware::from_fn(drain_unread));
        let mut held = Pending::start(app, "POST /refused", &[], 3);
        held.assert_unanswered("POST /refused").await;
        for chunk in [&b"a"[..], b"b"] {
            held.send(chunk);
            held.assert_unanswered("POST /refused after a chunk").await;
        }
        held.send(b"c");
        assert_eq!(held.finish().await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_handler_gets_the_whole_body_however_it_arrived() {
        let behind = Behind::default();
        let request = post_to_root()
            .body(chunked(vec![
                b"he".to_vec(),
                Vec::new(),
                b"llo ".to_vec(),
                b"world".to_vec(),
            ]))
            .unwrap();
        assert_eq!(behind.status_of(request).await, StatusCode::OK);
        assert_eq!(
            behind.body.lock().unwrap().as_deref(),
            Some(&b"hello world"[..])
        );

        let request = post_to_root().body(Body::from("whole")).unwrap();
        assert_eq!(behind.status_of(request).await, StatusCode::OK);
        assert_eq!(behind.body.lock().unwrap().as_deref(), Some(&b"whole"[..]));
        assert_eq!(behind.reached(), 2);
    }

    #[tokio::test]
    async fn a_body_of_exactly_the_limit_goes_through_and_one_byte_more_does_not() {
        let at_limit = vec![7u8; REQUEST_BODY_LIMIT];
        let behind = Behind::default();
        let request = post_to_root()
            .body(chunked(vec![at_limit.clone()]))
            .unwrap();
        assert_eq!(behind.status_of(request).await, StatusCode::OK);
        assert_eq!(
            behind.body.lock().unwrap().as_ref().map(|body| body.len()),
            Some(REQUEST_BODY_LIMIT)
        );

        // Chunked, so no length is stated and the limit is met only as the body is read.
        let over = Behind::default();
        let request = post_to_root()
            .body(chunked(vec![at_limit, vec![7u8]]))
            .unwrap();
        assert_eq!(over.status_of(request).await, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(over.reached(), 0, "an oversized body reached a route");
    }

    #[tokio::test]
    async fn a_stated_length_over_the_limit_is_refused_whatever_the_body_is() {
        let over = Behind::default();
        let request = post_to_root()
            .header(header::CONTENT_LENGTH, REQUEST_BODY_LIMIT + 1)
            .body(Body::from("short"))
            .unwrap();
        assert_eq!(over.status_of(request).await, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(over.reached(), 0);

        // A stated length of exactly the limit is not over it.
        let at = Behind::default();
        let request = post_to_root()
            .header(header::CONTENT_LENGTH, REQUEST_BODY_LIMIT)
            .body(Body::from("short"))
            .unwrap();
        assert_eq!(at.status_of(request).await, StatusCode::OK);

        // A length that is no number is not judged; the body is read as it comes.
        let odd = Behind::default();
        let request = post_to_root()
            .header(header::CONTENT_LENGTH, "many")
            .body(Body::from("short"))
            .unwrap();
        assert_eq!(odd.status_of(request).await, StatusCode::OK);
    }

    /// A refusal to a body whose stated length is over the limit goes out at once: `drain_unread`
    /// reads none of it, however much the caller has yet to send.
    #[tokio::test]
    async fn a_refused_body_stated_over_the_limit_is_not_read_before_the_refusal() {
        let app = axum::Router::new()
            .fallback(|| async { StatusCode::UNAUTHORIZED })
            .layer(axum::middleware::from_fn(drain_unread));
        let mut held = Pending::start(app, "POST /refused", &[], REQUEST_BODY_LIMIT + 1);
        assert_eq!(held.answered_while_held().await, StatusCode::UNAUTHORIZED);
    }

    /// A body that breaks off reaches no route and is not taken for one that is too large.
    #[tokio::test]
    async fn a_body_that_breaks_off_is_a_bad_request_and_reaches_no_route() {
        let behind = Behind::default();
        let body = Body::from_stream(futures_util::stream::iter([
            Ok(Bytes::from("half")),
            Err(std::io::Error::other("the caller went away")),
        ]));
        let request = post_to_root().body(body).unwrap();
        assert_eq!(behind.status_of(request).await, StatusCode::BAD_REQUEST);
        assert_eq!(behind.reached(), 0, "half a body reached a route");
    }

    /// A refused body over the limit is read no further than the limit, and a refused body that
    /// breaks off still gets its answer.
    #[tokio::test]
    async fn a_refused_body_is_not_read_past_the_limit_and_a_broken_one_is_still_answered() {
        use hyper::service::Service;

        let app = || {
            axum::Router::new()
                .fallback(|| async { StatusCode::UNAUTHORIZED })
                .layer(axum::middleware::from_fn(drain_unread))
        };
        let read = Arc::new(AtomicUsize::new(0));
        let counted = read.clone();
        // Far more than the limit, which is all of it that may be read.
        let endless = Body::from_stream(
            futures_util::stream::repeat_with(|| {
                Ok::<_, std::io::Error>(Bytes::from(vec![0u8; 64 * 1024]))
            })
            .take((REQUEST_BODY_LIMIT / (64 * 1024)) * 8)
            .inspect(move |chunk| {
                counted.fetch_add(
                    chunk.as_ref().map_or(0, |chunk| chunk.len()),
                    Ordering::SeqCst,
                );
            }),
        );
        let request = post_to_root().body(endless).unwrap();
        let status = hyper_util::service::TowerToHyperService::new(app())
            .call(request)
            .await
            .unwrap()
            .status();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let read = read.load(Ordering::SeqCst);
        assert!(
            (REQUEST_BODY_LIMIT..=REQUEST_BODY_LIMIT + 64 * 1024).contains(&read),
            "{read}"
        );

        let broken = Body::from_stream(futures_util::stream::iter([
            Ok(Bytes::from("half")),
            Err(std::io::Error::other("the caller went away")),
        ]));
        let status = hyper_util::service::TowerToHyperService::new(app())
            .call(post_to_root().body(broken).unwrap())
            .await
            .unwrap()
            .status();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
