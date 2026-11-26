# Attaching a file to a session

A client that shares no file system with the daemon, such as a phone, hands a running session a file by uploading it
to the daemon's host: the daemon saves it in the session's scratch directory and answers with the saved file's absolute
path, which the client puts into the message it sends the agent. The agent reads the file with its own tools, as it
reads any file named by path. A remote window of the desktop application and a browser attach a file this way when it
is dropped onto a session's terminal, and paste the saved file's path into it (see "In a remote window and in a
browser" in `docs/product/dropping-files-onto-a-terminal.md`); with the message composer on, every window attaches
picked, pasted and dropped files to a message this way too (see "Attaching files" in
`docs/product/message-composer.md`).

An image meant for the agent to take as an image is not attached this way: it is saved as a pasted image (see
`docs/product/pasting-images.md`), whose path an agent that takes pasted images takes as an attached image rather than
a file to read; for an agent that does not, a dropped image is attached like any other file.

## Whether a daemon takes attachments

A daemon that takes attachments says so in its `snapshot` with `attachments`, carrying `max_bytes` (the largest file it
takes) and `max_part_bytes` (the largest part). A daemon that predates attachments sends no `attachments` and answers
the attachment requests with `unreadable_request`; a client connected to one attaches no file: a window takes only
dropped images it can save as pasted images (see "How each file is sent" in
`docs/product/dropping-files-onto-a-terminal.md`).

## Uploading and saving a file

A file always goes to the daemon in parts, so that no single message takes longer to arrive than the daemon waits on a
silent connection:

1. `create_attachment_upload` names the session, the file's name where it came from, and its size. The session must
   be running or interrupted. It is answered with `attachment_upload_created`, carrying the upload's handle.
2. `write_attachment_part` adds each part, in order, exactly as an image's parts are added (see "Uploading the image in
   parts" in `docs/product/pasting-images.md`). Each is answered with `ack`.
3. `save_attachment` names the session and the upload, and saves the file once all of its bytes have arrived. It is
   answered with `attachment_saved`, carrying `path`, the absolute path of the saved file as the daemon's host spells
   paths.

`delete_attachment_upload` discards an upload, and is answered with `ack` also for an upload the connection no longer
holds.

| Request | Fields | Answer |
|---|---|---|
| `create_attachment_upload` | `session`, `name`, `size` | `attachment_upload_created` (`id`, `upload`) |
| `write_attachment_part` | `upload`, `offset` (the bytes received before this part), `data` (base64) | `ack` |
| `save_attachment` | `session`, `upload` | `attachment_saved` (`id`, `path`) |
| `delete_attachment_upload` | `upload` | `ack` |

An upload belongs to the connection that started it and is discarded on the same occasions as an image upload: when it
is deleted, when any of its requests is refused, 60 seconds after it was started or last received a part, or when the
connection closes. A save, refused or not, ends the upload.

## Limits

- **A file is 1 byte to 100 MiB.**
- **A part carries at most 1 MiB**, decoded.
- **A connection holds at most 2 attachment uploads at once**, and every connection's attachment uploads together at
  most 512 MiB of declared sizes. These are counted apart from image uploads, which have limits of their own.

## Where the file is kept

The parts are written as they arrive to a hidden file in the session's scratch directory,
`~/.octoboard/run/<session id>/attachments/`, on the daemon's host, readable by its owner alone; the save gives that
file its name, so the saved file is owner-only too. An upload that is not saved leaves nothing behind: its hidden file
is removed when the upload is discarded. A saved file is removed with the scratch directory, when the session's process
ends, or at the next daemon start for a session whose process that start does not take over (see
`~/.octoboard/run/<session id>/` in `docs/product/files-octoboard-owns.md`), unless a message put on the session's
composer queue has moved it into the session's composer directory first (see "Files of queued messages" in
`docs/product/composer-queue.md`). For an interrupted session, which has no scratch directory, the parts and the saved
file are in its composer directory, `~/.octoboard/composer/<session id>/attachments/`, instead.

## The saved file's name

The saved file is named after a safe form of the name the client gave:

- only the last part of the name after any `/` or `\` is kept;
- every character other than a letter or digit (of any script), `.`, `-` or `_` becomes `_`, and a run of them a single
  `_`;
- the name never starts with `.`, `-` or `_`, and never ends with `.` or `_`;
- a Windows device name (`CON`, `PRN`, `AUX`, `NUL`, `COM1` to `COM9`, `LPT1` to `LPT9`, in any case and whatever its
  extension) is given a leading `_`;
- a name longer than 120 characters or 200 bytes of UTF-8 is shortened from the end of the part before its extension,
  keeping an extension of up to 16 characters, its dot included, together with a `.tar` before it (`logs.tar.gz`);
- a name with nothing left is `attachment`.

A saved file therefore never lands outside the attachments directory, and its path holds no whitespace, quotes or
anything else an agent's path detection or a shell splits on, so it can be put into a message bare. Where a file of
that name is there already, the save numbers the new one before its extension: `report-2.pdf`, `logs-3.tar.gz`.

## When attaching fails

No refusal is particular to attachments:

- `unknown_session` for a session id that names no session, and `session_not_running` for an archived session or one
  starting or ending, at the start or at the save;
- `limit_exceeded` past a limit: `limit` is `attachment_uploads` past the uploads of one connection,
  `attachment_upload_bytes` past the declared sizes of all connections, and `upload_part_bytes` for a part over 1 MiB;
- `upload_unavailable` for an upload the connection does not hold (never started on it, already saved, deleted,
  discarded after a refusal, or expired), for an image upload's handle given to an attachment request or the reverse,
  and for a save naming another session than the upload's;
- `upload_parts_mismatch` for a part that does not start where the received bytes end or runs past the declared size,
  and for a save before all of the declared bytes have arrived;
- a size of nothing or over 100 MiB, a part whose data is not base64, and a write that fails are errors with no code.
