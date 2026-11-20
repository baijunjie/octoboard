//! Socket-level tests of the uploads a control connection serves: a pasted image's and an
//! attached file's.

use std::sync::Arc;

use futures_util::StreamExt;

use super::tests::{serve_control_of, Client};
use super::*;
use crate::test_support::PATIENCE;

/// A state with a running stand-in session `id` whose scratch directory is under `run`.
struct Running {
    state: Arc<AppState>,
    id: &'static str,
    run: crate::test_support::ScratchDir,
    /// The stand-in process, which ends before the directories it lives in go.
    _live: Box<dyn std::any::Any>,
    _dir: crate::test_support::ScratchDir,
}

fn running_session() -> Running {
    use crate::protocol::{AgentId, SessionStatus};
    let id = "control-upload-session";
    let (state, dir) = crate::test_support::app_state("control-upload");
    crate::state::test_support::insert_record(&state, &dir, id, SessionStatus::Working);
    let run = crate::test_support::ScratchDir::new("control-upload-run");
    let scratch = run.join(id);
    std::fs::create_dir_all(&scratch).unwrap();
    let live = crate::test_support::fake_live_session_in(id, AgentId::CLAUDE, "sleep 30", scratch);
    state.register_live(live.clone());
    Running {
        state,
        id,
        run,
        _live: Box::new(live),
        _dir: dir,
    }
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

/// A PNG of `size` bytes whose bytes are not all alike, so a part out of place shows.
fn png_of(size: usize) -> Vec<u8> {
    let mut bytes = PNG.to_vec();
    bytes.extend((0..size - PNG.len()).map(|n| (n % 251) as u8));
    bytes
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn pasted_in(run: &crate::test_support::ScratchDir, id: &str) -> std::path::PathBuf {
    run.join(id).join("pasted")
}

async fn created(client: &mut Client, id: &str, session: &str, size: usize) -> String {
    let reply = client
        .ask(
            id,
            serde_json::json!({"id": id, "type": "create_pasted_image_upload",
                "session": session, "size": size}),
        )
        .await;
    assert_eq!(reply["type"], "pasted_image_upload_created", "{reply}");
    reply["upload"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn an_image_uploaded_in_parts_sent_without_waiting_for_acks_is_saved_as_the_file_of_its_bytes(
) {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let bytes = png_of(3000);
    let upload = created(&mut client, "create", r.id, bytes.len()).await;

    // The parts and the save go out back to back: they are served in the order they came.
    for (n, part) in bytes.chunks(1000).enumerate() {
        client
            .send(
                serde_json::json!({"id": format!("part-{n}"), "type": "write_pasted_image_part",
                "upload": upload, "offset": n * 1000, "data": b64(part)}),
            )
            .await;
    }
    client
        .send(
            serde_json::json!({"id": "save", "type": "save_pasted_image",
            "session": r.id, "upload": upload}),
        )
        .await;
    for n in 0..3 {
        let ack = client.reply(&format!("part-{n}")).await;
        assert_eq!(ack["type"], "ack", "{ack}");
    }
    let saved = client.reply("save").await;
    assert_eq!(saved["type"], "pasted_image_saved", "{saved}");
    let path = std::path::PathBuf::from(saved["path"].as_str().unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(path.parent().unwrap(), pasted_in(&r.run, r.id));
    assert_eq!(path.extension().unwrap(), "png");

    // Saved, the upload is gone.
    let again = client
        .ask(
            "again",
            serde_json::json!({"id": "again", "type": "save_pasted_image",
                "session": r.id, "upload": upload}),
        )
        .await;
    assert_eq!(again["code"], "upload_unavailable", "{again}");
    assert_eq!(again["params"]["upload"], upload.as_str());
}

#[tokio::test]
async fn parts_that_do_not_add_up_are_refused_at_the_save_and_leave_no_file() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let bytes = png_of(3000);
    let upload = created(&mut client, "create", r.id, bytes.len()).await;
    for (n, part) in bytes.chunks(1000).take(2).enumerate() {
        let ack = client
            .ask(
                &format!("part-{n}"),
                serde_json::json!({"id": format!("part-{n}"), "type": "write_pasted_image_part",
                    "upload": upload, "offset": n * 1000, "data": b64(part)}),
            )
            .await;
        assert_eq!(ack["type"], "ack");
    }
    let refused = client
        .ask(
            "save",
            serde_json::json!({"id": "save", "type": "save_pasted_image",
                "session": r.id, "upload": upload}),
        )
        .await;
    assert_eq!(refused["type"], "error", "{refused}");
    assert_eq!(refused["code"], "upload_parts_mismatch");
    assert_eq!(refused["params"]["expected_offset"], "2000");
    assert_eq!(refused["params"]["size"], "3000");
    assert!(!pasted_in(&r.run, r.id).exists());

    let again = client
        .ask(
            "again",
            serde_json::json!({"id": "again", "type": "write_pasted_image_part",
                "upload": upload, "offset": 2000, "data": b64(&bytes[2000..])}),
        )
        .await;
    assert_eq!(
        again["code"], "upload_unavailable",
        "the refusal discarded it: {again}"
    );
}

#[tokio::test]
async fn a_part_out_of_place_is_refused_and_discards_the_upload() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let bytes = png_of(2000);
    let upload = created(&mut client, "create", r.id, bytes.len()).await;
    let refused = client
        .ask(
            "skip",
            serde_json::json!({"id": "skip", "type": "write_pasted_image_part",
                "upload": upload, "offset": 1000, "data": b64(&bytes[1000..])}),
        )
        .await;
    assert_eq!(refused["code"], "upload_parts_mismatch", "{refused}");
    assert_eq!(refused["params"]["expected_offset"], "0");
    let save = client
        .ask(
            "save",
            serde_json::json!({"id": "save", "type": "save_pasted_image",
                "session": r.id, "upload": upload}),
        )
        .await;
    assert_eq!(save["code"], "upload_unavailable", "{save}");
    assert!(!pasted_in(&r.run, r.id).exists());
}

#[tokio::test]
async fn a_deleted_upload_is_acknowledged_also_when_gone_and_cannot_be_saved() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let upload = created(&mut client, "create", r.id, 100).await;
    for id in ["delete", "delete-again"] {
        let reply = client
            .ask(
                id,
                serde_json::json!({"id": id, "type": "delete_pasted_image_upload",
                    "upload": upload}),
            )
            .await;
        assert_eq!(reply["type"], "ack", "{reply}");
    }
    let save = client
        .ask(
            "save",
            serde_json::json!({"id": "save", "type": "save_pasted_image",
                "session": r.id, "upload": upload}),
        )
        .await;
    assert_eq!(save["code"], "upload_unavailable", "{save}");
}

#[tokio::test]
async fn the_single_frame_save_is_still_served_and_a_save_naming_neither_or_both_is_refused() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let bytes = png_of(500);
    let saved = client
        .ask(
            "whole",
            serde_json::json!({"id": "whole", "type": "save_pasted_image",
                "session": r.id, "data": b64(&bytes)}),
        )
        .await;
    assert_eq!(saved["type"], "pasted_image_saved", "{saved}");
    let path = std::path::PathBuf::from(saved["path"].as_str().unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);

    let upload = created(&mut client, "create", r.id, bytes.len()).await;
    let neither = client
        .ask(
            "neither",
            serde_json::json!({"id": "neither", "type": "save_pasted_image", "session": r.id}),
        )
        .await;
    assert_eq!(neither["type"], "error", "{neither}");
    let both = client
        .ask(
            "both",
            serde_json::json!({"id": "both", "type": "save_pasted_image",
                "session": r.id, "data": b64(&bytes), "upload": upload}),
        )
        .await;
    assert_eq!(both["type"], "error", "{both}");
    let files = std::fs::read_dir(pasted_in(&r.run, r.id)).unwrap().count();
    assert_eq!(files, 1, "only the single-frame save wrote a file");
}

#[tokio::test]
async fn a_third_upload_on_one_connection_is_refused_and_another_connection_has_its_own_two() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    created(&mut client, "one", r.id, 10).await;
    created(&mut client, "two", r.id, 10).await;
    let third = client
        .ask(
            "three",
            serde_json::json!({"id": "three", "type": "create_pasted_image_upload",
                "session": r.id, "size": 10}),
        )
        .await;
    assert_eq!(third["code"], "limit_exceeded", "{third}");
    assert_eq!(third["params"]["limit"], "image_uploads");
    assert_eq!(third["params"]["max"], "2");

    let mut other = Client::connect(port).await;
    created(&mut other, "other", r.id, 10).await;
}

#[tokio::test]
async fn an_upload_for_an_unknown_session_is_refused_as_such() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let reply = client
        .ask(
            "create",
            serde_json::json!({"id": "create", "type": "create_pasted_image_upload",
                "session": "no-such-session", "size": 10}),
        )
        .await;
    assert_eq!(reply["code"], "unknown_session", "{reply}");
}

#[tokio::test]
async fn a_connection_that_closes_gives_back_the_bytes_its_uploads_held() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let budget = r.state.pasted_image_uploads.clone();
    let full = budget.available();
    let mut client = Client::connect(port).await;
    created(&mut client, "create", r.id, 4096).await;
    assert_eq!(budget.available(), full - 4096);
    drop(client);
    let given_back = crate::test_support::within(PATIENCE, {
        let budget = budget.clone();
        move || budget.available() == full
    })
    .await;
    assert!(given_back, "the closed connection still holds its upload");
}

impl Client {
    /// The reply carrying error `code`, for a reply that has no id of its own to look for.
    async fn reply_with_code(&mut self, code: &str) -> serde_json::Value {
        use tokio_tungstenite::tungstenite::Message as Wire;
        if let Some(at) = self.spare.iter().position(|v| v["code"] == code) {
            return self.spare.remove(at);
        }
        loop {
            let frame = tokio::time::timeout(PATIENCE, self.socket.next())
                .await
                .unwrap_or_else(|_| panic!("no {code} reply"))
                .expect("the connection stays open")
                .unwrap();
            if let Wire::Text(text) = frame {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                if value["code"] == code {
                    return value;
                }
                self.spare.push(value);
            }
        }
    }
}

#[tokio::test]
async fn an_upload_request_whose_fields_do_not_read_is_unreadable_and_discards_its_upload() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let bytes = png_of(2000);
    let upload = created(&mut client, "create", r.id, bytes.len()).await;
    client
        .send(
            serde_json::json!({"id": "ok", "type": "write_pasted_image_part",
            "upload": upload, "offset": 0, "data": b64(&bytes[..1000])}),
        )
        .await;
    assert_eq!(client.reply("ok").await["type"], "ack");

    // A valid upload handle, an offset that is not a number.
    client
        .send(
            serde_json::json!({"id": "bad", "type": "write_pasted_image_part",
            "upload": upload, "offset": "1000", "data": b64(&bytes[1000..])}),
        )
        .await;
    let unreadable = client.reply_with_code("unreadable_request").await;
    assert_eq!(unreadable["type"], "error", "{unreadable}");

    let after = client
        .ask(
            "after",
            serde_json::json!({"id": "after", "type": "write_pasted_image_part",
                "upload": upload, "offset": 1000, "data": b64(&bytes[1000..])}),
        )
        .await;
    assert_eq!(after["code"], "upload_unavailable", "{after}");
    assert!(!pasted_in(&r.run, r.id).exists());
}

#[tokio::test]
async fn an_unreadable_upload_request_that_names_no_upload_is_only_unreadable() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let upload = created(&mut client, "create", r.id, 100).await;
    // No upload to name, and one that is not text: nothing to discard.
    client
        .send(serde_json::json!({"id": "a", "type": "write_pasted_image_part", "offset": 0}))
        .await;
    client.reply_with_code("unreadable_request").await;
    client
        .send(
            serde_json::json!({"id": "b", "type": "write_pasted_image_part",
            "upload": 5, "offset": 0, "data": ""}),
        )
        .await;
    client.reply_with_code("unreadable_request").await;
    // The other upload is untouched.
    let ack = client
        .ask(
            "write",
            serde_json::json!({"id": "write", "type": "write_pasted_image_part",
                "upload": upload, "offset": 0, "data": b64(&png_of(100))}),
        )
        .await;
    assert_eq!(ack["type"], "ack", "{ack}");
}

fn attachments_of(run: &crate::test_support::ScratchDir, id: &str) -> std::path::PathBuf {
    run.join(id).join("attachments")
}

/// The hidden spool files of the attachments directory, none where it does not exist.
fn spooled(run: &crate::test_support::ScratchDir, id: &str) -> usize {
    std::fs::read_dir(attachments_of(run, id))
        .map(|entries| {
            entries
                .filter(|entry| {
                    let name = entry.as_ref().unwrap().file_name();
                    let name = name.to_string_lossy();
                    name.starts_with(".upload-") && name.ends_with(".partial")
                })
                .count()
        })
        .unwrap_or(0)
}

async fn attachment_created(
    client: &mut Client,
    id: &str,
    session: &str,
    name: &str,
    size: usize,
) -> String {
    let reply = client
        .ask(
            id,
            serde_json::json!({"id": id, "type": "create_attachment_upload",
                "session": session, "name": name, "size": size}),
        )
        .await;
    assert_eq!(reply["type"], "attachment_upload_created", "{reply}");
    reply["upload"].as_str().unwrap().to_string()
}

async fn attachment_part(
    client: &mut Client,
    id: &str,
    upload: &str,
    offset: usize,
    part: &[u8],
) -> serde_json::Value {
    client
        .ask(
            id,
            serde_json::json!({"id": id, "type": "write_attachment_part",
                "upload": upload, "offset": offset, "data": b64(part)}),
        )
        .await
}

async fn attachment_save(
    client: &mut Client,
    id: &str,
    session: &str,
    upload: &str,
) -> serde_json::Value {
    client
        .ask(
            id,
            serde_json::json!({"id": id, "type": "save_attachment",
                "session": session, "upload": upload}),
        )
        .await
}

#[test]
fn the_snapshot_carries_the_limits_of_an_attached_file() {
    let (state, _dir) = crate::test_support::app_state("snapshot-attachments");
    let json = serde_json::to_value(snapshot(&state).unwrap()).unwrap();
    assert_eq!(
        json["attachments"],
        serde_json::json!({"max_bytes": 104_857_600, "max_part_bytes": 1_048_576})
    );
}

#[tokio::test]
async fn a_file_uploaded_in_parts_sent_without_waiting_for_acks_is_saved_as_the_file_of_its_bytes()
{
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let bytes = png_of(3000);
    let upload = attachment_created(&mut client, "create", r.id, "report.pdf", bytes.len()).await;
    assert_eq!(
        spooled(&r.run, r.id),
        1,
        "the spool file exists while uploading"
    );

    // Many small parts and the save go out back to back, served in the order they came.
    let parts: Vec<_> = bytes.chunks(100).collect();
    for (n, part) in parts.iter().enumerate() {
        client
            .send(
                serde_json::json!({"id": format!("part-{n}"), "type": "write_attachment_part",
                "upload": upload, "offset": n * 100, "data": b64(part)}),
            )
            .await;
    }
    client
        .send(serde_json::json!({"id": "save", "type": "save_attachment",
            "session": r.id, "upload": upload}))
        .await;
    for n in 0..parts.len() {
        let ack = client.reply(&format!("part-{n}")).await;
        assert_eq!(ack["type"], "ack", "{ack}");
    }
    let saved = client.reply("save").await;
    assert_eq!(saved["type"], "attachment_saved", "{saved}");
    let path = std::path::PathBuf::from(saved["path"].as_str().unwrap());
    assert!(path.is_absolute(), "{path:?}");
    assert_eq!(path.parent().unwrap(), attachments_of(&r.run, r.id));
    assert_eq!(path.file_name().unwrap(), "report.pdf");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(spooled(&r.run, r.id), 0);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    // Saved, the upload is gone.
    let again = attachment_save(&mut client, "again", r.id, &upload).await;
    assert_eq!(again["code"], "upload_unavailable", "{again}");
    assert_eq!(again["params"]["upload"], upload.as_str());
}

#[tokio::test]
async fn two_files_saved_with_one_name_are_both_kept_the_second_numbered() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let mut paths = Vec::new();
    for (n, content) in [b"first".as_slice(), b"second"].into_iter().enumerate() {
        let upload = attachment_created(
            &mut client,
            &format!("c{n}"),
            r.id,
            "report.pdf",
            content.len(),
        )
        .await;
        let ack = attachment_part(&mut client, &format!("p{n}"), &upload, 0, content).await;
        assert_eq!(ack["type"], "ack", "{ack}");
        let saved = attachment_save(&mut client, &format!("s{n}"), r.id, &upload).await;
        assert_eq!(saved["type"], "attachment_saved", "{saved}");
        paths.push(std::path::PathBuf::from(saved["path"].as_str().unwrap()));
    }
    assert_eq!(paths[0].file_name().unwrap(), "report.pdf");
    assert_eq!(paths[1].file_name().unwrap(), "report-2.pdf");
    assert_eq!(std::fs::read(&paths[0]).unwrap(), b"first");
    assert_eq!(std::fs::read(&paths[1]).unwrap(), b"second");
}

#[tokio::test]
async fn a_name_that_climbs_out_is_saved_inside_the_attachments_directory() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let upload = attachment_created(&mut client, "create", r.id, "../../x", 1).await;
    attachment_part(&mut client, "part", &upload, 0, b"x").await;
    let saved = attachment_save(&mut client, "save", r.id, &upload).await;
    let path = std::path::PathBuf::from(saved["path"].as_str().unwrap());
    assert_eq!(path.parent().unwrap(), attachments_of(&r.run, r.id));
    assert!(!r.run.join("x").exists());
}

#[tokio::test]
async fn parts_that_end_short_are_refused_at_the_save_and_leave_no_file() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let bytes = png_of(3000);
    let upload = attachment_created(&mut client, "create", r.id, "a.bin", bytes.len()).await;
    for (n, part) in bytes.chunks(1000).take(2).enumerate() {
        let ack = attachment_part(&mut client, &format!("part-{n}"), &upload, n * 1000, part).await;
        assert_eq!(ack["type"], "ack");
    }
    let refused = attachment_save(&mut client, "save", r.id, &upload).await;
    assert_eq!(refused["type"], "error", "{refused}");
    assert_eq!(refused["code"], "upload_parts_mismatch");
    assert_eq!(refused["params"]["expected_offset"], "2000");
    assert_eq!(refused["params"]["size"], "3000");
    assert_eq!(spooled(&r.run, r.id), 0);
    let again = attachment_part(&mut client, "again", &upload, 2000, &bytes[2000..]).await;
    assert_eq!(
        again["code"], "upload_unavailable",
        "the refusal discarded it: {again}"
    );
    let files = std::fs::read_dir(attachments_of(&r.run, r.id))
        .unwrap()
        .count();
    assert_eq!(files, 0, "nothing was saved");
}

#[tokio::test]
async fn a_part_out_of_place_or_past_the_size_is_refused_and_discards_the_upload_and_its_file() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let upload = attachment_created(&mut client, "create", r.id, "a.bin", 2000).await;
    let refused = attachment_part(&mut client, "skip", &upload, 1000, &[1; 1000]).await;
    assert_eq!(refused["code"], "upload_parts_mismatch", "{refused}");
    assert_eq!(refused["params"]["expected_offset"], "0");
    assert_eq!(refused["params"]["size"], "2000");
    assert_eq!(spooled(&r.run, r.id), 0);
    let save = attachment_save(&mut client, "save", r.id, &upload).await;
    assert_eq!(save["code"], "upload_unavailable", "{save}");

    let upload = attachment_created(&mut client, "create-2", r.id, "a.bin", 2000).await;
    let refused = attachment_part(&mut client, "past", &upload, 0, &[1; 2001]).await;
    assert_eq!(refused["code"], "upload_parts_mismatch", "{refused}");
    assert_eq!(spooled(&r.run, r.id), 0);
}

#[tokio::test]
async fn a_part_over_one_mebibyte_is_refused_and_discards_the_upload_and_its_file() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let upload = attachment_created(&mut client, "create", r.id, "a.bin", 3 * 1024 * 1024).await;
    let refused = attachment_part(&mut client, "big", &upload, 0, &vec![1; 1024 * 1024 + 1]).await;
    assert_eq!(refused["code"], "limit_exceeded", "{refused}");
    assert_eq!(refused["params"]["limit"], "upload_part_bytes");
    assert_eq!(refused["params"]["max"], "1048576");
    assert_eq!(spooled(&r.run, r.id), 0);
    let after = attachment_part(&mut client, "after", &upload, 0, b"x").await;
    assert_eq!(after["code"], "upload_unavailable", "{after}");
}

#[tokio::test]
async fn data_that_is_not_base64_is_refused_as_a_plain_error_and_discards_the_upload() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let upload = attachment_created(&mut client, "create", r.id, "a.bin", 100).await;
    let refused = client
        .ask(
            "bad",
            serde_json::json!({"id": "bad", "type": "write_attachment_part",
                "upload": upload, "offset": 0, "data": "not base64 !!!"}),
        )
        .await;
    assert_eq!(refused["type"], "error", "{refused}");
    assert_eq!(refused["code"], "internal_error", "{refused}");
    assert!(
        refused["params"]["detail"]
            .as_str()
            .unwrap()
            .contains("base64"),
        "{refused}"
    );
    assert_eq!(spooled(&r.run, r.id), 0);
}

#[tokio::test]
async fn a_deleted_attachment_upload_is_acknowledged_also_when_gone_and_leaves_no_file() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let upload = attachment_created(&mut client, "create", r.id, "a.bin", 100).await;
    attachment_part(&mut client, "part", &upload, 0, &[1; 10]).await;
    assert_eq!(spooled(&r.run, r.id), 1);
    for id in ["delete", "delete-again", "delete-unknown"] {
        let handle = if id == "delete-unknown" {
            "attachment-404"
        } else {
            &upload
        };
        let reply = client
            .ask(
                id,
                serde_json::json!({"id": id, "type": "delete_attachment_upload",
                    "upload": handle}),
            )
            .await;
        assert_eq!(reply["type"], "ack", "{reply}");
    }
    assert_eq!(spooled(&r.run, r.id), 0);
    let save = attachment_save(&mut client, "save", r.id, &upload).await;
    assert_eq!(save["code"], "upload_unavailable", "{save}");
}

#[tokio::test]
async fn an_attachment_upload_is_refused_for_an_unknown_session_one_not_running_and_a_bad_size() {
    use crate::protocol::SessionStatus;
    let r = running_session();
    crate::state::test_support::insert_record(
        &r.state,
        &r._dir,
        "stopped-session",
        SessionStatus::Working,
    );
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    for (session, size, code) in [
        ("no-such-session", 10u64, "unknown_session"),
        ("stopped-session", 10, "session_not_running"),
        (r.id, 0, "internal_error"),
        (r.id, 100 * 1024 * 1024 + 1, "internal_error"),
    ] {
        let reply = client
            .ask(
                "create",
                serde_json::json!({"id": "create", "type": "create_attachment_upload",
                    "session": session, "name": "a.bin", "size": size}),
            )
            .await;
        assert_eq!(reply["type"], "error", "{session} {size}: {reply}");
        assert_eq!(reply["code"], code, "{session} {size}: {reply}");
    }
    assert_eq!(spooled(&r.run, r.id), 0);
    // The largest file is taken.
    attachment_created(&mut client, "largest", r.id, "a.bin", 100 * 1024 * 1024).await;
}

#[tokio::test]
async fn a_third_attachment_upload_on_one_connection_is_refused_and_another_has_its_own_two() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    attachment_created(&mut client, "one", r.id, "a.bin", 10).await;
    attachment_created(&mut client, "two", r.id, "b.bin", 10).await;
    let third = client
        .ask(
            "three",
            serde_json::json!({"id": "three", "type": "create_attachment_upload",
                "session": r.id, "name": "c.bin", "size": 10}),
        )
        .await;
    assert_eq!(third["code"], "limit_exceeded", "{third}");
    assert_eq!(third["params"]["limit"], "attachment_uploads");
    assert_eq!(third["params"]["max"], "2");
    assert_eq!(spooled(&r.run, r.id), 2, "the refused upload left no file");

    let mut other = Client::connect(port).await;
    attachment_created(&mut other, "other", r.id, "d.bin", 10).await;
}

#[tokio::test]
async fn declared_sizes_across_connections_are_held_to_the_attachment_budget() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let largest = 100 * 1024 * 1024;
    let mut clients = Vec::new();
    for n in 0..3 {
        clients.push(Client::connect(port).await);
        let count = if n == 2 { 1 } else { 2 };
        for k in 0..count {
            attachment_created(
                &mut clients[n],
                &format!("c{n}-{k}"),
                r.id,
                "big.bin",
                largest,
            )
            .await;
        }
    }
    // 500 MiB are declared; 12 MiB are left of the 512.
    let left = 12 * 1024 * 1024;
    let refused = clients[2]
        .ask(
            "over",
            serde_json::json!({"id": "over", "type": "create_attachment_upload",
                "session": r.id, "name": "more.bin", "size": left + 1}),
        )
        .await;
    assert_eq!(refused["code"], "limit_exceeded", "{refused}");
    assert_eq!(refused["params"]["limit"], "attachment_upload_bytes");
    assert_eq!(refused["params"]["max"], (512 * 1024 * 1024).to_string());
    attachment_created(&mut clients[2], "fits", r.id, "rest.bin", left).await;
}

#[tokio::test]
async fn image_and_attachment_uploads_have_apart_limits_and_handles() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let image = created(&mut client, "i1", r.id, 100).await;
    created(&mut client, "i2", r.id, 100).await;
    let attachment = attachment_created(&mut client, "a1", r.id, "a.bin", 100).await;
    attachment_created(&mut client, "a2", r.id, "b.bin", 100).await;

    let reply = attachment_part(&mut client, "wrong-1", &image, 0, b"abc").await;
    assert_eq!(reply["code"], "upload_unavailable", "{reply}");
    let reply = client
        .ask(
            "wrong-2",
            serde_json::json!({"id": "wrong-2", "type": "write_pasted_image_part",
                "upload": attachment, "offset": 0, "data": b64(b"abc")}),
        )
        .await;
    assert_eq!(reply["code"], "upload_unavailable", "{reply}");
    // The first refusal discarded nothing of the image upload, the second nothing of the file's.
    let ack = client
        .ask(
            "image-ok",
            serde_json::json!({"id": "image-ok", "type": "write_pasted_image_part",
                "upload": image, "offset": 0, "data": b64(&png_of(100))}),
        )
        .await;
    assert_eq!(ack["type"], "ack", "{ack}");
    let ack = attachment_part(&mut client, "attachment-ok", &attachment, 0, &[1; 100]).await;
    assert_eq!(ack["type"], "ack", "{ack}");
}

#[tokio::test]
async fn a_save_for_another_session_is_unavailable_and_discards_the_upload_and_its_file() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let upload = attachment_created(&mut client, "create", r.id, "a.bin", 3).await;
    attachment_part(&mut client, "part", &upload, 0, b"abc").await;
    let refused = attachment_save(&mut client, "other", "another-session", &upload).await;
    assert_eq!(refused["code"], "upload_unavailable", "{refused}");
    assert_eq!(spooled(&r.run, r.id), 0);
    let again = attachment_save(&mut client, "again", r.id, &upload).await;
    assert_eq!(again["code"], "upload_unavailable", "{again}");
}

#[tokio::test]
async fn a_save_after_the_scratch_directory_was_removed_is_not_running_and_recreates_nothing() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let upload = attachment_created(&mut client, "create", r.id, "a.bin", 3).await;
    attachment_part(&mut client, "part", &upload, 0, b"abc").await;
    std::fs::remove_dir_all(r.run.join(r.id)).unwrap();
    let refused = attachment_save(&mut client, "save", r.id, &upload).await;
    assert_eq!(refused["code"], "session_not_running", "{refused}");
    assert!(!r.run.join(r.id).exists(), "nothing was recreated");
}

#[tokio::test]
async fn a_connection_that_closes_removes_the_spool_files_and_gives_back_the_bytes() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let budget = r.state.attachment_uploads.clone();
    let full = budget.available();
    let mut client = Client::connect(port).await;
    let upload = attachment_created(&mut client, "create", r.id, "a.bin", 4096).await;
    attachment_part(&mut client, "part", &upload, 0, &[1; 100]).await;
    attachment_created(&mut client, "second", r.id, "b.bin", 4096).await;
    assert_eq!(spooled(&r.run, r.id), 2);
    assert_eq!(budget.available(), full - 8192);
    drop(client);
    let gone = crate::test_support::within(PATIENCE, {
        let (run, budget) = (r.run.join(r.id), budget.clone());
        move || {
            budget.available() == full
                && std::fs::read_dir(run.join("attachments"))
                    .map(|entries| entries.count() == 0)
                    .unwrap_or(true)
        }
    })
    .await;
    assert!(gone, "the closed connection still holds its uploads");
}

#[tokio::test]
async fn an_attachment_request_whose_fields_do_not_read_is_unreadable_and_discards_its_upload() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let upload = attachment_created(&mut client, "create", r.id, "a.bin", 2000).await;
    let ack = attachment_part(&mut client, "ok", &upload, 0, &[1; 1000]).await;
    assert_eq!(ack["type"], "ack", "{ack}");
    assert_eq!(spooled(&r.run, r.id), 1);

    // A valid upload handle, an offset that is not a number.
    client
        .send(
            serde_json::json!({"id": "bad", "type": "write_attachment_part",
            "upload": upload, "offset": "1000", "data": b64(&[1; 1000])}),
        )
        .await;
    let unreadable = client.reply_with_code("unreadable_request").await;
    assert_eq!(unreadable["type"], "error", "{unreadable}");
    let after = attachment_part(&mut client, "after", &upload, 1000, &[1; 1000]).await;
    assert_eq!(after["code"], "upload_unavailable", "{after}");
    assert_eq!(spooled(&r.run, r.id), 0);
}

#[tokio::test]
async fn an_unreadable_attachment_request_discards_only_an_attachment_upload() {
    let r = running_session();
    let port = serve_control_of(r.state.clone(), Heartbeat::STANDARD).await;
    let mut client = Client::connect(port).await;
    let image = created(&mut client, "create", r.id, 100).await;
    // An attachment request naming the image upload's handle: not its family's to discard.
    client
        .send(
            serde_json::json!({"id": "bad", "type": "write_attachment_part",
            "upload": image, "offset": "0", "data": ""}),
        )
        .await;
    client.reply_with_code("unreadable_request").await;
    let ack = client
        .ask(
            "write",
            serde_json::json!({"id": "write", "type": "write_pasted_image_part",
                "upload": image, "offset": 0, "data": b64(&png_of(100))}),
        )
        .await;
    assert_eq!(ack["type"], "ack", "{ack}");

    // And an unreadable image request does not discard an attachment upload.
    let upload = attachment_created(&mut client, "a", r.id, "a.bin", 10).await;
    client
        .send(
            serde_json::json!({"id": "bad-2", "type": "write_pasted_image_part",
            "upload": upload, "offset": "0", "data": ""}),
        )
        .await;
    client.reply_with_code("unreadable_request").await;
    let ack = attachment_part(&mut client, "still", &upload, 0, &[1; 10]).await;
    assert_eq!(ack["type"], "ack", "{ack}");
}
