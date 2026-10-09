//! The limits every browse read is held to, in one place so they are read together: each one
//! bounds memory or time on its own, and the retained-bytes reservation is derived from the
//! others. The values and the measurements behind them are recorded in `apps/daemon/PROTOCOL.md`'s
//! "Browse budgets" section; a change here is a change there.

use std::time::Duration;

/// The largest file body a read returns. Sized from the tracked files of a set of real projects:
/// 99.9 % are under 2.4 MB, the largest text file seen was a 2.3 MB string catalog and the largest
/// image 2.6 MB, so 4 MiB opens all of those with room, while a multi-megabyte binary nobody reads
/// in a viewer (a `.wasm`, a model file) is refused rather than shipped.
pub const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;

/// The most entries one directory listing carries before it is cut and marked incomplete. The
/// largest directories seen in real projects held 6,418 entries (a Gradle cache) and 4,536 (an
/// icon package under `node_modules`); 99.99 % held under 1,600.
pub const MAX_DIR_ENTRIES: usize = 10_000;

/// The most name bytes, as sent on the wire, one listing carries before it is cut and marked
/// incomplete — the bound that holds when the entries are few but their names are long. Ten
/// thousand typical names come to well under a tenth of this.
pub const MAX_LISTING_NAME_BYTES: usize = 2 * 1024 * 1024;

/// The most stdout a metadata `git` command (`rev-parse`, `worktree list`, `ls-files` and
/// `ls-tree` for one path) may print. A `worktree list` entry is about 200 bytes, so this is
/// thousands of worktrees; past it the command is killed rather than read further.
pub const GIT_METADATA_STDOUT: usize = 1024 * 1024;

/// The most stderr kept from any `git` command. A `git` error message is a few hundred bytes; the
/// rest of a longer one is read and discarded.
pub const GIT_STDERR: usize = 64 * 1024;

/// Upper bound on one `git` read. Every browse read is local — lazy fetching of missing objects is
/// switched off — so this is a guard against a wedged repository (a dead network volume, a held
/// lock), not a network allowance.
pub const GIT_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// How many browse reads run at once across the whole daemon, each one a blocking thread doing
/// file I/O or waiting on one `git`. Enough for a viewer, its neighbour and a couple of directory
/// expansions in parallel; anything past it waits its turn. A file read has no deadline of its
/// own — the operating system gives no portable way to abandon a blocked one — so a volume that
/// stops answering can hold these until it answers or fails.
pub const MAX_CONCURRENT_READS: usize = 4;

/// How many browse requests one control connection may have outstanding — waiting for a turn,
/// being read, or queued until their frame is written to the socket. A request past this is
/// refused with `limit_exceeded` at once rather than queued, so a client that floods the socket
/// cannot grow the daemon's backlog.
pub const MAX_PENDING_REQUESTS: usize = 16;

/// What a file read reserves out of the retained bytes before it starts: its worst case, three
/// times the file budget plus room for the rest of the frame. A text body is held once as read and
/// once serialized, where JSON escaping at most doubles it (a body that escaping would grow further
/// is sent as binary); a binary body is held base64-encoded (four thirds of it) and then
/// serialized again. Once the reply is serialized, the reservation shrinks to the frame's real
/// length. A project source reserves the same: its `git` output is bounded far below it, but what
/// that becomes on the wire is not worth a separate bound.
pub const READ_RESERVATION: usize = 3 * MAX_FILE_BYTES + 64 * 1024;

/// What a directory listing reserves: its names at [`MAX_LISTING_NAME_BYTES`] held as read and as
/// serialized, and the rest of each of [`MAX_DIR_ENTRIES`] entries (kind, size, link target and
/// the JSON around them, under 200 bytes) likewise.
pub const LISTING_RESERVATION: usize = 2 * (MAX_LISTING_NAME_BYTES + MAX_DIR_ENTRIES * 200);

/// The part of [`MAX_RETAINED_BYTES`] one connection may hold: enough for
/// [`MAX_CONCURRENT_READS`] file reads at their worst at once, so one window gets every read the
/// daemon runs at a time, while a client that stops reading its socket still leaves the rest to
/// every other connection.
pub const MAX_RETAINED_PER_CONNECTION: usize = MAX_CONCURRENT_READS * READ_RESERVATION;

/// The most bytes the daemon holds for browse replies at once, across every connection: each
/// read's buffers while it runs, and each finished reply until its frame is written to the socket.
/// Twice one connection's share, so a second window still gets a full share beside the first.
pub const MAX_RETAINED_BYTES: usize = 2 * MAX_RETAINED_PER_CONNECTION;

const _: () = assert!(LISTING_RESERVATION <= READ_RESERVATION);
const _: () = assert!(READ_RESERVATION <= MAX_RETAINED_PER_CONNECTION);
const _: () = assert!(MAX_RETAINED_PER_CONNECTION <= MAX_RETAINED_BYTES);
