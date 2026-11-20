//! Something a client uploads in parts, for a link too slow to send it in one frame before the
//! control socket's silence limit (see "The heartbeat" in `apps/daemon/PROTOCOL.md`): a pasted
//! image (`crate::session::pasted_image`) or an attached file (`crate::session::attachment`).
//!
//! A create request names the session and the size and is answered with a handle, each part
//! request adds the next part at the offset the received bytes end at, and the family's save
//! request takes the parts and writes the file. Where the parts are kept until then is the
//! family's [`Body`]: an image's in memory, a file's in a spool file on disk. An upload that is
//! never finished — deleted, refused, left to expire, or cut off with its connection — is dropped
//! with its body, which leaves nothing behind.
//!
//! Each family has its own [`Uploads`] on a connection, so a handle of one is unknown to the
//! other's requests, and its own [`UploadBudget`] across the daemon.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;

use crate::protocol::{error_code, CodedError};
use crate::state::AppState;

/// Uploads of one family one connection may hold at once.
pub const MAX_UPLOADS_PER_CONNECTION: usize = 2;

/// The most one part carries, decoded: about 1.4 MB in its frame.
pub const MAX_PART_BYTES: usize = 1024 * 1024;

/// How long an upload is kept with no part arriving, counted from its creation or its latest part.
pub const IDLE_EXPIRY: Duration = Duration::from_secs(60);

/// What sets one family of uploads apart from another.
#[derive(Debug)]
pub struct Kind {
    /// What the uploaded thing is called in messages: `image`, `file`.
    pub noun: &'static str,
    /// The largest upload taken, in bytes.
    pub max_bytes: usize,
    /// What a handle begins with.
    pub handle_prefix: &'static str,
    /// The `limit` of a `limit_exceeded` past [`MAX_UPLOADS_PER_CONNECTION`].
    pub uploads_limit: &'static str,
    /// The `limit` of a `limit_exceeded` past the family's [`UploadBudget`].
    pub bytes_limit: &'static str,
}

/// Where an upload's parts are kept until it is saved. Dropping it discards them.
pub trait Body: Send + 'static {
    /// Adds the next part after the ones before it. A failure discards the upload.
    fn append(&mut self, part: &[u8]) -> Result<()>;
}

impl Body for Vec<u8> {
    fn append(&mut self, part: &[u8]) -> Result<()> {
        self.extend_from_slice(part);
        Ok(())
    }
}

/// The daemon-wide bytes one family's uploads may hold, by their declared sizes, one per family
/// per daemon (`AppState::pasted_image_uploads`, `AppState::attachment_uploads`).
#[derive(Debug, Clone)]
pub struct UploadBudget {
    bytes: Arc<Semaphore>,
    max: usize,
}

impl UploadBudget {
    pub fn new(max: usize) -> Self {
        Self {
            bytes: Arc::new(Semaphore::new(max)),
            max,
        }
    }

    /// The bytes no upload holds right now.
    #[cfg(test)]
    pub fn available(&self) -> usize {
        self.bytes.available_permits()
    }
}

/// One connection's uploads of one family. Dropped with the connection, which drops every upload
/// it held.
#[derive(Debug)]
pub struct Uploads<B: Body> {
    kind: &'static Kind,
    budget: UploadBudget,
    next: AtomicU64,
    held: Arc<Mutex<HashMap<String, Upload<B>>>>,
}

#[derive(Debug)]
struct Upload<B> {
    session: String,
    size: usize,
    received: usize,
    body: B,
    /// When the upload expires unless another part arrives.
    expires: Instant,
    _budget: OwnedSemaphorePermit,
}

impl<B: Body> Uploads<B> {
    pub fn of_kind(kind: &'static Kind, budget: UploadBudget) -> Self {
        Self {
            kind,
            budget,
            next: AtomicU64::new(0),
            held: Arc::default(),
        }
    }

    /// Starts an upload of `size` bytes for the running session `session_id`, its parts kept in
    /// the body `open` makes from the session's scratch directory, and returns its handle.
    /// Refused with `unknown_session` or `session_not_running`, a size of nothing or over the
    /// kind's limit as a plain error, whatever `open` fails with, and `limit_exceeded` past the
    /// uploads this connection or the daemon may hold (the body made is dropped then). Blocks for
    /// as long as `open` does.
    pub fn create_with(
        &self,
        state: &AppState,
        session_id: &str,
        size: u64,
        open: impl FnOnce(&Path) -> Result<B>,
    ) -> Result<String> {
        let scratch = super::scratch::scratch_of(state, session_id)?;
        let noun = self.kind.noun;
        if size == 0 {
            bail!("the {noun} upload cannot be empty");
        }
        if size > self.kind.max_bytes as u64 {
            bail!("the {noun} is larger than {} bytes", self.kind.max_bytes);
        }
        let size = size as usize;
        let body = open(&scratch)?;
        let mut held = self.held.lock().expect("uploads lock poisoned");
        if held.len() >= MAX_UPLOADS_PER_CONNECTION {
            return Err(limit_exceeded(
                &format!("too many {noun} uploads are under way on this connection"),
                self.kind.uploads_limit,
                MAX_UPLOADS_PER_CONNECTION,
            ));
        }
        let Ok(permit) = self
            .budget
            .bytes
            .clone()
            .try_acquire_many_owned(size as u32)
        else {
            return Err(limit_exceeded(
                &format!("the {noun} uploads under way hold too many bytes"),
                self.kind.bytes_limit,
                self.budget.max,
            ));
        };
        let upload = format!(
            "{}-{}",
            self.kind.handle_prefix,
            self.next.fetch_add(1, Ordering::Relaxed)
        );
        let expires = Instant::now() + IDLE_EXPIRY;
        held.insert(
            upload.clone(),
            Upload {
                session: session_id.to_string(),
                size,
                received: 0,
                body,
                expires,
                _budget: permit,
            },
        );
        drop(held);
        expire_when_idle(Arc::downgrade(&self.held), upload.clone(), expires);
        Ok(upload)
    }

    /// Adds the part `data` (base64) holds at `offset`. Any refusal discards the upload:
    /// `upload_unavailable` for one this connection does not hold, `limit_exceeded` for a part
    /// over [`MAX_PART_BYTES`], a plain error for data that is not base64 or a body that fails to
    /// take the part, and `upload_parts_mismatch` for a part that does not start where the
    /// received bytes end or runs past the upload's size. Blocks for as long as the body's
    /// [`Body::append`] does.
    pub fn write(&self, upload: &str, offset: u64, data: &str) -> Result<()> {
        let mut held = self.held.lock().expect("uploads lock poisoned");
        let Some(entry) = held.get_mut(upload) else {
            return Err(unavailable(self.kind, upload));
        };
        let refusal = match part_of(self.kind, data) {
            Ok(part) => {
                let received = entry.received;
                if offset != received as u64 || received + part.len() > entry.size {
                    Err(mismatch(self.kind, received, entry.size))
                } else {
                    match entry.body.append(&part) {
                        Ok(()) => {
                            entry.received += part.len();
                            entry.expires = Instant::now() + IDLE_EXPIRY;
                            return Ok(());
                        }
                        Err(err) => Err(err),
                    }
                }
            }
            Err(err) => Err(err),
        };
        // Dropped outside the lock: a body on disk is removed as it drops.
        let removed = held.remove(upload);
        drop(held);
        drop(removed);
        refusal
    }

    /// Discards the upload, if this connection holds it.
    pub fn delete(&self, upload: &str) {
        let removed = self
            .held
            .lock()
            .expect("uploads lock poisoned")
            .remove(upload);
        drop(removed);
    }

    /// Takes the upload's body for a save to the session `session_id`, which must be the one it
    /// was created for: refused with `upload_unavailable` otherwise, or for an upload this
    /// connection does not hold, and with `upload_parts_mismatch` when the parts end short of its
    /// size. The upload is gone either way; its bytes stay counted against the budget for as long
    /// as the [`Taken`] holding them.
    pub fn take(&self, upload: &str, session_id: &str) -> Result<Taken<B>> {
        let entry = self
            .held
            .lock()
            .expect("uploads lock poisoned")
            .remove(upload)
            .ok_or_else(|| unavailable(self.kind, upload))?;
        if entry.session != session_id {
            return Err(unavailable(self.kind, upload));
        }
        if entry.received != entry.size {
            return Err(mismatch(self.kind, entry.received, entry.size));
        }
        Ok(Taken {
            body: entry.body,
            _budget: entry._budget,
        })
    }

    /// The uploads this connection holds.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.held.lock().expect("uploads lock poisoned").len()
    }
}

/// An upload's body, taken for its save, with its share of the budget.
#[derive(Debug)]
pub struct Taken<B> {
    pub body: B,
    _budget: OwnedSemaphorePermit,
}

/// Drops `upload` once it has gone `idle` with no part, checking again whenever a part has moved
/// its expiry on; ends as soon as the upload or its connection's uploads are gone.
fn expire_when_idle<B: Body>(
    held: Weak<Mutex<HashMap<String, Upload<B>>>>,
    upload: String,
    first: Instant,
) {
    tokio::spawn(async move {
        let mut at = first;
        loop {
            tokio::time::sleep_until(at).await;
            let Some(held) = held.upgrade() else {
                return;
            };
            let mut held = held.lock().expect("uploads lock poisoned");
            let Some(entry) = held.get(&upload) else {
                return;
            };
            if entry.expires <= Instant::now() {
                let removed = held.remove(&upload);
                drop(held);
                drop(removed);
                return;
            }
            at = entry.expires;
        }
    });
}

/// The bytes of one part, or why it is refused.
fn part_of(kind: &Kind, data: &str) -> Result<Vec<u8>> {
    if data.len() > MAX_PART_BYTES.div_ceil(3) * 4 {
        return Err(part_too_large(kind));
    }
    let part = STANDARD
        .decode(data)
        .with_context(|| format!("the {} part is not valid base64", kind.noun))?;
    if part.len() > MAX_PART_BYTES {
        return Err(part_too_large(kind));
    }
    Ok(part)
}

fn part_too_large(kind: &Kind) -> anyhow::Error {
    limit_exceeded(
        &format!("the {} part is larger than one part may be", kind.noun),
        "upload_part_bytes",
        MAX_PART_BYTES,
    )
}

fn limit_exceeded(message: &str, limit: &str, max: usize) -> anyhow::Error {
    CodedError::raised(
        error_code::LIMIT_EXCEEDED,
        message,
        &[("limit", limit), ("max", &max.to_string())],
    )
}

fn unavailable(kind: &Kind, upload: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::UPLOAD_UNAVAILABLE,
        format!("this connection holds no such {} upload", kind.noun),
        &[("upload", upload)],
    )
}

fn mismatch(kind: &Kind, received: usize, size: usize) -> anyhow::Error {
    CodedError::raised(
        error_code::UPLOAD_PARTS_MISMATCH,
        format!("the {} parts do not add up to the upload", kind.noun),
        &[
            ("expected_offset", &received.to_string()),
            ("size", &size.to_string()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::pasted_image::{ImageUploads, MAX_BYTES, MAX_UPLOAD_BYTES};
    use crate::session::test_support::{fixture, pasted_directory, Fixture, PNG};
    use crate::test_support::{code_of, param_of, tasks_idle};

    /// `size` bytes that begin as a PNG.
    fn image(size: usize) -> Vec<u8> {
        let mut bytes = PNG.to_vec();
        bytes.resize(size, 7);
        bytes
    }

    fn b64(bytes: &[u8]) -> String {
        STANDARD.encode(bytes)
    }

    /// A connection's uploads under a budget of `max` bytes of its own.
    fn small(max: usize) -> (ImageUploads, UploadBudget) {
        let budget = UploadBudget::new(max);
        (Uploads::images(budget.clone()), budget)
    }

    /// An upload of `bytes` created and completed in parts of `part`, ready to be taken.
    fn completed(f: &Fixture, uploads: &ImageUploads, bytes: &[u8], part: usize) -> String {
        let upload = uploads.create(&f.state, &f.id, bytes.len() as u64).unwrap();
        for (n, chunk) in bytes.chunks(part).enumerate() {
            uploads
                .write(&upload, (n * part) as u64, &b64(chunk))
                .unwrap();
        }
        upload
    }

    fn is_plain(err: &anyhow::Error) -> bool {
        err.downcast_ref::<CodedError>().is_none()
    }

    #[tokio::test]
    async fn an_upload_of_a_running_session_gets_a_handle_and_each_one_its_own() {
        let f = fixture(true);
        let (uploads, _) = small(1000);
        let first = uploads.create(&f.state, &f.id, 100).unwrap();
        let second = uploads.create(&f.state, &f.id, 100).unwrap();
        assert!(!first.is_empty());
        assert_ne!(first, second);
        assert_eq!(uploads.len(), 2);
    }

    #[tokio::test]
    async fn an_upload_for_an_unknown_session_or_one_not_running_is_refused_as_such() {
        let f = fixture(true);
        let (uploads, budget) = small(1000);
        let err = uploads.create(&f.state, "no-such-session", 10).unwrap_err();
        assert_eq!(code_of(&err), error_code::UNKNOWN_SESSION);
        let stopped = fixture(false);
        let err = uploads.create(&stopped.state, &stopped.id, 10).unwrap_err();
        assert_eq!(code_of(&err), error_code::SESSION_NOT_RUNNING);
        assert_eq!(uploads.len(), 0);
        assert_eq!(budget.available(), 1000);
    }

    #[tokio::test]
    async fn a_size_of_nothing_or_over_the_image_limit_is_refused_and_holds_nothing() {
        let f = fixture(true);
        let budget = UploadBudget::new(MAX_UPLOAD_BYTES);
        let uploads = Uploads::images(budget.clone());
        for size in [0, MAX_BYTES as u64 + 1, u64::MAX] {
            let err = uploads.create(&f.state, &f.id, size).unwrap_err();
            assert!(is_plain(&err), "size {size}: {err:#}");
        }
        assert_eq!(uploads.len(), 0);
        assert_eq!(budget.available(), MAX_UPLOAD_BYTES);
        // The limit itself, and the least there is, are taken.
        uploads.create(&f.state, &f.id, MAX_BYTES as u64).unwrap();
        uploads.create(&f.state, &f.id, 1).unwrap();
    }

    #[tokio::test]
    async fn a_third_upload_on_one_connection_is_refused_until_one_is_gone() {
        let f = fixture(true);
        let (uploads, _) = small(1000);
        let first = uploads.create(&f.state, &f.id, 10).unwrap();
        uploads.create(&f.state, &f.id, 10).unwrap();
        let err = uploads.create(&f.state, &f.id, 10).unwrap_err();
        assert_eq!(code_of(&err), error_code::LIMIT_EXCEEDED);
        assert_eq!(param_of(&err, "limit"), "image_uploads");
        assert_eq!(param_of(&err, "max"), "2");
        assert_eq!(uploads.len(), 2);

        uploads.delete(&first);
        uploads.create(&f.state, &f.id, 10).unwrap();
    }

    #[tokio::test]
    async fn the_declared_sizes_of_every_connections_uploads_share_the_daemons_budget() {
        let f = fixture(true);
        let budget = UploadBudget::new(25);
        let (one, two, three) = (
            Uploads::images(budget.clone()),
            Uploads::images(budget.clone()),
            Uploads::images(budget.clone()),
        );
        one.create(&f.state, &f.id, 10).unwrap();
        two.create(&f.state, &f.id, 10).unwrap();
        assert_eq!(budget.available(), 5);
        let err = three.create(&f.state, &f.id, 6).unwrap_err();
        assert_eq!(code_of(&err), error_code::LIMIT_EXCEEDED);
        assert_eq!(param_of(&err, "limit"), "image_upload_bytes");
        assert_eq!(param_of(&err, "max"), "25");
        assert_eq!(three.len(), 0);
        assert_eq!(budget.available(), 5, "a refusal holds nothing");
        // Exactly what is left is taken.
        three.create(&f.state, &f.id, 5).unwrap();
        assert_eq!(budget.available(), 0);
    }

    #[tokio::test]
    async fn the_daemons_budget_is_sixty_four_mebibytes_of_declared_sizes() {
        let f = fixture(true);
        let budget = UploadBudget::new(MAX_UPLOAD_BYTES);
        let connections: Vec<ImageUploads> =
            (0..3).map(|_| Uploads::images(budget.clone())).collect();
        for uploads in &connections {
            uploads.create(&f.state, &f.id, MAX_BYTES as u64).unwrap();
            uploads.create(&f.state, &f.id, MAX_BYTES as u64).unwrap();
        }
        let left = MAX_UPLOAD_BYTES - 6 * MAX_BYTES;
        assert_eq!(budget.available(), left);
        let more = Uploads::images(budget.clone());
        let err = more.create(&f.state, &f.id, left as u64 + 1).unwrap_err();
        assert_eq!(code_of(&err), error_code::LIMIT_EXCEEDED);
        assert_eq!(param_of(&err, "limit"), "image_upload_bytes");
        assert_eq!(param_of(&err, "max"), MAX_UPLOAD_BYTES.to_string());
        more.create(&f.state, &f.id, left as u64).unwrap();
    }

    #[tokio::test]
    async fn parts_in_order_are_taken_as_the_bytes_of_the_image() {
        let f = fixture(true);
        let (uploads, budget) = small(1000);
        let bytes = image(100);
        let upload = completed(&f, &uploads, &bytes, 30);
        assert_eq!(uploads.take(&upload, &f.id).unwrap().body, bytes);
        assert_eq!(uploads.len(), 0);
        assert_eq!(budget.available(), 1000, "taken, it holds nothing");
    }

    #[tokio::test]
    async fn a_part_may_be_the_whole_image_and_parts_of_one_byte_add_up() {
        let f = fixture(true);
        let (uploads, _) = small(1000);
        let bytes = image(20);
        let whole = completed(&f, &uploads, &bytes, 20);
        assert_eq!(uploads.take(&whole, &f.id).unwrap().body, bytes);
        let singly = completed(&f, &uploads, &bytes, 1);
        assert_eq!(uploads.take(&singly, &f.id).unwrap().body, bytes);
    }

    #[tokio::test]
    async fn a_part_of_exactly_the_part_limit_is_taken_and_one_byte_over_discards_the_upload() {
        let f = fixture(true);
        let uploads = Uploads::images(UploadBudget::new(MAX_UPLOAD_BYTES));
        let upload = uploads.create(&f.state, &f.id, MAX_BYTES as u64).unwrap();
        uploads
            .write(&upload, 0, &b64(&vec![1; MAX_PART_BYTES]))
            .unwrap();
        let err = uploads
            .write(
                &upload,
                MAX_PART_BYTES as u64,
                &b64(&vec![1; MAX_PART_BYTES + 1]),
            )
            .unwrap_err();
        assert_eq!(code_of(&err), error_code::LIMIT_EXCEEDED);
        assert_eq!(param_of(&err, "limit"), "upload_part_bytes");
        assert_eq!(param_of(&err, "max"), MAX_PART_BYTES.to_string());
        let gone = uploads
            .write(&upload, MAX_PART_BYTES as u64, &b64(b"x"))
            .unwrap_err();
        assert_eq!(code_of(&gone), error_code::UPLOAD_UNAVAILABLE);
    }

    #[tokio::test]
    async fn a_part_far_over_the_part_limit_is_refused_by_its_size_alone() {
        let f = fixture(true);
        let uploads = Uploads::images(UploadBudget::new(MAX_UPLOAD_BYTES));
        let upload = uploads.create(&f.state, &f.id, MAX_BYTES as u64).unwrap();
        let err = uploads
            .write(&upload, 0, &"A".repeat(MAX_PART_BYTES * 3))
            .unwrap_err();
        assert_eq!(code_of(&err), error_code::LIMIT_EXCEEDED);
        assert_eq!(param_of(&err, "limit"), "upload_part_bytes");
        assert_eq!(uploads.len(), 0);
    }

    #[tokio::test]
    async fn a_part_that_does_not_start_where_the_received_bytes_end_is_refused_and_discards_the_upload(
    ) {
        let f = fixture(true);
        for (received, offset) in [(0usize, 5u64), (10, 5), (10, 11), (10, 0)] {
            let (uploads, budget) = small(1000);
            let upload = uploads.create(&f.state, &f.id, 50).unwrap();
            if received > 0 {
                uploads.write(&upload, 0, &b64(&vec![1; received])).unwrap();
            }
            let err = uploads.write(&upload, offset, &b64(b"abc")).unwrap_err();
            assert_eq!(code_of(&err), error_code::UPLOAD_PARTS_MISMATCH);
            assert_eq!(param_of(&err, "expected_offset"), received.to_string());
            assert_eq!(param_of(&err, "size"), "50");
            assert_eq!(uploads.len(), 0, "received {received}, offset {offset}");
            assert_eq!(budget.available(), 1000);
            let gone = uploads
                .write(&upload, received as u64, &b64(b"abc"))
                .unwrap_err();
            assert_eq!(code_of(&gone), error_code::UPLOAD_UNAVAILABLE);
            let taken = uploads.take(&upload, &f.id).unwrap_err();
            assert_eq!(code_of(&taken), error_code::UPLOAD_UNAVAILABLE);
        }
    }

    #[tokio::test]
    async fn a_part_running_past_the_size_is_refused_and_a_part_ending_at_it_is_not() {
        let f = fixture(true);
        let (uploads, budget) = small(1000);
        let ends_at_size = uploads.create(&f.state, &f.id, 10).unwrap();
        uploads.write(&ends_at_size, 0, &b64(&[1; 4])).unwrap();
        uploads.write(&ends_at_size, 4, &b64(&[1; 6])).unwrap();
        assert_eq!(uploads.take(&ends_at_size, &f.id).unwrap().body.len(), 10);

        let past = uploads.create(&f.state, &f.id, 10).unwrap();
        uploads.write(&past, 0, &b64(&[1; 4])).unwrap();
        let err = uploads.write(&past, 4, &b64(&[1; 7])).unwrap_err();
        assert_eq!(code_of(&err), error_code::UPLOAD_PARTS_MISMATCH);
        assert_eq!(param_of(&err, "expected_offset"), "4");
        assert_eq!(param_of(&err, "size"), "10");
        assert_eq!(uploads.len(), 0);
        assert_eq!(budget.available(), 1000);
    }

    #[tokio::test]
    async fn a_part_sent_twice_is_refused_at_the_second_and_discards_the_upload() {
        let f = fixture(true);
        let (uploads, _) = small(1000);
        let upload = uploads.create(&f.state, &f.id, 20).unwrap();
        uploads.write(&upload, 0, &b64(&[1; 5])).unwrap();
        let err = uploads.write(&upload, 0, &b64(&[1; 5])).unwrap_err();
        assert_eq!(code_of(&err), error_code::UPLOAD_PARTS_MISMATCH);
        assert_eq!(param_of(&err, "expected_offset"), "5");
        assert_eq!(uploads.len(), 0);
    }

    #[tokio::test]
    async fn data_that_is_not_base64_is_refused_and_discards_the_upload() {
        let f = fixture(true);
        let (uploads, budget) = small(1000);
        let upload = uploads.create(&f.state, &f.id, 20).unwrap();
        let err = uploads.write(&upload, 0, "not base64 !!!").unwrap_err();
        assert!(is_plain(&err), "{err:#}");
        assert!(format!("{err:#}").contains("base64"), "{err:#}");
        assert_eq!(uploads.len(), 0);
        assert_eq!(budget.available(), 1000);
        let gone = uploads.write(&upload, 0, &b64(b"abc")).unwrap_err();
        assert_eq!(code_of(&gone), error_code::UPLOAD_UNAVAILABLE);
    }

    #[tokio::test]
    async fn a_part_for_an_upload_this_connection_does_not_hold_is_unavailable() {
        let f = fixture(true);
        let (uploads, _) = small(1000);
        let err = uploads.write("upload-404", 0, &b64(b"abc")).unwrap_err();
        assert_eq!(code_of(&err), error_code::UPLOAD_UNAVAILABLE);
        assert_eq!(param_of(&err, "upload"), "upload-404");
        // Another connection's handle is not this one's.
        let (other, _) = small(1000);
        let theirs = other.create(&f.state, &f.id, 10).unwrap();
        let err = uploads.write(&theirs, 0, &b64(b"abc")).unwrap_err();
        assert_eq!(code_of(&err), error_code::UPLOAD_UNAVAILABLE);
        assert_eq!(other.len(), 1, "the other connection's upload is untouched");
    }

    #[tokio::test]
    async fn taking_an_upload_for_another_session_is_unavailable_and_the_upload_is_gone() {
        let f = fixture(true);
        let (uploads, budget) = small(1000);
        let bytes = image(30);
        let upload = completed(&f, &uploads, &bytes, 30);
        let err = uploads.take(&upload, "another-session").unwrap_err();
        assert_eq!(code_of(&err), error_code::UPLOAD_UNAVAILABLE);
        assert_eq!(param_of(&err, "upload"), upload);
        assert_eq!(uploads.len(), 0);
        assert_eq!(budget.available(), 1000);
        let again = uploads.take(&upload, &f.id).unwrap_err();
        assert_eq!(code_of(&again), error_code::UPLOAD_UNAVAILABLE);
    }

    #[tokio::test]
    async fn taking_an_upload_whose_parts_end_short_is_a_mismatch_and_the_upload_is_gone() {
        let f = fixture(true);
        for received in [0usize, 1, 29] {
            let (uploads, budget) = small(1000);
            let upload = uploads.create(&f.state, &f.id, 30).unwrap();
            if received > 0 {
                uploads.write(&upload, 0, &b64(&vec![1; received])).unwrap();
            }
            let err = uploads.take(&upload, &f.id).unwrap_err();
            assert_eq!(code_of(&err), error_code::UPLOAD_PARTS_MISMATCH);
            assert_eq!(param_of(&err, "expected_offset"), received.to_string());
            assert_eq!(param_of(&err, "size"), "30");
            assert_eq!(uploads.len(), 0, "received {received}");
            assert_eq!(budget.available(), 1000);
            let again = uploads.take(&upload, &f.id).unwrap_err();
            assert_eq!(code_of(&again), error_code::UPLOAD_UNAVAILABLE);
        }
    }

    #[tokio::test]
    async fn taking_an_upload_that_was_never_held_is_unavailable() {
        let f = fixture(true);
        let (uploads, _) = small(1000);
        let err = uploads.take("upload-404", &f.id).unwrap_err();
        assert_eq!(code_of(&err), error_code::UPLOAD_UNAVAILABLE);
    }

    #[tokio::test]
    async fn a_deleted_upload_is_gone_and_gives_its_bytes_back_and_deleting_again_is_harmless() {
        let f = fixture(true);
        let (uploads, budget) = small(1000);
        let upload = uploads.create(&f.state, &f.id, 100).unwrap();
        uploads.write(&upload, 0, &b64(&[1; 10])).unwrap();
        assert_eq!(budget.available(), 900);
        uploads.delete(&upload);
        uploads.delete(&upload);
        uploads.delete("upload-404");
        assert_eq!(uploads.len(), 0);
        assert_eq!(budget.available(), 1000);
        let err = uploads.write(&upload, 10, &b64(b"abc")).unwrap_err();
        assert_eq!(code_of(&err), error_code::UPLOAD_UNAVAILABLE);
    }

    #[tokio::test]
    async fn dropping_a_connections_uploads_gives_back_every_byte_they_held() {
        let f = fixture(true);
        let budget = UploadBudget::new(1000);
        let uploads = Uploads::images(budget.clone());
        let survivor = Uploads::images(budget.clone());
        uploads.create(&f.state, &f.id, 100).unwrap();
        let written = uploads.create(&f.state, &f.id, 200).unwrap();
        uploads.write(&written, 0, &b64(&[1; 50])).unwrap();
        survivor.create(&f.state, &f.id, 300).unwrap();
        assert_eq!(budget.available(), 400);
        drop(uploads);
        assert_eq!(
            budget.available(),
            700,
            "only the dropped connection's bytes came back"
        );
        assert_eq!(survivor.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn an_upload_is_dropped_after_the_idle_time_from_its_last_part() {
        let f = fixture(true);
        let budget = UploadBudget::new(1000);
        let uploads = Uploads::images(budget.clone());
        let second = std::time::Duration::from_secs(1);
        let upload = uploads.create(&f.state, &f.id, 100).unwrap();
        tokio::time::advance(IDLE_EXPIRY - second).await;
        uploads.write(&upload, 0, &b64(&[1; 10])).unwrap();

        // Past the time since it was created, short of the time since its last part.
        tokio::time::advance(IDLE_EXPIRY - second).await;
        tasks_idle().await;
        assert_eq!(uploads.len(), 1);
        assert_eq!(budget.available(), 900);

        // A part arriving moves the expiry on again.
        uploads.write(&upload, 10, &b64(&[1; 10])).unwrap();
        tokio::time::advance(IDLE_EXPIRY - second).await;
        tasks_idle().await;
        assert_eq!(uploads.len(), 1);

        // Past the idle time with no part: gone, its bytes back.
        tokio::time::advance(second * 2).await;
        tasks_idle().await;
        assert_eq!(uploads.len(), 0);
        assert_eq!(budget.available(), 1000);
        let err = uploads.write(&upload, 20, &b64(b"abc")).unwrap_err();
        assert_eq!(code_of(&err), error_code::UPLOAD_UNAVAILABLE);
        let err = uploads.take(&upload, &f.id).unwrap_err();
        assert_eq!(code_of(&err), error_code::UPLOAD_UNAVAILABLE);
    }

    #[tokio::test(start_paused = true)]
    async fn uploads_taken_deleted_or_left_to_expire_all_end_up_gone_with_their_bytes_back() {
        let f = fixture(true);
        let budget = UploadBudget::new(1000);
        let uploads = Uploads::images(budget.clone());
        let bytes = image(20);
        let taken = completed(&f, &uploads, &bytes, 20);
        assert_eq!(uploads.take(&taken, &f.id).unwrap().body, bytes);
        let deleted = uploads.create(&f.state, &f.id, 10).unwrap();
        uploads.delete(&deleted);
        let kept = uploads.create(&f.state, &f.id, 10).unwrap();
        tokio::time::advance(IDLE_EXPIRY * 2).await;
        tasks_idle().await;
        assert_eq!(uploads.len(), 0, "{kept}");
        assert_eq!(budget.available(), 1000);
    }

    #[tokio::test(start_paused = true)]
    async fn an_upload_that_is_never_saved_leaves_nothing_on_disk() {
        let f = fixture(true);
        let uploads = Uploads::images(UploadBudget::new(MAX_UPLOAD_BYTES));
        let scratch = f.run.join(&f.id);
        let nothing_there = || {
            std::fs::read_dir(&scratch).unwrap().next().is_none()
                && !pasted_directory(&f, &f.id).exists()
        };
        assert!(nothing_there());

        // Written and taken, but never saved.
        let bytes = image(PNG.len() + 40);
        let taken = completed(&f, &uploads, &bytes, 16);
        assert!(nothing_there(), "parts are held in memory");
        uploads.take(&taken, &f.id).unwrap();
        assert!(nothing_there());

        // Deleted, refused, expired and dropped with its connection.
        let deleted = completed(&f, &uploads, &bytes, 16);
        uploads.delete(&deleted);
        let refused = uploads.create(&f.state, &f.id, 50).unwrap();
        uploads.write(&refused, 3, &b64(b"abc")).unwrap_err();
        let expiring = completed(&f, &uploads, &bytes, 16);
        tokio::time::advance(IDLE_EXPIRY * 2).await;
        tasks_idle().await;
        assert_eq!(uploads.len(), 0, "{expiring}");
        let last = completed(&f, &uploads, &bytes, 16);
        drop(uploads);
        assert!(nothing_there(), "{last}");
    }

    #[tokio::test]
    async fn the_bytes_taken_stay_counted_against_the_budget_until_they_are_dropped() {
        let f = fixture(true);
        let (uploads, budget) = small(1000);
        let bytes = image(100);
        let upload = completed(&f, &uploads, &bytes, 30);
        assert_eq!(budget.available(), 900);
        let taken = uploads.take(&upload, &f.id).unwrap();
        assert_eq!(taken.body, bytes);
        assert_eq!(uploads.len(), 0, "the upload itself is gone");
        assert_eq!(budget.available(), 900, "its bytes are still held");
        // Held bytes keep another upload from taking their share.
        let other = Uploads::images(budget.clone());
        let err = other.create(&f.state, &f.id, 901).unwrap_err();
        assert_eq!(param_of(&err, "limit"), "image_upload_bytes");
        drop(taken);
        assert_eq!(budget.available(), 1000);
        other.create(&f.state, &f.id, 901).unwrap();
    }
}
