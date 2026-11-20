//! The requests about a connection's uploads, a pasted image's or an attached file's, which the
//! connection serves itself, since it holds the uploads; see `crate::session::upload`.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::protocol::{Event, Request, RequestBody};
use crate::session::attachment::AttachmentUploads;
use crate::session::pasted_image::ImageUploads;
use crate::state::AppState;

/// The `type`s of the requests about a pasted image upload.
const IMAGE_UPLOAD_REQUEST_TYPES: &[&str] = &[
    "create_pasted_image_upload",
    "write_pasted_image_part",
    "delete_pasted_image_upload",
    "save_pasted_image",
];

/// The `type`s of the requests about an attachment upload.
const ATTACHMENT_UPLOAD_REQUEST_TYPES: &[&str] = &[
    "create_attachment_upload",
    "write_attachment_part",
    "delete_attachment_upload",
    "save_attachment",
];

/// One connection's uploads, each family on its own, so a handle of one is unknown to the other.
/// Dropped with the connection, and with them their parts.
pub(super) struct ConnectionUploads {
    images: ImageUploads,
    /// Shared with the blocking threads its spool files are written on.
    attachments: Arc<AttachmentUploads>,
}

impl ConnectionUploads {
    pub(super) fn new(state: &AppState) -> Self {
        Self {
            images: ImageUploads::images(state.pasted_image_uploads.clone()),
            attachments: Arc::new(AttachmentUploads::attachments(
                state.attachment_uploads.clone(),
            )),
        }
    }

    /// Discards the upload `upload` of the family the request type `kind` belongs to.
    pub(super) fn discard(&self, kind: &str, upload: &str) {
        if ATTACHMENT_UPLOAD_REQUEST_TYPES.contains(&kind) {
            self.attachments.delete(upload);
        } else {
            self.images.delete(upload);
        }
    }
}

/// What [`upload_request`] makes of a frame.
pub(super) enum UploadFrame {
    /// A request this connection serves itself.
    Serve(Box<Request>),
    /// A frame of an upload request type (`kind`) whose fields do not read, naming this upload:
    /// it is discarded, and the frame answered by the ordinary request path.
    Unreadable { kind: String, upload: String },
}

/// The frame as a request about one of this connection's uploads, when its `type` (`kind`) is
/// one: `None` for any other frame, `save_pasted_image` with no `upload` included, which the
/// ordinary request path serves. A frame of one of these types whose fields do not read is left
/// to that path too, which answers it, with the upload it names, if any, to discard.
pub(super) fn upload_request(text: &str, kind: Option<&str>) -> Option<UploadFrame> {
    /// The `upload` of a frame, read without the rest of its fields: a single-frame save's
    /// `data` is skipped, not copied.
    #[derive(serde::Deserialize)]
    struct Named {
        #[serde(default)]
        upload: Option<String>,
    }
    let kind = kind?;
    if !IMAGE_UPLOAD_REQUEST_TYPES.contains(&kind)
        && !ATTACHMENT_UPLOAD_REQUEST_TYPES.contains(&kind)
    {
        return None;
    }
    let upload = serde_json::from_str::<Named>(text)
        .ok()
        .and_then(|named| named.upload);
    if kind == "save_pasted_image" && upload.is_none() {
        return None;
    }
    match serde_json::from_str::<Request>(text) {
        Ok(request) => Some(UploadFrame::Serve(Box::new(request))),
        Err(_) => upload.map(|upload| UploadFrame::Unreadable {
            kind: kind.to_string(),
            upload,
        }),
    }
}

/// Serves a request about one of this connection's uploads, in the order the frames came, so
/// that a client may send the parts of an upload without waiting for each `ack`. A part of an
/// attached file is written on a blocking thread but awaited here, so the parts still reach its
/// spool file in order; a create and a save leave this loop once what has to happen in order is
/// done (nothing names an upload before its create is answered, and a save has taken its upload).
pub(super) async fn serve_upload(
    state: &Arc<AppState>,
    uploads: &Arc<ConnectionUploads>,
    outbound: &mpsc::Sender<Event>,
    request: Request,
) {
    let id = request.id;
    let answer = match request.body {
        // Off this loop, since it reads the store.
        RequestBody::CreatePastedImageUpload { session, size } => {
            let (state, uploads) = (state.clone(), uploads.clone());
            reply_off_loop(
                outbound,
                id,
                move || uploads.images.create(&state, &session, size),
                |id, upload| Event::PastedImageUploadCreated { id, upload },
            );
            return;
        }
        RequestBody::WritePastedImagePart {
            upload,
            offset,
            data,
        } => uploads
            .images
            .write(&upload, offset, &data)
            .map(|()| Event::Ack { id: id.clone() }),
        RequestBody::DeletePastedImageUpload { upload } => {
            uploads.images.delete(&upload);
            Ok(Event::Ack { id: id.clone() })
        }
        RequestBody::SavePastedImage {
            session,
            data,
            upload: Some(upload),
        } => {
            if data.is_some() {
                uploads.images.delete(&upload);
                Err(anyhow::anyhow!(
                    "save_pasted_image takes exactly one of data and upload"
                ))
            } else {
                match uploads.images.take(&upload, &session) {
                    Ok(taken) => {
                        let state = state.clone();
                        reply_off_loop(
                            outbound,
                            id,
                            // The bytes stay counted against the uploads' budget until written.
                            move || {
                                crate::session::pasted_image::save_bytes(
                                    &state,
                                    &session,
                                    &taken.body,
                                )
                            },
                            saved_as(|id, path| Event::PastedImageSaved { id, path }),
                        );
                        return;
                    }
                    Err(err) => Err(err),
                }
            }
        }
        // Off this loop, since it reads the store and creates the spool file.
        RequestBody::CreateAttachmentUpload {
            session,
            name,
            size,
        } => {
            let (state, attachments) = (state.clone(), uploads.attachments.clone());
            reply_off_loop(
                outbound,
                id,
                move || attachments.create(&state, &session, &name, size),
                |id, upload| Event::AttachmentUploadCreated { id, upload },
            );
            return;
        }
        RequestBody::WriteAttachmentPart {
            upload,
            offset,
            data,
        } => {
            let attachments = uploads.attachments.clone();
            tokio::task::spawn_blocking(move || attachments.write(&upload, offset, &data))
                .await
                .map_err(anyhow::Error::from)
                .and_then(|written| written)
                .map(|()| Event::Ack { id: id.clone() })
        }
        RequestBody::DeleteAttachmentUpload { upload } => {
            uploads.attachments.delete(&upload);
            Ok(Event::Ack { id: id.clone() })
        }
        RequestBody::SaveAttachment { session, upload } => {
            match uploads.attachments.take(&upload, &session) {
                Ok(taken) => {
                    let state = state.clone();
                    reply_off_loop(
                        outbound,
                        id,
                        move || crate::session::attachment::save(&state, &session, taken),
                        saved_as(|id, path| Event::AttachmentSaved { id, path }),
                    );
                    return;
                }
                Err(err) => Err(err),
            }
        }
        _ => unreachable!("only upload requests are served here"),
    };
    let reply = answer.unwrap_or_else(|err| Event::from_error(id, &err));
    let _ = outbound.send(reply).await;
}

/// Runs `work` on a blocking thread off the connection's loop and queues its answer, `to_event` of
/// what it gave or the error it failed with, for the request `id`.
fn reply_off_loop<T: Send + 'static>(
    outbound: &mpsc::Sender<Event>,
    id: Option<String>,
    work: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
    to_event: impl FnOnce(Option<String>, T) -> Event + Send + 'static,
) {
    let outbound = outbound.clone();
    tokio::spawn(async move {
        let done = tokio::task::spawn_blocking(work)
            .await
            .map_err(anyhow::Error::from)
            .and_then(|done| done);
        let reply = match done {
            Ok(done) => to_event(id, done),
            Err(err) => Event::from_error(id, &err),
        };
        let _ = outbound.send(reply).await;
    });
}

/// The answer of a save, the path written as the daemon's host spells it.
fn saved_as(
    event: impl FnOnce(Option<String>, String) -> Event + Send + 'static,
) -> impl FnOnce(Option<String>, PathBuf) -> Event + Send + 'static {
    move |id, path| event(id, path.to_string_lossy().into_owned())
}
