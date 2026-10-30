# Handling paths and files in the daemon

## A path built from a hook's values is built as if an attacker chose them

Applies to daemon code that turns a value an agent's hook reported into a path it removes, moves or writes to — and
`Session.agent_session_id` is such a value, since `hook_callback` (`apps/daemon/src/server.rs`) records it from the
payload, as are the transcript paths the hooks report. `POST /hook/:session` takes no `Origin`, and the key it needs is
readable by any local process (it is in every session's command lines), so any local process can post to it
(`apps/daemon/PROTOCOL.md`, "Who may connect"), the agent's own model included once something it read steers it there.
The daemon trusting local processes does not make these values safe to name a path with.

So such a path holds to four conditions: the id must match the agent's real id shape exactly (a canonical UUID),
never be cut out of a reported path; the path is built from a records directory the daemon resolved itself and that
id, a reported path counting only when it lies inside that directory; only an entry at the agent's exact layout depth
is taken, since a name one level higher can be a whole directory's (a Claude project directory is named like
`-Users-me-work`); and no directory between the records directory and the entry may be a link. `record_paths` in
`apps/daemon/src/agent_records.rs` does all four; extend it rather than write a second one.

## Opening a path taken from text an agent could have written can run code

Applies to daemon code that hands a path to the host's opener (`/usr/bin/open` on macOS, `xdg-open` on Linux) because
the path appeared in text an agent, its prompt or a repository could have put there: a session's terminal output, a
rendered document, a hook's payload. An agent echoes whatever it read, so someone else can choose such a path. And the
opener does not only show: it runs an application or other bundle, a `.command` or shell script, an installer or any
file with an executable bit, and follows a `.webloc`, `.fileloc` or `.desktop` shortcut on to somewhere else.

So the daemon resolves such a path itself at the moment it opens it, never trusting a client's earlier answer; it must
lie inside a root the daemon chose once symbolic links are resolved; nothing launchable may sit anywhere along it; and
inside a root other users can write to (a system temp directory such as `/tmp`), the file and every directory down to
it must belong to the daemon's user and be closed to writes by group and others, or another user can swap it between
the check and the open. `resolve` in `apps/daemon/src/path_links.rs` does all of this; extend it rather than write a
second one.

## Act destructively on "it is gone" only when `NotFound` said so

Applies to daemon code that deletes, trashes or refuses something because a file or directory is missing — an agent's
record, an account directory, a worktree. `Path::exists`, `is_dir` and `is_file` answer false on any error (permission
denied, an I/O error, a volume not mounted), so their false proves nothing: judge absence only from `metadata` or
`symlink_metadata` failing with `ErrorKind::NotFound`, and take any other error as unknown. A place that reads as
empty is not proof either — an unmounted volume's mount point is an empty directory, and an agent account directory
holding none of the agent's record directories has never shown a record to be missing. So such a judgement has three
answers, and unknown never destroys anything; `agent_records::Resumability` is the shape for a session's
conversation.

## Read a project's files only through `browse::live`, its git directory and the agent's configuration without blocking

Applies to daemon code that reads a file's content from a project's directory or worktree — to count it, sniff it,
hash it, send it, or to read the configuration the project keeps while launching a session (`.mcp.json`,
`.claude/settings.local.json`, `.codex/config.toml`). Go through `live::read_file` (from a path relative to the scope)
or `live::read_resolved` (once the path has been resolved inside it), never `File::open`, `fs::read` or
`fs::read_to_string`. What sits in a project is whatever the user or an agent put there: a link leading out of the
project, a FIFO whose open blocks until a writer appears — holding one of the browse reads' few daemon-wide slots, or
the launch itself, with no deadline — a device that reads forever, a file swapped mid-read. `live` refuses each of
these and honours the request's cancel; a direct open does none of it, and a fixture of plain files cannot tell the
two apart.

Which reader a launch uses goes by who owns the file, not by where it lies. A file the project keeps in a directory
above it cannot go through `live`; a file the project does not own (in the agent's configuration directory, the home
directory, a system-wide directory, Octoboard's own data, a session's records) is read as the agent reads it, following
links. Neither may block: open with `O_NONBLOCK`, take only a regular file, and bound the bytes read. The second holds
even under the project's directory: a home directory opened as the project holds `~/.claude`, `~/.claude.json` and
`~/.codex`, which dotfile managers often link out of it, so `live` would refuse files the agent does read; and where the
project's `.claude` or `.codex` is the agent configuration directory in effect, the project-side files in it count as
the agent's too. In an adapter, `read_bounded_text` in `apps/daemon/src/adapter/files.rs` reads a file the project
keeps (through `live` inside the canonical project, as a regular file above it), `read_regular_text` a file it does not
own, and `files::is_agent_config_dir` tells whether a directory in the project's tree is the agent's configuration
directory.

A repository's own files are files an agent can plant too, though no listing shows them: the `.git` pointer file, a
linked worktree's `commondir`, reflogs, `packed-refs`, a remote's `HEAD`. A FIFO or a link to `/dev/zero` in their place
blocks or reads forever just the same, and they are read while watching a project, in a sweep's few slots and under the
repository's gate, where one stuck read stalls everything behind it. Find a worktree's git and common directories
through `hostfs::git_dir` (`git_dir`, `common_dir`) rather than reading `.git` or `commondir` yourself, and read any
other file in a git directory through `hostfs::open_regular` or `hostfs::read_regular` with a bound; `live` is no
answer here, since a linked worktree's git directory lies outside the project.

A refusal from these readers does not mean the agent cannot read the file. Where taking an unreadable file as absent is
the unsafe answer (an allowlist it may hold would be skipped), a regular file that is there but refused takes the safe
side.

## Decide whether two account directories are one by `account_dir_key`, never by comparing their text

Applies to daemon code that asks whether two agent configuration directories are the same one — an account's
`config_dir`, a session's, the default account's — to find a session's account, catch a collision, tell a repoint
from a respelling, or key state per directory. The stored text stays as the user typed it
(`account_dirs::normalize_account_dir` only normalises it lexically), and macOS's default file system is
case-insensitive, so `~/.Claude` and `~/.claude` are one directory there under two spellings. Compare
`account_dirs::account_dir_key` of both sides in Rust, and key a map or registry by it. A `WHERE config_dir = ?` in SQL
or a `==` on the strings compares bytes, and one such comparison among folded ones makes a case-only respelling read as
another directory in one place and the same one everywhere else, so state kept for the directory is dropped or
applied to the wrong one. An exact-text match is right only for asking whether a stored row changed since it was read
(a guard of the form `WHERE id = ?1 AND config_dir = ?2` with the text just read).

## Decide whether two directories are one, or one lies under another, by device and inode, never by their text

Applies to daemon code that asks whether two paths name one existing directory, or whether one lies inside another —
an already associated project, a trusted folder covering a project, a folder shared between projects — other than an
agent configuration directory. Neither normalised text nor `canonicalize` output answers it: a link inside a folder
can lead out of it, a link or a case-only respelling (macOS's default file system is case-insensitive) can name a
directory that is inside, whether `canonicalize` folds case is the file system's doing (it does on APFS, not on
Linux), and a bind mount or a macOS firmlink gives one directory two real paths. So compare the device and inode of
what each path resolves to, and for containment walk the resolved path's ancestors: `hostfs::DirectoryIdentities`
answers "is this one of these directories", `hostfs::place::Place` answers "does this lie in that one".

Decide which way a path that cannot be resolved (nothing there, unreadable, no inode, the look timed out) counts by
what a match does. Where a match grants something, as trust coverage does, it matches nothing, with no fallback to the
text. Where a match is what protects, as refusing a second project at one directory does, it falls back to the
normalised text. Where a missed match destroys something — moving a folder to the Trash, removing a linked worktree
— a path that cannot be resolved counts as a match, and that decision is made only in `apps/daemon/src/trash_safety.rs`
(`looked_at_for_trash` for a move to the Trash, `Worktrees` for a worktree's removal), whose module header says which
paths fall back to their text; it also protects the home directory, the root of a disk and a directory holding
Octoboard's data whatever else is found. Code that destroys a directory asks it; extend it rather than write a second.
The looks read the file system, so they run on blocking threads after the store's lock is released, each with a
deadline, and one that outran it is left to finish and, once it has run past its stuck threshold, recorded in a
`state::StalledPasses` under its key, so later decisions take the fallback at once instead of starting another thread a
stuck mount holds the same way; `hostfs::place::looked_within` and `coordinator::identities_within` are the shape.
`state.directory_looks` is one registry for every decision of this kind (trust, project removal, branch cleanup,
account removal), so a look with a shorter deadline than theirs — a quick check ahead of a confirmation — keeps
`hostfs::place::Looking::stuck_after` at the full deadline: a merely slow path recorded as stuck makes all of them take
a healthy path as unreachable. A decision that must not be cut short by what such an earlier look left running takes
`Stuck::Wait` instead of `Stuck::Skip`.
