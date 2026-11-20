//! Images the user pastes into a session's terminal, kept as files its agent can read by path.
//!
//! An agent's own way of taking an image from the clipboard works only where the daemon runs on the
//! machine the clipboard is on, and only for some agents; a pasted image file's path is taken as an
//! attached image by all of them, wherever the daemon runs. So the window sends the image's bytes
//! (`save_pasted_image`), the daemon writes them under the session's scratch directory, and the
//! window pastes the path it is answered with. The file goes away with the scratch directory, when
//! the session's process ends; the agent has read the image into its conversation by then.
//!
//! On a slow link the image can instead come in parts (`crate::session::upload`), so that no
//! single frame takes longer than the control socket's silence limit to arrive; the parts are held
//! in memory and nothing reaches the disk before the save.
//!
//! The name is made only of characters a path detection in an agent does not split on: no space,
//! and an extension that says what the bytes are, which is what an agent goes by.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use super::scratch::{scratch_of, subdirectory};
use super::upload::{Kind, UploadBudget, Uploads};
use crate::hostfs::replace::{replace_beside, Permissions};
use crate::state::AppState;

/// The largest image taken, in bytes. The single-frame request carries it base64-encoded in one
/// frame, which a WebSocket message holds at most 16 MiB of, so this stays under three quarters of
/// that; an upload in parts is held to the same limit.
pub const MAX_BYTES: usize = 10 * 1024 * 1024;

/// The declared sizes of every connection's image uploads together, held in memory until each is
/// saved or dropped.
pub const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;

/// What sets image uploads apart.
static IMAGE: Kind = Kind {
    noun: "image",
    max_bytes: MAX_BYTES,
    handle_prefix: "upload",
    uploads_limit: "image_uploads",
    bytes_limit: "image_upload_bytes",
};

/// One connection's image uploads, their parts held in memory.
pub type ImageUploads = Uploads<Vec<u8>>;

impl Uploads<Vec<u8>> {
    /// A connection's image uploads, under the daemon's image budget `budget`.
    pub fn images(budget: UploadBudget) -> Self {
        Self::of_kind(&IMAGE, budget)
    }

    /// Starts an image upload of `size` bytes for the running session `session_id`; see
    /// [`Uploads::create_with`].
    pub fn create(&self, state: &AppState, session_id: &str, size: u64) -> Result<String> {
        self.create_with(state, session_id, size, |_| {
            Ok(Vec::with_capacity(size as usize))
        })
    }
}

impl UploadBudget {
    /// The daemon's budget of image uploads, [`MAX_UPLOAD_BYTES`].
    pub fn images() -> Self {
        Self::new(MAX_UPLOAD_BYTES)
    }
}

/// The directory under a session's scratch directory the images go in.
const DIRECTORY: &str = "pasted";

/// Distinguishes two images saved within one millisecond.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// Writes the image `data` (base64) holds for the running session `session_id`, and returns the
/// file's absolute path. Refused with `unknown_session` or `session_not_running`; an image over
/// [`MAX_BYTES`], one that is not a PNG, JPEG, GIF or WebP, or a write that fails is an error the
/// window shows as it is: it limits the size before sending and sends only those formats.
/// **Blocks.**
pub fn save(state: &AppState, session_id: &str, data: &str) -> Result<PathBuf> {
    let scratch = scratch_of(state, session_id)?;
    // Four characters hold three bytes, so a longer text is over the limit whatever it decodes to.
    if data.len() > MAX_BYTES.div_ceil(3) * 4 {
        bail!("the image is larger than {MAX_BYTES} bytes");
    }
    let bytes = STANDARD
        .decode(data)
        .context("the image is not valid base64")?;
    write_image(session_id, &scratch, &bytes)
}

/// [`save`] for an image already decoded, as an upload's parts hold it. **Blocks.**
pub fn save_bytes(state: &AppState, session_id: &str, bytes: &[u8]) -> Result<PathBuf> {
    let scratch = scratch_of(state, session_id)?;
    write_image(session_id, &scratch, bytes)
}

/// Writes `bytes` as an image file under `scratch`, the session's scratch directory.
fn write_image(session_id: &str, scratch: &std::path::Path, bytes: &[u8]) -> Result<PathBuf> {
    if bytes.len() > MAX_BYTES {
        bail!("the image is larger than {MAX_BYTES} bytes");
    }
    let Some(extension) = extension_of(bytes) else {
        bail!("the image is not a PNG, JPEG, GIF or WebP file");
    };
    let directory = subdirectory(session_id, scratch, DIRECTORY)?;
    let path = directory.join(format!(
        "paste-{}-{}.{extension}",
        crate::time::now_millis(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    // The owner's alone, like everything else in the scratch directory.
    replace_beside(&path, bytes, Permissions::Private)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// The extension of the format `bytes` begin as, from its signature.
fn extension_of(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    use super::{save, save_bytes, MAX_BYTES};
    use crate::protocol::{error_code, Event, RequestBody};
    use crate::session::test_support::{fixture, pasted_directory, Fixture, PNG};
    use crate::test_support::code_of;

    const JPEG: &[u8] = b"\xff\xd8\xff\xe0rest of the image";
    const GIF_87: &[u8] = b"GIF87arest of the image";
    const GIF_89: &[u8] = b"GIF89arest of the image";
    const WEBP: &[u8] = b"RIFF\x10\x00\x00\x00WEBPVP8 rest";

    fn encoded(bytes: &[u8]) -> String {
        STANDARD.encode(bytes)
    }

    fn extension(path: &Path) -> String {
        path.extension().unwrap().to_string_lossy().into_owned()
    }

    #[tokio::test]
    async fn an_image_is_saved_as_a_file_of_the_bytes_sent() {
        let f = fixture(true);
        let path = save(&f.state, &f.id, &encoded(PNG)).unwrap();
        assert!(path.is_absolute(), "{path:?}");
        assert_eq!(std::fs::read(&path).unwrap(), PNG);
        assert_eq!(path.parent().unwrap(), pasted_directory(&f, &f.id));
    }

    #[tokio::test]
    async fn already_decoded_bytes_are_saved_as_the_same_file_save_writes() {
        let f = fixture(true);
        let path = save_bytes(&f.state, &f.id, JPEG).unwrap();
        assert!(path.is_absolute(), "{path:?}");
        assert_eq!(extension(&path), "jpg");
        assert_eq!(std::fs::read(&path).unwrap(), JPEG);
        assert_eq!(path.parent().unwrap(), pasted_directory(&f, &f.id));
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("paste-"), "{name:?}");
        assert!(!name.chars().any(char::is_whitespace), "{name:?}");
        let other = save_bytes(&f.state, &f.id, JPEG).unwrap();
        assert_ne!(path, other);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bytes_saved_as_decoded_are_the_owners_alone() {
        use std::os::unix::fs::PermissionsExt;
        let f = fixture(true);
        let path = save_bytes(&f.state, &f.id, PNG).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[tokio::test]
    async fn bytes_of_exactly_the_limit_are_taken_and_one_byte_over_is_not() {
        let f = fixture(true);
        let mut at_limit = PNG.to_vec();
        at_limit.resize(MAX_BYTES, 0);
        let path = save_bytes(&f.state, &f.id, &at_limit).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), MAX_BYTES as u64);

        let mut over = PNG.to_vec();
        over.resize(MAX_BYTES + 1, 0);
        let refused = save_bytes(&f.state, &f.id, &over).unwrap_err();
        assert!(format!("{refused:#}").contains("larger"), "{refused:#}");
    }

    #[tokio::test]
    async fn bytes_that_are_none_of_the_four_formats_are_refused_and_leave_no_file() {
        let f = fixture(true);
        for bytes in [
            &b""[..],
            b"just some text",
            b"\x89PNG",
            b"RIFF\x10\x00\x00\x00WEB",
        ] {
            let err = save_bytes(&f.state, &f.id, bytes).unwrap_err();
            assert!(
                err.downcast_ref::<crate::protocol::CodedError>().is_none(),
                "{bytes:?} is refused as a plain error: {err:#}"
            );
        }
        assert!(!pasted_directory(&f, &f.id).exists());
    }

    #[tokio::test]
    async fn bytes_for_a_session_that_is_unknown_or_not_running_are_refused_as_such() {
        let f = fixture(true);
        let err = save_bytes(&f.state, "no-such-session", PNG).unwrap_err();
        assert_eq!(code_of(&err), error_code::UNKNOWN_SESSION);
        let stopped = fixture(false);
        let err = save_bytes(&stopped.state, &stopped.id, PNG).unwrap_err();
        assert_eq!(code_of(&err), error_code::SESSION_NOT_RUNNING);
        assert!(!pasted_directory(&stopped, &stopped.id).exists());
    }

    #[tokio::test]
    async fn bytes_saved_for_a_scratch_directory_removed_since_are_not_made_again() {
        let f = fixture(true);
        std::fs::remove_dir_all(f.run.join(&f.id)).unwrap();
        let err = save_bytes(&f.state, &f.id, PNG).unwrap_err();
        assert_eq!(code_of(&err), error_code::SESSION_NOT_RUNNING);
        assert!(!f.run.join(&f.id).exists());
    }

    #[tokio::test]
    async fn the_extension_is_the_one_the_file_signature_names() {
        let f = fixture(true);
        for (bytes, expected) in [
            (PNG, "png"),
            (JPEG, "jpg"),
            (GIF_87, "gif"),
            (GIF_89, "gif"),
            (WEBP, "webp"),
        ] {
            let path = save(&f.state, &f.id, &encoded(bytes)).unwrap();
            assert_eq!(extension(&path), expected);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
    }

    #[tokio::test]
    async fn a_name_has_no_whitespace_and_two_pastes_never_share_one() {
        let f = fixture(true);
        let paths: Vec<PathBuf> = (0..20)
            .map(|_| save(&f.state, &f.id, &encoded(PNG)).unwrap())
            .collect();
        for path in &paths {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            assert!(!name.chars().any(char::is_whitespace), "{name:?}");
            assert!(path.exists(), "{path:?}");
        }
        let distinct: std::collections::HashSet<_> = paths.iter().collect();
        assert_eq!(distinct.len(), paths.len(), "{paths:?}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_file_is_the_owners_alone() {
        use std::os::unix::fs::PermissionsExt;
        let f = fixture(true);
        let path = save(&f.state, &f.id, &encoded(PNG)).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[tokio::test]
    async fn an_image_of_exactly_the_limit_is_taken_and_one_byte_over_is_not() {
        let f = fixture(true);
        let mut at_limit = PNG.to_vec();
        at_limit.resize(MAX_BYTES, 0);
        let path = save(&f.state, &f.id, &encoded(&at_limit)).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), MAX_BYTES as u64);

        let mut over = PNG.to_vec();
        over.resize(MAX_BYTES + 1, 0);
        let refused = save(&f.state, &f.id, &encoded(&over)).unwrap_err();
        assert!(format!("{refused:#}").contains("larger"), "{refused:#}");
    }

    #[tokio::test]
    async fn data_far_over_the_limit_is_refused() {
        let f = fixture(true);
        let refused = save(&f.state, &f.id, &"A".repeat(MAX_BYTES * 2)).unwrap_err();
        assert!(format!("{refused:#}").contains("larger"), "{refused:#}");
        assert!(!pasted_directory(&f, &f.id).exists());
    }

    #[tokio::test]
    async fn data_that_is_not_base64_is_refused_and_leaves_no_file() {
        let f = fixture(true);
        let refused = save(&f.state, &f.id, "not base64 !!!").unwrap_err();
        assert!(format!("{refused:#}").contains("base64"), "{refused:#}");
        assert!(!pasted_directory(&f, &f.id).exists());
    }

    #[tokio::test]
    async fn data_that_is_none_of_the_four_formats_is_refused_and_leaves_no_file() {
        let f = fixture(true);
        let refused: [&[u8]; 6] = [
            b"",
            b"just some text",
            b"\x89PNG",
            b"GIF90a rest of the image",
            b"RIFF\x10\x00\x00\x00WAVEfmt rest",
            b"RIFF\x10\x00\x00\x00WEB",
        ];
        for bytes in refused {
            let err = save(&f.state, &f.id, &encoded(bytes)).unwrap_err();
            assert!(
                err.downcast_ref::<crate::protocol::CodedError>().is_none(),
                "{bytes:?} is refused as a plain error: {err:#}"
            );
        }
        assert!(!pasted_directory(&f, &f.id).exists());
    }

    #[tokio::test]
    async fn an_unknown_session_is_refused_as_such() {
        let f = fixture(true);
        let err = save(&f.state, "no-such-session", &encoded(PNG)).unwrap_err();
        assert_eq!(code_of(&err), error_code::UNKNOWN_SESSION);
        assert!(!pasted_directory(&f, "no-such-session").exists());
    }

    #[tokio::test]
    async fn a_session_that_is_not_running_is_refused_as_such() {
        let f = fixture(false);
        let err = save(&f.state, &f.id, &encoded(PNG)).unwrap_err();
        assert_eq!(code_of(&err), error_code::SESSION_NOT_RUNNING);
        assert!(!pasted_directory(&f, &f.id).exists());
    }

    #[tokio::test]
    async fn a_scratch_directory_removed_since_the_check_is_not_made_again() {
        let f = fixture(true);
        std::fs::remove_dir_all(f.run.join(&f.id)).unwrap();
        let err = save(&f.state, &f.id, &encoded(PNG)).unwrap_err();
        assert_eq!(code_of(&err), error_code::SESSION_NOT_RUNNING);
        assert!(!f.run.join(&f.id).exists());
    }

    #[tokio::test]
    async fn a_session_whose_process_has_ended_is_refused_as_not_running_and_nothing_is_made_again()
    {
        let mut f = fixture(true);
        let scratch = f.run.join(&f.id);
        // The process ends and takes its scratch directory with it.
        drop(f.live.take());
        let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
        while scratch.exists() && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(
            !scratch.exists(),
            "the stand-in's scratch directory was removed"
        );
        let err = save(&f.state, &f.id, &encoded(PNG)).unwrap_err();
        assert_eq!(code_of(&err), error_code::SESSION_NOT_RUNNING);
        assert!(!scratch.exists());
    }

    #[tokio::test]
    async fn only_the_pasted_directory_is_added_to_the_scratch_directory() {
        let f = fixture(true);
        let scratch = f.run.join(&f.id);
        let before: Vec<_> = std::fs::read_dir(&scratch)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(before.is_empty(), "{before:?}");
        save(&f.state, &f.id, &encoded(PNG)).unwrap();
        let after: Vec<_> = std::fs::read_dir(&scratch)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(after, vec![std::ffi::OsString::from("pasted")]);
    }

    #[tokio::test]
    async fn the_request_is_answered_with_its_id_and_the_absolute_path_of_the_saved_file() {
        let f = fixture(true);
        let reply = crate::coordinator::handle(
            &f.state,
            Some("request-7".to_string()),
            RequestBody::SavePastedImage {
                session: f.id.clone(),
                data: Some(encoded(JPEG)),
                upload: None,
            },
        )
        .await
        .unwrap();
        let Some(Event::PastedImageSaved { id, path }) = reply else {
            panic!("pasted_image_saved: {reply:?}");
        };
        assert_eq!(id.as_deref(), Some("request-7"));
        let path = PathBuf::from(path);
        assert!(path.is_absolute(), "{path:?}");
        assert_eq!(extension(&path), "jpg");
        assert_eq!(std::fs::read(&path).unwrap(), JPEG);
    }

    #[tokio::test]
    async fn the_request_for_a_session_that_is_not_running_is_refused_with_its_code() {
        let f = fixture(false);
        let err = crate::coordinator::handle(
            &f.state,
            None,
            RequestBody::SavePastedImage {
                session: f.id.clone(),
                data: Some(encoded(PNG)),
                upload: None,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(code_of(&err), error_code::SESSION_NOT_RUNNING);
    }

    async fn handled(f: &Fixture, body: RequestBody) -> anyhow::Result<Option<Event>> {
        crate::coordinator::handle(&f.state, Some("request-1".to_string()), body).await
    }

    #[tokio::test]
    async fn the_request_with_an_upload_is_refused_where_the_parts_are_not_held_and_writes_nothing()
    {
        let f = fixture(true);
        let err = handled(
            &f,
            RequestBody::SavePastedImage {
                session: f.id.clone(),
                data: None,
                upload: Some("upload-0".to_string()),
            },
        )
        .await
        .unwrap_err();
        assert!(!format!("{err:#}").is_empty());
        assert!(!pasted_directory(&f, &f.id).exists());
    }

    #[tokio::test]
    async fn the_request_with_both_data_and_an_upload_or_with_neither_is_refused() {
        let f = fixture(true);
        for (data, upload) in [
            (Some(encoded(PNG)), Some("upload-0".to_string())),
            (None, None),
        ] {
            let result = handled(
                &f,
                RequestBody::SavePastedImage {
                    session: f.id.clone(),
                    data,
                    upload,
                },
            )
            .await;
            assert!(result.is_err(), "{result:?}");
        }
        assert!(!pasted_directory(&f, &f.id).exists());
    }

    #[tokio::test]
    async fn the_requests_about_an_upload_are_served_on_a_control_connection_only() {
        let f = fixture(true);
        let bodies = [
            RequestBody::CreatePastedImageUpload {
                session: f.id.clone(),
                size: 100,
            },
            RequestBody::WritePastedImagePart {
                upload: "upload-0".to_string(),
                offset: 0,
                data: encoded(PNG),
            },
            RequestBody::DeletePastedImageUpload {
                upload: "upload-0".to_string(),
            },
            RequestBody::CreateAttachmentUpload {
                session: f.id.clone(),
                name: "report.pdf".to_string(),
                size: 100,
            },
            RequestBody::WriteAttachmentPart {
                upload: "attachment-0".to_string(),
                offset: 0,
                data: encoded(PNG),
            },
            RequestBody::SaveAttachment {
                session: f.id.clone(),
                upload: "attachment-0".to_string(),
            },
            RequestBody::DeleteAttachmentUpload {
                upload: "attachment-0".to_string(),
            },
        ];
        for body in bodies {
            let result = handled(&f, body).await;
            assert!(result.is_err(), "{result:?}");
        }
        assert!(!pasted_directory(&f, &f.id).exists());
        assert!(!f.run.join(&f.id).join("attachments").exists());
    }
}
