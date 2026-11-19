//! A pasted image uploaded in parts, for a client on a link too slow to send a 10 MiB image in one
//! frame before the control socket's silence limit (see "The heartbeat" in
//! `apps/daemon/PROTOCOL.md`).
//!
//! `create_pasted_image_upload` names the session and the size, each `write_pasted_image_part`
//! adds the next part, and `save_pasted_image` with the upload's handle writes the file as the
//! single-frame request does. The parts are held in memory by the connection that sent them and
//! nothing reaches the disk before that save, so an upload that is never finished — deleted,
//! refused, left to expire, or cut off with its connection — leaves nothing behind.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;

use super::MAX_BYTES;
use crate::protocol::{error_code, CodedError};
use crate::state::AppState;

/// Uploads one connection may hold at once.
pub const MAX_UPLOADS_PER_CONNECTION: usize = 2;

/// The declared sizes of every connection's uploads together, held in memory until each is saved
/// or dropped.
pub const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;

/// The most one part carries, decoded: about 1.4 MB in its frame.
pub const MAX_PART_BYTES: usize = 1024 * 1024;

/// How long an upload is kept with no part arriving, counted from its creation or its latest part.
pub const IDLE_EXPIRY: Duration = Duration::from_secs(60);

/// The daemon-wide bytes uploads may hold, one per daemon (`AppState::pasted_image_uploads`).
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

impl Default for UploadBudget {
    fn default() -> Self {
        Self::new(MAX_UPLOAD_BYTES)
    }
}

/// One connection's uploads. Dropped with the connection, which drops every upload it held.
#[derive(Debug)]
pub struct Uploads {
    budget: UploadBudget,
    next: AtomicU64,
    held: Arc<Mutex<HashMap<String, Upload>>>,
}

#[derive(Debug)]
struct Upload {
    session: String,
    size: usize,
    bytes: Vec<u8>,
    /// When the upload expires unless another part arrives.
    expires: Instant,
    _budget: OwnedSemaphorePermit,
}

impl Uploads {
    pub fn new(budget: UploadBudget) -> Self {
        Self {
            budget,
            next: AtomicU64::new(0),
            held: Arc::default(),
        }
    }

    /// Starts an upload of `size` bytes for the running session `session_id` and returns its
    /// handle. Refused with `unknown_session` or `session_not_running`, a size of nothing or over
    /// [`MAX_BYTES`] as a plain error, and `limit_exceeded` past the uploads this connection or the
    /// daemon may hold.
    pub fn create(&self, state: &AppState, session_id: &str, size: u64) -> Result<String> {
        super::scratch_of(state, session_id)?;
        if size == 0 {
            bail!("an image upload cannot be empty");
        }
        if size > MAX_BYTES as u64 {
            bail!("the image is larger than {MAX_BYTES} bytes");
        }
        let size = size as usize;
        let mut held = self.held.lock().expect("uploads lock poisoned");
        if held.len() >= MAX_UPLOADS_PER_CONNECTION {
            return Err(limit_exceeded(
                "too many image uploads are under way on this connection",
                "image_uploads",
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
                "the image uploads under way hold too many bytes",
                "image_upload_bytes",
                self.budget.max,
            ));
        };
        let upload = format!("upload-{}", self.next.fetch_add(1, Ordering::Relaxed));
        let expires = Instant::now() + IDLE_EXPIRY;
        held.insert(
            upload.clone(),
            Upload {
                session: session_id.to_string(),
                size,
                bytes: Vec::with_capacity(size),
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
    /// over [`MAX_PART_BYTES`], a plain error for data that is not base64, and
    /// `upload_parts_mismatch` for a part that does not start where the received bytes end or runs
    /// past the upload's size.
    pub fn write(&self, upload: &str, offset: u64, data: &str) -> Result<()> {
        let mut held = self.held.lock().expect("uploads lock poisoned");
        let Some(entry) = held.get_mut(upload) else {
            return Err(unavailable(upload));
        };
        let refusal = match part_of(data) {
            Ok(part) => {
                let received = entry.bytes.len();
                if offset != received as u64 || received + part.len() > entry.size {
                    Err(mismatch(received, entry.size))
                } else {
                    entry.bytes.extend_from_slice(&part);
                    entry.expires = Instant::now() + IDLE_EXPIRY;
                    return Ok(());
                }
            }
            Err(err) => Err(err),
        };
        held.remove(upload);
        refusal
    }

    /// Discards the upload, if this connection holds it.
    pub fn delete(&self, upload: &str) {
        self.held
            .lock()
            .expect("uploads lock poisoned")
            .remove(upload);
    }

    /// Takes the upload's bytes for a save to the session `session_id`, which must be the one it
    /// was created for: refused with `upload_unavailable` otherwise, or for an upload this
    /// connection does not hold, and with `upload_parts_mismatch` when the parts end short of its
    /// size. The upload is gone either way; its bytes stay counted against the budget for as long
    /// as the [`Taken`] holding them.
    pub fn take(&self, upload: &str, session_id: &str) -> Result<Taken> {
        let entry = self
            .held
            .lock()
            .expect("uploads lock poisoned")
            .remove(upload)
            .ok_or_else(|| unavailable(upload))?;
        if entry.session != session_id {
            return Err(unavailable(upload));
        }
        if entry.bytes.len() != entry.size {
            return Err(mismatch(entry.bytes.len(), entry.size));
        }
        Ok(Taken {
            bytes: entry.bytes,
            _budget: entry._budget,
        })
    }

    /// The uploads this connection holds.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.held.lock().expect("uploads lock poisoned").len()
    }
}

/// An upload's bytes, taken for its save, with their share of the budget.
#[derive(Debug)]
pub struct Taken {
    pub bytes: Vec<u8>,
    _budget: OwnedSemaphorePermit,
}

/// Drops `upload` once it has gone `idle` with no part, checking again whenever a part has moved
/// its expiry on; ends as soon as the upload or its connection's uploads are gone.
fn expire_when_idle(held: Weak<Mutex<HashMap<String, Upload>>>, upload: String, first: Instant) {
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
                held.remove(&upload);
                return;
            }
            at = entry.expires;
        }
    });
}

/// The bytes of one part, or why it is refused.
fn part_of(data: &str) -> Result<Vec<u8>> {
    if data.len() > MAX_PART_BYTES.div_ceil(3) * 4 {
        return Err(part_too_large());
    }
    let part = STANDARD
        .decode(data)
        .context("the image part is not valid base64")?;
    if part.len() > MAX_PART_BYTES {
        return Err(part_too_large());
    }
    Ok(part)
}

fn part_too_large() -> anyhow::Error {
    limit_exceeded(
        "the image part is larger than one part may be",
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

fn unavailable(upload: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::UPLOAD_UNAVAILABLE,
        "this connection holds no such image upload",
        &[("upload", upload)],
    )
}

fn mismatch(received: usize, size: usize) -> anyhow::Error {
    CodedError::raised(
        error_code::UPLOAD_PARTS_MISMATCH,
        "the image parts do not add up to the upload",
        &[
            ("expected_offset", &received.to_string()),
            ("size", &size.to_string()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::pasted_image::tests::{fixture, pasted_directory, Fixture, PNG};
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
    fn small(max: usize) -> (Uploads, UploadBudget) {
        let budget = UploadBudget::new(max);
        (Uploads::new(budget.clone()), budget)
    }

    /// An upload of `bytes` created and completed in parts of `part`, ready to be taken.
    fn completed(f: &Fixture, uploads: &Uploads, bytes: &[u8], part: usize) -> String {
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
        let budget = UploadBudget::default();
        let uploads = Uploads::new(budget.clone());
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
            Uploads::new(budget.clone()),
            Uploads::new(budget.clone()),
            Uploads::new(budget.clone()),
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
        let budget = UploadBudget::default();
        let connections: Vec<Uploads> = (0..3).map(|_| Uploads::new(budget.clone())).collect();
        for uploads in &connections {
            uploads.create(&f.state, &f.id, MAX_BYTES as u64).unwrap();
            uploads.create(&f.state, &f.id, MAX_BYTES as u64).unwrap();
        }
        let left = MAX_UPLOAD_BYTES - 6 * MAX_BYTES;
        assert_eq!(budget.available(), left);
        let more = Uploads::new(budget.clone());
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
        assert_eq!(uploads.take(&upload, &f.id).unwrap().bytes, bytes);
        assert_eq!(uploads.len(), 0);
        assert_eq!(budget.available(), 1000, "taken, it holds nothing");
    }

    #[tokio::test]
    async fn a_part_may_be_the_whole_image_and_parts_of_one_byte_add_up() {
        let f = fixture(true);
        let (uploads, _) = small(1000);
        let bytes = image(20);
        let whole = completed(&f, &uploads, &bytes, 20);
        assert_eq!(uploads.take(&whole, &f.id).unwrap().bytes, bytes);
        let singly = completed(&f, &uploads, &bytes, 1);
        assert_eq!(uploads.take(&singly, &f.id).unwrap().bytes, bytes);
    }

    #[tokio::test]
    async fn a_part_of_exactly_the_part_limit_is_taken_and_one_byte_over_discards_the_upload() {
        let f = fixture(true);
        let uploads = Uploads::new(UploadBudget::default());
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
        let uploads = Uploads::new(UploadBudget::default());
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
        assert_eq!(uploads.take(&ends_at_size, &f.id).unwrap().bytes.len(), 10);

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
        let uploads = Uploads::new(budget.clone());
        let survivor = Uploads::new(budget.clone());
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
        let uploads = Uploads::new(budget.clone());
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
        let uploads = Uploads::new(budget.clone());
        let bytes = image(20);
        let taken = completed(&f, &uploads, &bytes, 20);
        assert_eq!(uploads.take(&taken, &f.id).unwrap().bytes, bytes);
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
        let uploads = Uploads::new(UploadBudget::default());
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
        assert_eq!(taken.bytes, bytes);
        assert_eq!(uploads.len(), 0, "the upload itself is gone");
        assert_eq!(budget.available(), 900, "its bytes are still held");
        // Held bytes keep another upload from taking their share.
        let other = Uploads::new(budget.clone());
        let err = other.create(&f.state, &f.id, 901).unwrap_err();
        assert_eq!(param_of(&err, "limit"), "image_upload_bytes");
        drop(taken);
        assert_eq!(budget.available(), 1000);
        other.create(&f.state, &f.id, 901).unwrap();
    }
}
