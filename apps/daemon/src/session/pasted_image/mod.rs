//! Images the user pastes into a session's terminal, kept as files its agent can read by path.
//!
//! An agent's own way of taking an image from the clipboard works only where the daemon runs on the
//! machine the clipboard is on, and only for some agents; a pasted image file's path is taken as an
//! attached image, wherever the daemon runs, by every agent whose provider declares
//! `message_images`. So for a session of such a provider the window sends the image's bytes
//! (`save_pasted_image`), the daemon writes them under the session's scratch directory, and the
//! window pastes the path it is answered with; a session of any other provider is refused
//! (`provider_unsupported`), since its agent would read the path as text. The file goes away with the scratch directory, when
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

use super::scratch::{files_home, scratch_of, subdirectory};
use super::upload::{Kind, UploadBudget, Uploads};
use crate::hostfs::replace::{replace_beside, Permissions};
use crate::protocol::{error_code, CodedError, ProviderOperation};
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
    /// [`Uploads::create_with`]. Refused as [`save`] is for a provider that takes no images.
    pub fn create(&self, state: &AppState, session_id: &str, size: u64) -> Result<String> {
        require_images_taken(state, session_id)?;
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
pub const DIRECTORY: &str = "pasted";

/// What the name of an image's file starts with. The whole name is
/// `paste-<milliseconds>-<count>.<extension>`, the extension one of [`FORMATS`]' (see
/// [`saved_name_len`]).
pub const NAME_PREFIX: &str = "paste-";

/// A format an image is saved in.
pub struct ImageFormat {
    /// The extension its file is given.
    pub extension: &'static str,
    /// Its media type.
    pub media_type: &'static str,
}

/// Every format an image is saved in: PNG, JPEG, GIF and WebP, in that order.
pub const FORMATS: [ImageFormat; 4] = [
    ImageFormat {
        extension: "png",
        media_type: "image/png",
    },
    ImageFormat {
        extension: "jpg",
        media_type: "image/jpeg",
    },
    ImageFormat {
        extension: "gif",
        media_type: "image/gif",
    },
    ImageFormat {
        extension: "webp",
        media_type: "image/webp",
    },
];

/// The length, in characters, of the name of an image's file `text` starts with:
/// [`NAME_PREFIX`], a run of digits, `-`, a run of digits, `.` and one of [`FORMATS`]' extensions.
/// `None` where `text` starts with no such name. What follows the name is not looked at.
pub fn saved_name_len(text: &str) -> Option<usize> {
    let rest = text.strip_prefix(NAME_PREFIX)?;
    let digits = |text: &str| {
        let count = text.chars().take_while(char::is_ascii_digit).count();
        (count > 0).then_some(count)
    };
    let first = digits(rest)?;
    let rest = rest[first..].strip_prefix('-')?;
    let second = digits(rest)?;
    let rest = rest[second..].strip_prefix('.')?;
    let extension = FORMATS
        .iter()
        .map(|format| format.extension)
        .find(|extension| rest.starts_with(extension))?;
    Some(NAME_PREFIX.len() + first + 1 + second + 1 + extension.len())
}

/// Distinguishes two images saved within one millisecond.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// Refuses the session `session_id` when its provider does not declare `message_images`
/// (`provider_unsupported`), or as an unknown session.
fn require_images_taken(state: &AppState, session_id: &str) -> Result<()> {
    let agent = state.session_record(session_id)?.agent;
    CodedError::require_offered(
        agent,
        ProviderOperation::MessageImages,
        agent.descriptor().capabilities.message_images.then_some(()),
    )
}

/// Writes the image `data` (base64) holds for the session `session_id`, in its scratch directory
/// while it runs and in its composer directory otherwise (`super::scratch::files_home`), and
/// returns the file's absolute path. Refused with `unknown_session` or `session_not_running` (an
/// archived session), and with
/// `provider_unsupported` for a session whose provider takes no images; an image over
/// [`MAX_BYTES`], one that is not a PNG, JPEG, GIF or WebP, or a write that fails is an error the
/// window shows as it is: it limits the size before sending and sends only those formats.
/// **Blocks.**
pub fn save(state: &AppState, session_id: &str, data: &str) -> Result<PathBuf> {
    require_images_taken(state, session_id)?;
    let scratch = files_home(state, session_id)?;
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
    require_images_taken(state, session_id)?;
    let scratch = files_home(state, session_id)?;
    write_image(session_id, &scratch, bytes)
}

/// Checks that `paths` are each an image `save` or `save_bytes` wrote for the running session
/// `session_id`: directly in its scratch directory's or its composer directory's `pasted`
/// directory, a file that exists. A
/// path anywhere else, one with `..` in it or one of another session's would have the agent read
/// any file the daemon can. Refused with `unreadable_request` (`field: images`), or as
/// [`scratch_of`] refuses.
pub(crate) fn require_saved_for(
    state: &AppState,
    session_id: &str,
    paths: &[String],
) -> Result<()> {
    let directory = scratch_of(state, session_id)?.join(DIRECTORY);
    let queued = state.composer_dir(session_id).join(DIRECTORY);
    for path in paths {
        let path = std::path::Path::new(path);
        let own = (path.parent() == Some(directory.as_path())
            || path.parent() == Some(queued.as_path()))
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(NAME_PREFIX))
            && path.is_file();
        if !own {
            return Err(CodedError::raised(
                error_code::UNREADABLE_REQUEST,
                format!(
                    "`{}` is not an image saved for this session",
                    path.display()
                ),
                &[("field", "images")],
            ));
        }
    }
    Ok(())
}

/// Writes `bytes` as an image file under `scratch`, the session's scratch directory.
fn write_image(session_id: &str, scratch: &std::path::Path, bytes: &[u8]) -> Result<PathBuf> {
    if bytes.len() > MAX_BYTES {
        bail!("the image is larger than {MAX_BYTES} bytes");
    }
    let Some(format) = format_of(bytes) else {
        bail!("the image is not a PNG, JPEG, GIF or WebP file");
    };
    let directory = subdirectory(session_id, scratch, DIRECTORY)?;
    let path = directory.join(format!(
        "{NAME_PREFIX}{}-{}.{}",
        crate::time::now_millis(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        format.extension
    ));
    // The owner's alone, like everything else in the scratch directory.
    replace_beside(&path, bytes, Permissions::Private)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// The format `bytes` begin as, from its signature.
fn format_of(bytes: &[u8]) -> Option<&'static ImageFormat> {
    let [png, jpeg, gif, webp] = &FORMATS;
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(png)
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some(jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(gif)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(webp)
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

    /// Only a file `save_bytes` wrote for this session, still there, is one of its images.
    #[tokio::test]
    async fn only_an_image_saved_for_the_session_is_one_of_its_images() {
        let f = fixture(true);
        let saved = save_bytes(&f.state, &f.id, PNG).unwrap();
        let path = saved.to_string_lossy().into_owned();
        super::require_saved_for(&f.state, &f.id, std::slice::from_ref(&path)).unwrap();
        let outside = f.run.join("paste-1.png");
        std::fs::write(&outside, PNG).unwrap();
        let sneaky = format!("{}/../{}", saved.parent().unwrap().display(), "paste-1.png");
        for other in [
            outside.to_string_lossy().into_owned(),
            sneaky,
            "/etc/hosts".to_string(),
        ] {
            let refused = super::require_saved_for(&f.state, &f.id, &[path.clone(), other.clone()])
                .unwrap_err();
            assert_eq!(code_of(&refused), error_code::UNREADABLE_REQUEST, "{other}");
            assert_eq!(crate::test_support::param_of(&refused, "field"), "images");
        }
        std::fs::remove_file(&saved).unwrap();
        let refused =
            super::require_saved_for(&f.state, &f.id, std::slice::from_ref(&path)).unwrap_err();
        assert_eq!(code_of(&refused), error_code::UNREADABLE_REQUEST);
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
        assert_eq!(
            super::saved_name_len(&format!("{name} and more")),
            Some(name.chars().count()),
            "{name:?}"
        );
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

    /// An interrupted session has no scratch directory: its pasted image is saved straight into
    /// its composer directory, and an archived session takes none.
    #[tokio::test]
    async fn an_image_for_an_interrupted_session_is_saved_in_its_composer_directory() {
        let f = fixture(false);
        let set = |status| {
            let mut record = f.state.session_record(&f.id).unwrap();
            record.status = status;
            f.state.save_session(&record).unwrap();
        };
        set(crate::protocol::SessionStatus::Interrupted);
        let path = save(&f.state, &f.id, &encoded(PNG)).unwrap();
        assert_eq!(
            path.parent().unwrap(),
            f.state.composer_dir(&f.id).join("pasted")
        );
        assert_eq!(std::fs::read(&path).unwrap(), PNG);
        set(crate::protocol::SessionStatus::Archived);
        let err = save_bytes(&f.state, &f.id, PNG).unwrap_err();
        assert_eq!(code_of(&err), error_code::SESSION_NOT_RUNNING);
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

    /// A session whose provider does not declare `message_images` is refused an image, in one
    /// frame, already decoded or as an upload, and nothing is written: its agent would read the
    /// pasted path as text.
    #[tokio::test]
    async fn a_session_of_a_provider_that_takes_no_images_is_refused_and_nothing_is_written() {
        use crate::protocol::AgentId;
        use crate::session::pasted_image::ImageUploads;
        use crate::session::upload::UploadBudget;

        let f = fixture(true);
        let _no_images = crate::providers::test_provider::without_message_images();
        let without = AgentId::TEST;
        assert!(!without.descriptor().capabilities.message_images);
        let mut session = f.state.session_record(&f.id).unwrap();
        session.agent = without;
        f.state.store.update_session(&session).unwrap();

        let refusals = [
            save(&f.state, &f.id, &encoded(PNG)).unwrap_err(),
            save_bytes(&f.state, &f.id, PNG).unwrap_err(),
            ImageUploads::images(UploadBudget::images())
                .create(&f.state, &f.id, PNG.len() as u64)
                .unwrap_err(),
        ];
        for refused in refusals {
            assert_eq!(code_of(&refused), error_code::PROVIDER_UNSUPPORTED);
            assert_eq!(
                crate::test_support::param_of(&refused, "operation"),
                "message_images"
            );
            assert_eq!(
                crate::test_support::param_of(&refused, "agent"),
                without.wire_name()
            );
        }
        assert!(!pasted_directory(&f, &f.id).exists());
    }

    /// A saved image's name is the prefix, two runs of digits and one of the formats' extensions;
    /// anything else is no such name.
    #[test]
    fn a_saved_name_is_told_by_its_shape() {
        for (text, len) in [
            ("paste-1-0.png", Some(13)),
            ("paste-17954-12.webp'rest", Some(19)),
            ("paste-1-0.jpg/paste-2-0.gif", Some(13)),
            ("paste-1.png", None),
            ("paste--0.png", None),
            ("paste-1-0.bmp", None),
            ("shot-1-0.png", None),
            ("paste-1-0png", None),
        ] {
            assert_eq!(super::saved_name_len(text), len, "{text}");
        }
    }

    /// Each format's extension is the one an image of its signature is saved under.
    #[test]
    fn each_format_is_the_one_its_signature_is_saved_as() {
        let extensions: Vec<_> = [PNG, JPEG, GIF_89, WEBP]
            .iter()
            .map(|bytes| super::format_of(bytes).expect("a format").extension)
            .collect();
        assert_eq!(extensions, ["png", "jpg", "gif", "webp"]);
        assert_eq!(super::format_of(GIF_87).unwrap().media_type, "image/gif");
        assert!(super::format_of(b"BM not one").is_none());
    }
}
