//! The tests of a listener that answers only once the request's body has been read
//! (`crate::request_body`) drive its router in memory with a body whose chunks the test holds, so
//! that "no answer yet" is a poll of the answer's future that finds it pending, not a wait on the
//! clock. A refusal that answers from the head alone is caught as long as it does not wait on
//! outside work.

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;

use axum::body::{Body, Bytes};
use axum::extract::Request;
use axum::http::StatusCode;
use axum::Router;
use hyper::service::Service;
use tokio::sync::mpsc;

use super::PATIENCE;

/// How many times the answer is polled to find it pending.
const UNANSWERED_POLLS: usize = 8;

/// A request in flight to a router whose body has not all been sent.
pub(crate) struct Pending {
    answer: Pin<Box<dyn Future<Output = StatusCode>>>,
    body: Option<mpsc::UnboundedSender<Bytes>>,
}

impl Pending {
    /// Sends the head of `request_line` (`"POST /path?query"`) with `headers` (each `"Name:
    /// value"`) and a stated body of `length` bytes, none of which has been sent yet.
    pub(crate) fn start(app: Router, request_line: &str, headers: &[&str], length: usize) -> Self {
        let (method, target) = request_line
            .split_once(' ')
            .expect("a request line is a method and a target");
        let (body, chunks) = mpsc::unbounded_channel::<Bytes>();
        let chunks = futures_util::stream::unfold(chunks, |mut chunks| async move {
            chunks
                .recv()
                .await
                .map(|chunk| (Ok::<_, Infallible>(chunk), chunks))
        });
        let mut request = Request::builder()
            .method(method)
            .uri(target)
            .header("Content-Length", length);
        for header in headers {
            let (name, value) = header.split_once(": ").expect("a header is `Name: value`");
            request = request.header(name, value);
        }
        let request = request
            .body(Body::from_stream(chunks))
            .expect("a well-formed request");
        let service = hyper_util::service::TowerToHyperService::new(app);
        Self {
            answer: Box::pin(async move {
                let Ok(response) = service.call(request).await;
                response.status()
            }),
            body: Some(body),
        }
    }

    /// Polls the answer a few times, yielding to the runtime between polls (no clock), and fails
    /// if it is there. A refusal that answers from the head alone is caught as long as it does not
    /// wait on outside work; one that awaits something that yields a few times is caught too.
    pub(crate) async fn assert_unanswered(&mut self, what: &str) {
        for _ in 0..UNANSWERED_POLLS {
            let early = futures_util::FutureExt::now_or_never(&mut self.answer);
            assert!(
                early.is_none(),
                "{what} was answered before its body was sent: {early:?}"
            );
            tokio::task::yield_now().await;
        }
    }

    /// Sends `chunk` of the body.
    pub(crate) fn send(&mut self, chunk: &[u8]) {
        self.body
            .as_ref()
            .expect("the body is still open")
            .send(Bytes::copy_from_slice(chunk))
            .expect("the router still reads the body");
    }

    /// Returns the answer's status without the body having been sent or ended, and fails if it
    /// does not come.
    pub(crate) async fn answered_while_held(&mut self) -> StatusCode {
        tokio::time::timeout(PATIENCE, &mut self.answer)
            .await
            .expect("the answer comes without the body")
    }

    /// Ends the body and returns the answer's status.
    pub(crate) async fn finish(mut self) -> StatusCode {
        self.body.take();
        tokio::time::timeout(PATIENCE, self.answer)
            .await
            .expect("the answer comes once the body is in")
    }
}

/// What the tests of a listener's answers before a body share: sends the head of a `POST`-like
/// request stating a two-byte body, asserts that no answer comes while the body is held, runs
/// `while_held` (to assert what the router has and has not done by then), sends the body and
/// returns the answer's status. A router that answers from the head alone is found `Ready` at the
/// first step, and one that waits for the body passes however slow the machine.
pub(crate) async fn status_once_the_body_is_in(
    app: Router,
    request_line: &str,
    headers: &[&str],
    while_held: impl FnOnce(),
) -> StatusCode {
    let mut pending = Pending::start(app, request_line, headers, 2);
    pending.assert_unanswered(request_line).await;
    while_held();
    pending.send(b"{}");
    pending.finish().await
}
