# Pasting an image into a session's terminal

An image on the clipboard pasted into an agent session's terminal reaches the agent as an attached image: Octoboard
saves it as a file on the machine the daemon runs on and pastes that file's absolute path into the terminal, as a
system terminal pastes the path of an image file dragged onto it. Claude Code, Codex and Grok Build each take a pasted
path of an image file as an attached image (`[Image #1]` in their input). Because the agent reads the file, not the
clipboard, this works for every agent, also when the daemon runs on another machine than the window, and in a browser
as well as in the desktop app. Pasting text, and the editing menu itself, are described in
`docs/product/terminal-copy-paste-and-selection.md`.

## Which pastes paste an image

- **Only a session's terminal**: the terminal of the selected project or console session, whose program is its agent.
  A shell terminal in the Terminal panel pastes no image: a clipboard holding only an image is pasted there as an empty
  paste, as before.
- **Both ways of pasting**: the platform's paste shortcut (Cmd+V on macOS, Ctrl+V elsewhere) and Paste in the
  terminal's editing menu.
- **Only a paste with no text.** When the clipboard holds text, the text is pasted, whether or not it also holds an
  image.
- **The image** is the one the paste itself carries, or, where it carries none, the one read from the clipboard when
  the paste is made, never earlier. In a browser that read may ask for permission to read the clipboard.
- **Nothing to save means an empty paste, silently.** Where the platform cannot read an image from the clipboard (a
  browser that offers no clipboard reading), the clipboard holds no image, or reading it fails or is refused, an empty
  paste is sent, with no alert. An agent may answer an empty paste by reading the clipboard itself, which only works
  on the machine the agent runs on.

## What is pasted

- **The path of the saved file**, as typed input subject to the program's bracketed paste mode, like any paste. It is
  pasted bare, or, when it contains whitespace, in single quotes as a POSIX shell quotes it (a single quote inside it
  written `'\''`), since an agent does not read an unquoted path with a space as an image.
- **Formats.** A PNG, JPEG, GIF or WebP image is saved as it is; an image in any other format is converted to PNG first.
  The file's extension follows its contents (`png`, `jpg`, `gif`, `webp`).
- **At most 10 MiB per image**, measured after any conversion.
- **The destination is checked again** once the image is read and once it is saved, either of which can take a
  permission prompt or a transfer: when by then another session is selected or the session can no longer take input,
  nothing is pasted, and the paste is never redirected to another session. A file already saved stays until it is
  removed with the rest (see "Where the image is kept" below).
- **Typing does not wait for the image.** The path is pasted once the file is saved, so text typed straight after an
  image paste can reach the agent before the image's path.

## Where the image is kept

Each image is a new file in the session's scratch directory, `~/.octoboard/run/<session id>/pasted/`, on the daemon's
host, named `paste-<milliseconds>-<n>.<extension>`, where `<n>` tells apart images saved within the same millisecond;
the name holds nothing an agent's path detection splits on. The file is readable by its owner alone. It is removed with
the scratch directory, when the session's process ends, or at the next daemon start for a session whose process that
start does not take over (see `~/.octoboard/run/<session id>/` in `docs/product/files-octoboard-owns.md`).

## When pasting an image fails

A failure pastes nothing and shows an error alert saying the image could not be pasted (see "The error alert" in
`docs/product/error-alerts.md`), with no remedy:

- an image over the limit: the alert says it is larger than 10 MiB;
- an image that cannot be converted to PNG: the alert says the image on the clipboard is in a format that cannot be
  converted;
- a session whose process has ended by the time the image reaches the daemon: refused as `session_not_running`, and the
  alert says the session is not running;
- any other failure of the daemon to save the file is shown as that failure.

**A large image over a slow connection.** The image goes to the daemon in a single message on the window's connection,
and nothing else from the window reaches the daemon while it is being sent. Where sending it takes longer than the
daemon waits on a silent connection, 45 seconds (see "Losing the daemon connection" in
`docs/product/application-lifecycle.md`) — a daemon on another machine reached over an uplink slower than about
2.5 Mbit/s, for an image near the limit — the daemon drops the connection, the window reconnects and the paste fails.

## The request

The window saves an image with the control request `save_pasted_image`:

| Field | Value |
|---|---|
| `session` | The session whose terminal the image is pasted into |
| `data` | The image file's bytes, base64-encoded: a PNG, JPEG, GIF or WebP file (judged by its signature, which also gives the file's extension), at most 10 MiB decoded |
| `upload` | In place of `data`, an upload of the same connection whose parts hold the image (see "Uploading the image in parts" below). Exactly one of `data` and `upload` is given; the window always sends `data` |

It is answered with `pasted_image_saved`, carrying `id` and `path`, the absolute path of the saved file as the
daemon's host spells paths. It is refused with `unknown_session` for an id that names no session and
`session_not_running` for a session with no process. Data over 10 MiB, data that is not one of the four formats or not
valid base64, and a write that fails are errors with no code.

### Uploading the image in parts

For a client whose link cannot carry the whole image in one message within the daemon's 45 seconds of silence, the
daemon also takes the image in parts; the window never uploads this way. The client starts an upload of the image's
size (`create_pasted_image_upload`), sends its bytes in order in parts of at most 1 MiB each
(`write_pasted_image_part`), and then saves it with `save_pasted_image` naming the upload. The saved file, its
formats, its 10 MiB limit and the answer are those of the single-message request.

The parts are held in the daemon's memory only, so an upload that is not saved leaves nothing behind. An upload
belongs to the connection that started it and is discarded when it is deleted (`delete_pasted_image_upload`), when
any of its requests is refused, 60 seconds after it was started or last received a part, or when the connection
closes; a request naming a discarded upload is refused with `upload_unavailable`, and parts that do not continue
where the received bytes end, run past the declared size, or end short of it at the save are refused with
`upload_parts_mismatch`. A connection holds at most 2 uploads at once, and all connections' uploads together at most
64 MiB of declared sizes; past either, starting an upload is refused with `limit_exceeded`. The requests' fields and
refusals in full are in "Uploading a pasted image in parts" in `apps/daemon/PROTOCOL.md`.
