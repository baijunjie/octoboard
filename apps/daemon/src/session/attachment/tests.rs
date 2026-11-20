use std::path::PathBuf;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use super::*;
use crate::protocol::{error_code, CodedError};
use crate::session::pasted_image::ImageUploads;
use crate::session::test_support::{fixture, Fixture};
use crate::session::upload::IDLE_EXPIRY;
use crate::test_support::{code_of, param_of, tasks_idle};

fn b64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// `size` bytes that are not all alike, so a part out of place shows.
fn bytes_of(size: usize) -> Vec<u8> {
    (0..size).map(|n| (n % 251) as u8).collect()
}

fn attachments_in(f: &Fixture) -> PathBuf {
    f.run.join(&f.id).join("attachments")
}

/// The names in the attachments directory, sorted; none where it does not exist.
fn names_in(f: &Fixture) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(attachments_in(f)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The hidden spool files in the attachments directory.
fn spool_files(f: &Fixture) -> Vec<String> {
    names_in(f)
        .into_iter()
        .filter(|name| name.starts_with(".upload-") && name.ends_with(".partial"))
        .collect()
}

fn uploads_of(f: &Fixture) -> AttachmentUploads {
    Uploads::attachments(f.state.attachment_uploads.clone())
}

/// An upload of `bytes` called `name`, created and completed in parts of `part`.
fn completed(
    f: &Fixture,
    uploads: &AttachmentUploads,
    name: &str,
    bytes: &[u8],
    part: usize,
) -> String {
    let upload = uploads
        .create(&f.state, &f.id, name, bytes.len() as u64)
        .unwrap();
    for (n, chunk) in bytes.chunks(part).enumerate() {
        uploads
            .write(&upload, (n * part) as u64, &b64(chunk))
            .unwrap();
    }
    upload
}

/// [`completed`], taken and saved: the path the file got.
fn saved(f: &Fixture, uploads: &AttachmentUploads, name: &str, bytes: &[u8]) -> PathBuf {
    let upload = completed(f, uploads, name, bytes, 1000);
    save(&f.state, &f.id, uploads.take(&upload, &f.id).unwrap()).unwrap()
}

fn is_plain(err: &anyhow::Error) -> bool {
    err.downcast_ref::<CodedError>().is_none()
}

#[test]
fn the_snapshot_limits_are_a_hundred_mebibytes_a_file_and_one_a_part() {
    let limits = limits();
    assert_eq!(limits.max_bytes, 104_857_600);
    assert_eq!(limits.max_part_bytes, 1_048_576);
}

#[tokio::test]
async fn a_file_uploaded_in_parts_is_saved_under_its_name_with_the_bytes_sent() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let bytes = bytes_of(3500);
    let upload = completed(&f, &uploads, "report.pdf", &bytes, 1000);
    let path = save(&f.state, &f.id, uploads.take(&upload, &f.id).unwrap()).unwrap();
    assert!(path.is_absolute(), "{path:?}");
    assert_eq!(path.parent().unwrap(), attachments_in(&f));
    assert_eq!(path.file_name().unwrap(), "report.pdf");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(
        names_in(&f),
        ["report.pdf"],
        "the spool file is gone with the save"
    );
    assert_eq!(uploads.len(), 0);
}

#[tokio::test]
async fn a_file_may_come_in_one_part_or_in_parts_of_one_byte() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let bytes = bytes_of(20);
    let whole = saved(&f, &uploads, "whole.bin", &bytes);
    assert_eq!(std::fs::read(whole).unwrap(), bytes);
    let upload = completed(&f, &uploads, "singly.bin", &bytes, 1);
    let path = save(&f.state, &f.id, uploads.take(&upload, &f.id).unwrap()).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[cfg(unix)]
#[tokio::test]
async fn the_spool_file_and_the_saved_file_are_the_owners_alone() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let mode =
        |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    let upload = uploads.create(&f.state, &f.id, "a.txt", 10).unwrap();
    let spool = spool_files(&f);
    assert_eq!(spool.len(), 1, "created with the upload: {spool:?}");
    let spool = attachments_in(&f).join(&spool[0]);
    assert_eq!(mode(&spool), 0o600);
    uploads.write(&upload, 0, &b64(&[1; 10])).unwrap();
    assert_eq!(mode(&spool), 0o600);
    let path = save(&f.state, &f.id, uploads.take(&upload, &f.id).unwrap()).unwrap();
    assert_eq!(mode(&path), 0o600);
}

#[tokio::test]
async fn while_an_upload_is_under_way_its_hidden_spool_file_is_in_the_attachments_directory() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    uploads.create(&f.state, &f.id, "a.txt", 100).unwrap();
    assert_eq!(spool_files(&f).len(), 1);
    assert_eq!(names_in(&f).len(), 1, "nothing else is there yet");
    uploads.create(&f.state, &f.id, "b.txt", 100).unwrap();
    let spool = spool_files(&f);
    assert_eq!(spool.len(), 2);
    assert_ne!(spool[0], spool[1]);
}

#[tokio::test]
async fn two_files_with_one_name_are_both_kept_the_second_numbered() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let first = saved(&f, &uploads, "report.pdf", b"first");
    let second = saved(&f, &uploads, "report.pdf", b"second");
    let third = saved(&f, &uploads, "report.pdf", b"third");
    assert_eq!(first.file_name().unwrap(), "report.pdf");
    assert_eq!(second.file_name().unwrap(), "report-2.pdf");
    assert_eq!(third.file_name().unwrap(), "report-3.pdf");
    assert_eq!(std::fs::read(first).unwrap(), b"first");
    assert_eq!(std::fs::read(second).unwrap(), b"second");
    assert_eq!(std::fs::read(third).unwrap(), b"third");
    assert_eq!(names_in(&f), ["report-2.pdf", "report-3.pdf", "report.pdf"]);
}

#[tokio::test]
async fn a_long_name_of_wide_characters_is_saved_and_numbered_within_what_a_file_system_takes() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let name = format!("{}.txt", "报".repeat(150));
    let first = saved(&f, &uploads, &name, b"first");
    let second = saved(&f, &uploads, &name, b"second");
    for path in [&first, &second] {
        let saved_name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(saved_name.len() < 255, "{} bytes", saved_name.len());
        assert!(
            saved_name.starts_with('报') && saved_name.ends_with(".txt"),
            "{saved_name}"
        );
    }
    assert_eq!(std::fs::read(first).unwrap(), b"first");
    assert_eq!(std::fs::read(second).unwrap(), b"second");
}

#[tokio::test]
async fn a_name_that_climbs_out_or_holds_whitespace_is_saved_inside_the_attachments_directory() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let climbing = saved(&f, &uploads, "../../x", b"x");
    assert_eq!(climbing.parent().unwrap(), attachments_in(&f));
    assert_eq!(climbing.file_name().unwrap(), "x");
    let spaced = saved(&f, &uploads, "my report (final).pdf", b"y");
    assert_eq!(spaced.parent().unwrap(), attachments_in(&f));
    let name = spaced.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        !name
            .chars()
            .any(|c| c.is_whitespace() || c == '(' || c == ')'),
        "{name:?}"
    );
    let nothing = saved(&f, &uploads, "///", b"z");
    assert_eq!(nothing.file_name().unwrap(), "attachment");
    // Nothing was written above the attachments directory.
    let scratch: Vec<_> = std::fs::read_dir(f.run.join(&f.id))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(scratch, ["attachments"]);
    assert!(!f.run.join("x").exists());
}

#[tokio::test]
async fn a_size_of_nothing_or_over_a_hundred_mebibytes_is_refused_and_leaves_nothing() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    for size in [0, MAX_BYTES as u64 + 1, u64::MAX] {
        let err = uploads.create(&f.state, &f.id, "a.bin", size).unwrap_err();
        assert!(is_plain(&err), "size {size}: {err:#}");
    }
    assert_eq!(uploads.len(), 0);
    assert!(spool_files(&f).is_empty());
    assert_eq!(
        f.state.attachment_uploads.available(),
        MAX_UPLOAD_BYTES,
        "a refusal holds no bytes"
    );
    // The limit itself, and the least there is, are taken.
    uploads
        .create(&f.state, &f.id, "a.bin", MAX_BYTES as u64)
        .unwrap();
    uploads.create(&f.state, &f.id, "b.bin", 1).unwrap();
}

#[tokio::test]
async fn an_upload_for_an_unknown_session_or_one_not_running_is_refused_as_such() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let err = uploads
        .create(&f.state, "no-such-session", "a.txt", 10)
        .unwrap_err();
    assert_eq!(code_of(&err), error_code::UNKNOWN_SESSION);
    let stopped = fixture(false);
    let err = uploads
        .create(&stopped.state, &stopped.id, "a.txt", 10)
        .unwrap_err();
    assert_eq!(code_of(&err), error_code::SESSION_NOT_RUNNING);
    assert_eq!(uploads.len(), 0);
    assert!(
        !stopped.run.join(&stopped.id).exists(),
        "nothing is created for a session with no process"
    );
}

#[tokio::test]
async fn a_third_upload_on_one_connection_is_refused_leaves_no_spool_file_and_one_may_follow_a_delete(
) {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let first = uploads.create(&f.state, &f.id, "a.txt", 10).unwrap();
    uploads.create(&f.state, &f.id, "b.txt", 10).unwrap();
    let err = uploads.create(&f.state, &f.id, "c.txt", 10).unwrap_err();
    assert_eq!(code_of(&err), error_code::LIMIT_EXCEEDED);
    assert_eq!(param_of(&err, "limit"), "attachment_uploads");
    assert_eq!(param_of(&err, "max"), "2");
    assert_eq!(uploads.len(), 2);
    assert_eq!(spool_files(&f).len(), 2, "the refused upload left no file");

    uploads.delete(&first);
    assert_eq!(spool_files(&f).len(), 1);
    uploads.create(&f.state, &f.id, "c.txt", 10).unwrap();
    assert_eq!(spool_files(&f).len(), 2);
}

#[tokio::test]
async fn declared_sizes_across_connections_are_held_to_the_daemons_budget() {
    let f = fixture(true);
    let budget = f.state.attachment_uploads.clone();
    assert_eq!(budget.available(), MAX_UPLOAD_BYTES);
    let connections: Vec<AttachmentUploads> = (0..3).map(|_| uploads_of(&f)).collect();
    // Five of the largest files take 500 MiB of the 512.
    for (n, size) in [MAX_BYTES; 5].into_iter().enumerate() {
        connections[n / 2]
            .create(&f.state, &f.id, "big.bin", size as u64)
            .unwrap();
    }
    let left = MAX_UPLOAD_BYTES - 5 * MAX_BYTES;
    assert_eq!(budget.available(), left);
    let spooled = spool_files(&f).len();
    let err = connections[2]
        .create(&f.state, &f.id, "more.bin", left as u64 + 1)
        .unwrap_err();
    assert_eq!(code_of(&err), error_code::LIMIT_EXCEEDED);
    assert_eq!(param_of(&err, "limit"), "attachment_upload_bytes");
    assert_eq!(param_of(&err, "max"), MAX_UPLOAD_BYTES.to_string());
    assert_eq!(budget.available(), left, "a refusal holds nothing");
    assert_eq!(spool_files(&f).len(), spooled, "and leaves no spool file");
    // Exactly what is left is taken.
    connections[2]
        .create(&f.state, &f.id, "rest.bin", left as u64)
        .unwrap();
    assert_eq!(budget.available(), 0);
    // Dropping a connection gives its bytes back.
    drop(connections);
    assert_eq!(budget.available(), MAX_UPLOAD_BYTES);
}

#[tokio::test]
async fn the_attachment_and_image_limits_and_budgets_are_apart() {
    let f = fixture(true);
    let attachments = uploads_of(&f);
    let images = ImageUploads::images(f.state.pasted_image_uploads.clone());
    let images_available = f.state.pasted_image_uploads.available();
    // Two of each on one connection.
    attachments.create(&f.state, &f.id, "a.bin", 1000).unwrap();
    attachments.create(&f.state, &f.id, "b.bin", 1000).unwrap();
    images.create(&f.state, &f.id, 500).unwrap();
    images.create(&f.state, &f.id, 500).unwrap();
    assert_eq!(
        f.state.attachment_uploads.available(),
        MAX_UPLOAD_BYTES - 2000
    );
    assert_eq!(
        f.state.pasted_image_uploads.available(),
        images_available - 1000
    );
    // Each family's third is refused with its own limit.
    let err = attachments.create(&f.state, &f.id, "c.bin", 1).unwrap_err();
    assert_eq!(param_of(&err, "limit"), "attachment_uploads");
    let err = images.create(&f.state, &f.id, 1).unwrap_err();
    assert_eq!(param_of(&err, "limit"), "image_uploads");
}

#[tokio::test]
async fn a_part_of_exactly_one_mebibyte_is_taken_and_a_byte_more_discards_the_upload_and_its_spool_file(
) {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let upload = uploads
        .create(&f.state, &f.id, "big.bin", 3 * 1024 * 1024)
        .unwrap();
    uploads
        .write(&upload, 0, &b64(&vec![1; 1024 * 1024]))
        .unwrap();
    let err = uploads
        .write(&upload, 1024 * 1024, &b64(&vec![1; 1024 * 1024 + 1]))
        .unwrap_err();
    assert_eq!(code_of(&err), error_code::LIMIT_EXCEEDED);
    assert_eq!(param_of(&err, "limit"), "upload_part_bytes");
    assert_eq!(param_of(&err, "max"), "1048576");
    assert_eq!(uploads.len(), 0);
    assert!(spool_files(&f).is_empty());
    assert_eq!(f.state.attachment_uploads.available(), MAX_UPLOAD_BYTES);
}

#[tokio::test]
async fn a_part_at_the_wrong_offset_is_a_mismatch_and_discards_the_upload_and_its_spool_file() {
    let f = fixture(true);
    for (received, offset) in [(0usize, 5u64), (10, 5), (10, 11), (10, 0)] {
        let uploads = uploads_of(&f);
        let upload = uploads.create(&f.state, &f.id, "a.bin", 50).unwrap();
        if received > 0 {
            uploads.write(&upload, 0, &b64(&vec![1; received])).unwrap();
        }
        assert_eq!(spool_files(&f).len(), 1);
        let err = uploads.write(&upload, offset, &b64(b"abc")).unwrap_err();
        assert_eq!(code_of(&err), error_code::UPLOAD_PARTS_MISMATCH);
        assert_eq!(param_of(&err, "expected_offset"), received.to_string());
        assert_eq!(param_of(&err, "size"), "50");
        assert_eq!(uploads.len(), 0, "received {received}, offset {offset}");
        assert!(
            spool_files(&f).is_empty(),
            "received {received}, offset {offset}"
        );
        let gone = uploads
            .write(&upload, received as u64, &b64(b"abc"))
            .unwrap_err();
        assert_eq!(code_of(&gone), error_code::UPLOAD_UNAVAILABLE);
    }
}

#[tokio::test]
async fn a_part_running_past_the_size_is_a_mismatch_and_discards_the_upload_and_its_spool_file() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let upload = uploads.create(&f.state, &f.id, "a.bin", 10).unwrap();
    uploads.write(&upload, 0, &b64(&[1; 4])).unwrap();
    let err = uploads.write(&upload, 4, &b64(&[1; 7])).unwrap_err();
    assert_eq!(code_of(&err), error_code::UPLOAD_PARTS_MISMATCH);
    assert_eq!(param_of(&err, "expected_offset"), "4");
    assert_eq!(param_of(&err, "size"), "10");
    assert_eq!(uploads.len(), 0);
    assert!(spool_files(&f).is_empty());
}

#[tokio::test]
async fn data_that_is_not_base64_is_refused_and_discards_the_upload_and_its_spool_file() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let upload = uploads.create(&f.state, &f.id, "a.bin", 10).unwrap();
    let err = uploads.write(&upload, 0, "not base64 !!!").unwrap_err();
    assert!(is_plain(&err), "{err:#}");
    assert_eq!(uploads.len(), 0);
    assert!(spool_files(&f).is_empty());
}

#[tokio::test]
async fn taking_an_upload_whose_parts_end_short_is_a_mismatch_and_removes_its_spool_file() {
    let f = fixture(true);
    for received in [0usize, 1, 29] {
        let uploads = uploads_of(&f);
        let upload = uploads.create(&f.state, &f.id, "a.bin", 30).unwrap();
        if received > 0 {
            uploads.write(&upload, 0, &b64(&vec![1; received])).unwrap();
        }
        let err = match uploads.take(&upload, &f.id) {
            Ok(_) => panic!("a short upload was taken"),
            Err(err) => err,
        };
        assert_eq!(code_of(&err), error_code::UPLOAD_PARTS_MISMATCH);
        assert_eq!(param_of(&err, "expected_offset"), received.to_string());
        assert_eq!(param_of(&err, "size"), "30");
        assert!(spool_files(&f).is_empty(), "received {received}");
        assert!(names_in(&f).is_empty(), "no file was saved");
    }
}

#[tokio::test]
async fn taking_an_upload_for_another_session_is_unavailable_and_removes_its_spool_file() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let upload = completed(&f, &uploads, "a.bin", &bytes_of(30), 30);
    let err = match uploads.take(&upload, "another-session") {
        Ok(_) => panic!("another session's save took the upload"),
        Err(err) => err,
    };
    assert_eq!(code_of(&err), error_code::UPLOAD_UNAVAILABLE);
    assert_eq!(uploads.len(), 0);
    assert!(spool_files(&f).is_empty());
    assert!(names_in(&f).is_empty());
}

#[tokio::test]
async fn an_image_handle_is_unavailable_to_attachment_requests_and_the_reverse() {
    let f = fixture(true);
    let attachments = uploads_of(&f);
    let images = ImageUploads::images(f.state.pasted_image_uploads.clone());
    let image = images.create(&f.state, &f.id, 100).unwrap();
    let attachment = attachments.create(&f.state, &f.id, "a.bin", 100).unwrap();

    let err = attachments.write(&image, 0, &b64(b"abc")).unwrap_err();
    assert_eq!(code_of(&err), error_code::UPLOAD_UNAVAILABLE);
    let err = images.write(&attachment, 0, &b64(b"abc")).unwrap_err();
    assert_eq!(code_of(&err), error_code::UPLOAD_UNAVAILABLE);
    let err = match attachments.take(&image, &f.id) {
        Ok(_) => panic!("an image handle was taken as an attachment"),
        Err(err) => err,
    };
    assert_eq!(code_of(&err), error_code::UPLOAD_UNAVAILABLE);
    // Neither refusal touched the other family's upload.
    assert_eq!(images.len(), 1);
}

#[tokio::test]
async fn deleting_an_upload_removes_its_spool_file_and_deleting_an_unknown_handle_is_harmless() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let upload = uploads.create(&f.state, &f.id, "a.bin", 100).unwrap();
    uploads.write(&upload, 0, &b64(&[1; 10])).unwrap();
    assert_eq!(spool_files(&f).len(), 1);
    uploads.delete(&upload);
    uploads.delete(&upload);
    uploads.delete("attachment-404");
    assert!(spool_files(&f).is_empty());
    assert_eq!(f.state.attachment_uploads.available(), MAX_UPLOAD_BYTES);
    let err = uploads.write(&upload, 10, &b64(b"abc")).unwrap_err();
    assert_eq!(code_of(&err), error_code::UPLOAD_UNAVAILABLE);
}

#[tokio::test]
async fn dropping_the_connections_uploads_removes_every_spool_file() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let written = uploads.create(&f.state, &f.id, "a.bin", 100).unwrap();
    uploads.write(&written, 0, &b64(&[1; 10])).unwrap();
    uploads.create(&f.state, &f.id, "b.bin", 100).unwrap();
    assert_eq!(spool_files(&f).len(), 2);
    drop(uploads);
    assert!(spool_files(&f).is_empty());
    assert_eq!(f.state.attachment_uploads.available(), MAX_UPLOAD_BYTES);
}

#[tokio::test(start_paused = true)]
async fn an_upload_left_idle_is_dropped_with_its_spool_file_after_the_idle_time() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let second = std::time::Duration::from_secs(1);
    let upload = uploads.create(&f.state, &f.id, "a.bin", 100).unwrap();
    tokio::time::advance(IDLE_EXPIRY - second).await;
    uploads.write(&upload, 0, &b64(&[1; 10])).unwrap();
    // Past the time since it was created, short of the time since its last part.
    tokio::time::advance(IDLE_EXPIRY - second).await;
    tasks_idle().await;
    assert_eq!(uploads.len(), 1);
    assert_eq!(spool_files(&f).len(), 1);
    // Past the idle time with no part: gone, and its spool file with it.
    tokio::time::advance(second * 2).await;
    tasks_idle().await;
    assert_eq!(uploads.len(), 0);
    assert!(spool_files(&f).is_empty());
    assert_eq!(f.state.attachment_uploads.available(), MAX_UPLOAD_BYTES);
    let err = uploads.write(&upload, 10, &b64(b"abc")).unwrap_err();
    assert_eq!(code_of(&err), error_code::UPLOAD_UNAVAILABLE);
}

#[tokio::test]
async fn an_upload_taken_but_never_saved_leaves_nothing_behind() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let upload = completed(&f, &uploads, "a.bin", &bytes_of(30), 30);
    let taken = uploads.take(&upload, &f.id).unwrap();
    drop(taken);
    assert!(names_in(&f).is_empty());
    assert_eq!(f.state.attachment_uploads.available(), MAX_UPLOAD_BYTES);
}

#[tokio::test]
async fn the_bytes_taken_stay_counted_until_the_save_returns() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let upload = completed(&f, &uploads, "a.bin", &bytes_of(100), 100);
    let taken = uploads.take(&upload, &f.id).unwrap();
    assert_eq!(
        f.state.attachment_uploads.available(),
        MAX_UPLOAD_BYTES - 100
    );
    save(&f.state, &f.id, taken).unwrap();
    assert_eq!(f.state.attachment_uploads.available(), MAX_UPLOAD_BYTES);
}

#[tokio::test]
async fn a_save_for_an_unknown_session_is_refused_and_removes_the_spool_file() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let upload = completed(&f, &uploads, "a.bin", &bytes_of(30), 30);
    let taken = uploads.take(&upload, &f.id).unwrap();
    let err = save(&f.state, "no-such-session", taken).unwrap_err();
    assert_eq!(code_of(&err), error_code::UNKNOWN_SESSION);
    assert!(names_in(&f).is_empty(), "{:?}", names_in(&f));
}

#[tokio::test]
async fn a_save_after_the_scratch_directory_was_removed_is_not_running_and_recreates_nothing() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    let upload = completed(&f, &uploads, "a.bin", &bytes_of(30), 30);
    let taken = uploads.take(&upload, &f.id).unwrap();
    std::fs::remove_dir_all(f.run.join(&f.id)).unwrap();
    let err = save(&f.state, &f.id, taken).unwrap_err();
    assert_eq!(code_of(&err), error_code::SESSION_NOT_RUNNING);
    assert!(
        !f.run.join(&f.id).exists(),
        "the scratch directory stays gone"
    );
}

#[tokio::test]
async fn an_upload_created_after_the_scratch_directory_was_removed_is_not_running_and_recreates_nothing(
) {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    std::fs::remove_dir_all(f.run.join(&f.id)).unwrap();
    let err = uploads.create(&f.state, &f.id, "a.bin", 30).unwrap_err();
    assert_eq!(code_of(&err), error_code::SESSION_NOT_RUNNING);
    assert!(!f.run.join(&f.id).exists());
    assert_eq!(uploads.len(), 0);
    assert_eq!(f.state.attachment_uploads.available(), MAX_UPLOAD_BYTES);
}

#[tokio::test]
async fn the_attachments_directory_is_made_by_the_first_upload_and_reused_by_the_next() {
    let f = fixture(true);
    let uploads = uploads_of(&f);
    assert!(!attachments_in(&f).exists());
    let first = saved(&f, &uploads, "a.txt", b"a");
    let second = saved(&f, &uploads, "b.txt", b"b");
    assert_eq!(first.parent(), second.parent());
    assert_eq!(names_in(&f), ["a.txt", "b.txt"]);
}
