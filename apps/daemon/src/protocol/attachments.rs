//! What the snapshot tells a client of the files it can attach to a session.

use serde::Serialize;

/// The limits of an attachment upload (`create_attachment_upload`). A daemon that sends this in its
/// `snapshot` takes attachments; one that predates them sends no `attachments`, and a client then
/// offers no way to attach a file.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttachmentLimits {
    /// The largest file taken, in bytes.
    pub max_bytes: u64,
    /// The most one `write_attachment_part` carries, in bytes decoded.
    pub max_part_bytes: u64,
}
