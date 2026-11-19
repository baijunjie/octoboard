//! The content search against real directories and repositories: what each option matches, what
//! the walk leaves out, each bound ending the search marked incomplete, and the request served end
//! to end through a connection's lane.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;

use super::{root, search, Bounds, Compiled, Query, Sending, Summary, Taken};
use crate::browse::lane::{Frame, Gate, Lane};
use crate::browse::tests::{code, git_env, Fixture};
use crate::browse::{budget, Cancelled};
use crate::protocol::{
    error_code, BrowseBody, BrowseRequest, CodedError, ContentMatch, ContentSearchStop, Event,
    MatchRange,
};
use crate::test_support::{git, repo_with, symlink, ScratchDir, PATIENCE};
use crate::wire_path::RelPath;

fn write(dir: &Path, path: &str, body: &[u8]) {
    let path = dir.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn query(text: &str) -> Query {
    Query {
        text: text.to_string(),
        ..Query::default()
    }
}

/// What a search of `dir` (or of `directory` within it) found, its lines sorted by path and line,
/// how it ended and how many batches it was sent in.
fn run_in(
    dir: &Path,
    directory: Option<&str>,
    query: &Query,
    bounds: &Bounds,
) -> anyhow::Result<(Vec<ContentMatch>, Summary, usize)> {
    let scope = crate::hostfs::canonical(dir).unwrap();
    let directory = directory.map(|path| RelPath::parse(path).unwrap());
    let walked = root(&scope, directory.as_ref())?;
    let compiled = Compiled::new(query)?;
    let mut found = Vec::new();
    let mut batches = 0;
    let summary = search(
        &scope,
        &walked,
        &compiled,
        bounds,
        &AtomicBool::new(false),
        &mut |batch, _: &Sending| {
            batches += 1;
            found.extend(batch);
            Ok(Taken::Sent)
        },
    )?;
    found.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    Ok((found, summary, batches))
}

fn run(dir: &Path, query: &Query) -> (Vec<ContentMatch>, Summary) {
    let (found, summary, _) = run_in(dir, None, query, &Bounds::standard()).unwrap();
    (found, summary)
}

/// The `path:line` of each line found.
fn places(found: &[ContentMatch]) -> Vec<String> {
    found
        .iter()
        .map(|line| format!("{}:{}", line.path, line.line))
        .collect()
}

fn options_project() -> ScratchDir {
    let dir = ScratchDir::new("content-search-options");
    write(&dir, "a.txt", b"Needle here\nneedle\nneedles\nnothing\n");
    write(&dir, "b.ts", b"a needle\n");
    write(&dir, "sub/c.ts", b"needle in sub\n");
    write(&dir, "sub/d.txt", b"needle in sub\n");
    dir
}

#[test]
fn each_option_narrows_what_matches() {
    let dir = options_project();
    let (found, summary) = run(&dir, &query("needle"));
    assert_eq!(
        places(&found),
        [
            "a.txt:1",
            "a.txt:2",
            "a.txt:3",
            "b.ts:1",
            "sub/c.ts:1",
            "sub/d.txt:1"
        ]
    );
    assert_eq!(
        summary,
        Summary {
            matches: 6,
            files: 4,
            complete: true,
            stopped_by: None
        }
    );
    let cased = Query {
        match_case: true,
        ..query("Needle")
    };
    assert_eq!(places(&run(&dir, &cased).0), ["a.txt:1"]);
    let word = Query {
        whole_word: true,
        ..query("needle")
    };
    assert!(!places(&run(&dir, &word).0).contains(&"a.txt:3".to_string()));
    assert_eq!(run(&dir, &word).0.len(), 5);
    // Without the regular-expression option the pattern is literal text.
    assert!(run(&dir, &query("need(le)s")).0.is_empty());
    let regex = Query {
        regex: true,
        ..query("need(le)s")
    };
    assert_eq!(places(&run(&dir, &regex).0), ["a.txt:3"]);
}

#[test]
fn the_file_mask_takes_names_paths_and_exclusions() {
    let dir = options_project();
    let masked = |mask: &str| {
        let masked = Query {
            file_mask: Some(mask.to_string()),
            ..query("needle")
        };
        places(&run(&dir, &masked).0)
    };
    assert_eq!(masked("*.ts"), ["b.ts:1", "sub/c.ts:1"]);
    assert_eq!(masked("*.ts, !sub/*"), ["b.ts:1"]);
    assert_eq!(masked("sub/*.txt"), ["sub/d.txt:1"]);
    assert_eq!(masked("!*.txt"), ["b.ts:1", "sub/c.ts:1"]);
    assert_eq!(masked(" , ").len(), 6);
}

#[test]
fn a_directory_limits_the_search_and_paths_stay_relative_to_the_project() {
    let dir = options_project();
    let (found, _, _) = run_in(&dir, Some("sub"), &query("needle"), &Bounds::standard()).unwrap();
    assert_eq!(places(&found), ["sub/c.ts:1", "sub/d.txt:1"]);
    assert_eq!(
        code(run_in(
            &dir,
            Some("missing"),
            &query("needle"),
            &Bounds::standard()
        )),
        error_code::FILE_NOT_FOUND
    );
    assert_eq!(
        code(run_in(
            &dir,
            Some("a.txt"),
            &query("needle"),
            &Bounds::standard()
        )),
        error_code::UNSUPPORTED_FILE_TYPE
    );
}

#[test]
fn ignored_files_are_searched_only_when_included() {
    let repo = repo_with(
        "content-search-ignored",
        &[
            (b".gitignore", b"build/\n*.log\n"),
            (b"kept.txt", b"needle\n"),
        ],
    );
    write(&repo, "build/inner/out.txt", b"needle\n");
    write(&repo, "debug.log", b"needle\n");
    // Names the file tree leaves out are left out whatever the option.
    write(&repo, ".DS_Store", b"needle\n");
    assert_eq!(places(&run(&repo, &query("needle")).0), ["kept.txt:1"]);
    let included = Query {
        include_ignored: true,
        ..query("needle")
    };
    assert_eq!(
        places(&run(&repo, &included).0),
        ["build/inner/out.txt:1", "debug.log:1", "kept.txt:1"]
    );
}

#[test]
fn binary_files_are_skipped_even_after_a_match() {
    let dir = ScratchDir::new("content-search-binary");
    write(&dir, "text.txt", b"needle\n");
    write(&dir, "early.bin", b"needle\0\n");
    // A NUL found only after matching lines: what the file matched before is dropped.
    let mut late = b"needle\n".to_vec();
    late.extend(std::iter::repeat_n(b"filler line\n", 20_000).flatten());
    late.extend(b"\0needle\n");
    write(&dir, "late.bin", &late);
    let (found, summary) = run(&dir, &query("needle"));
    assert_eq!(places(&found), ["text.txt:1"]);
    assert!(summary.complete);
}

#[test]
fn links_are_never_followed() {
    let dir = ScratchDir::new("content-search-links");
    let outside = ScratchDir::new("content-search-links-outside");
    write(&outside, "secret.txt", b"needle\n");
    write(&dir, "real/inside.txt", b"needle\n");
    symlink(outside.join("secret.txt"), dir.join("file-link.txt")).unwrap();
    symlink(&*outside, dir.join("dir-link")).unwrap();
    symlink(dir.join("real"), dir.join("inner-link")).unwrap();
    let (found, summary) = run(&dir, &query("needle"));
    assert_eq!(places(&found), ["real/inside.txt:1"]);
    assert!(summary.complete);
}

#[test]
fn a_file_over_the_size_bound_is_skipped_without_marking_the_search_incomplete() {
    let dir = ScratchDir::new("content-search-large");
    write(&dir, "small.txt", b"needle\n");
    write(&dir, "large.txt", b"needle and much more\n");
    let bounds = Bounds {
        max_file_bytes: 10,
        ..Bounds::standard()
    };
    let (found, summary, _) = run_in(&dir, None, &query("needle"), &bounds).unwrap();
    assert_eq!(places(&found), ["small.txt:1"]);
    assert!(summary.complete);
}

/// How a search of a project holding a few matching files ends under `bounds`.
fn ended_under(bounds: Bounds, send: &mut super::Send) -> Summary {
    let dir = ScratchDir::new("content-search-bounds");
    for name in ["a.txt", "b.txt", "c.txt"] {
        write(&dir, name, b"needle needle\nneedle\n");
    }
    let scope = crate::hostfs::canonical(&dir).unwrap();
    let compiled = Compiled::new(&query("needle")).unwrap();
    search(
        &scope,
        &scope,
        &compiled,
        &bounds,
        &AtomicBool::new(false),
        send,
    )
    .unwrap()
}

#[test]
fn each_bound_ends_the_search_marked_incomplete() {
    let take = &mut |_: Vec<ContentMatch>, _: &Sending| Ok(Taken::Sent);
    let timed = ended_under(
        Bounds {
            timeout: Duration::ZERO,
            ..Bounds::standard()
        },
        take,
    );
    assert_eq!(
        (timed.complete, timed.stopped_by),
        (false, Some(ContentSearchStop::Time))
    );
    let counted = ended_under(
        Bounds {
            max_matches: 4,
            ..Bounds::standard()
        },
        take,
    );
    assert_eq!(
        (counted.complete, counted.stopped_by),
        (false, Some(ContentSearchStop::Matches))
    );
    assert!(counted.matches <= 4, "{counted:?}");
    let read = ended_under(
        Bounds {
            max_bytes_read: 5,
            ..Bounds::standard()
        },
        take,
    );
    assert_eq!(
        (read.complete, read.stopped_by),
        (false, Some(ContentSearchStop::BytesRead))
    );
    let refused = ended_under(Bounds::standard(), &mut |_, _: &Sending| Ok(Taken::Refused));
    assert_eq!(
        refused,
        Summary {
            matches: 0,
            files: 0,
            complete: false,
            stopped_by: Some(ContentSearchStop::ResultBytes)
        }
    );
    // A batch the search ran out of time to send ends it at its time.
    let late = ended_under(Bounds::standard(), &mut |_, _: &Sending| {
        Ok(Taken::OutOfTime)
    });
    assert_eq!(
        (late.complete, late.stopped_by),
        (false, Some(ContentSearchStop::Time))
    );
    // Exactly as many matches as the bound is a complete search.
    let exact = ended_under(
        Bounds {
            max_matches: 9,
            ..Bounds::standard()
        },
        take,
    );
    assert_eq!((exact.matches, exact.complete), (9, true));
}

#[test]
fn a_cancelled_search_ends_with_the_cancel() {
    let dir = options_project();
    let scope = crate::hostfs::canonical(&dir).unwrap();
    let compiled = Compiled::new(&query("needle")).unwrap();
    let cancel = AtomicBool::new(true);
    let outcome = search(
        &scope,
        &scope,
        &compiled,
        &Bounds::standard(),
        &cancel,
        &mut |_, _: &Sending| Ok(Taken::Sent),
    );
    assert!(outcome.unwrap_err().downcast_ref::<Cancelled>().is_some());
}

/// A connection's lane for `search_content`, with its control and reply queues.
fn connection() -> (Arc<Lane>, mpsc::Receiver<Event>, mpsc::Receiver<Frame>) {
    let (control_tx, control_rx) = mpsc::channel(64);
    let (content_tx, content_rx) = mpsc::channel(64);
    let lane = Lane::new(
        Arc::new(Gate::with_budget()),
        budget::MAX_PENDING_REQUESTS,
        budget::MAX_RETAINED_PER_CONNECTION,
        control_tx,
        content_tx,
    );
    (Arc::new(lane), control_rx, content_rx)
}

async fn submit(fixture: &Fixture, lane: &Arc<Lane>, request: serde_json::Value) {
    submit_within(fixture, lane, request, Bounds::standard()).await;
}

/// [`submit`], the search held to `bounds`.
async fn submit_within(
    fixture: &Fixture,
    lane: &Arc<Lane>,
    request: serde_json::Value,
    bounds: Bounds,
) {
    let request: BrowseRequest = serde_json::from_value(request).unwrap();
    let state = fixture.state.clone();
    let reservation = budget::reservation_for(&request.body);
    lane.submit_stream(
        request.id,
        request.slot,
        reservation,
        move |cancel, stream| {
            let BrowseBody::SearchProjectContent(search) = request.body else {
                panic!("not a content search");
            };
            crate::browse::search_content_within(&state, search, &git_env, cancel, stream, &bounds)
        },
    )
    .await;
}

async fn next<T>(rx: &mut mpsc::Receiver<T>) -> T {
    tokio::time::timeout(PATIENCE, rx.recv())
        .await
        .expect("an answer in time")
        .expect("the channel is open")
}

#[tokio::test]
async fn a_search_request_is_acknowledged_then_streams_its_matches_and_ends() {
    let fixture = Fixture::new();
    let dir = options_project();
    let project = fixture.project("content-project", &dir);
    let (lane, mut control, mut content) = connection();
    submit(
        &fixture,
        &lane,
        serde_json::json!({
            "id": "find-1", "slot": "find", "type": "search_project_content",
            "project": project, "search": "s-1", "query": "NEEDLE", "file_mask": "*.ts"
        }),
    )
    .await;
    let ack = serde_json::to_value(next(&mut control).await).unwrap();
    assert_eq!(ack, serde_json::json!({"type": "ack", "id": "find-1"}));
    let mut lines = Vec::new();
    let ended = loop {
        let frame: serde_json::Value =
            serde_json::from_str(&next(&mut content).await.text).unwrap();
        assert_eq!(frame["search"], "s-1");
        assert_eq!(frame["project"], "content-project");
        match frame["type"].as_str() {
            Some("project_content_matches") => {
                lines.extend(frame["matches"].as_array().unwrap().iter().cloned())
            }
            Some("project_content_search_ended") => break frame,
            other => panic!("not a search event: {other:?}"),
        }
    };
    lines.sort_by_key(|line| line["path"].as_str().unwrap().to_string());
    assert_eq!(
        lines[0],
        serde_json::json!({
            "path": "b.ts", "line": 1, "text": "a needle", "text_start": 0,
            "ranges": [{"start": 2, "end": 8}], "clipped_end": false
        })
    );
    assert_eq!(lines.len(), 2);
    assert_eq!(ended["matches"], 2);
    assert_eq!(ended["files"], 2);
    assert_eq!(ended["complete"], true);
    assert!(ended.get("stopped_by").is_none());
    // The end is the last thing the search sends, so anything more would be queued already.
    assert!(content.try_recv().is_err(), "an event followed the end");
    assert!(
        control.try_recv().is_err(),
        "a second answer followed the end"
    );
}

#[tokio::test]
async fn a_search_that_cannot_start_is_refused_without_an_ack() {
    let fixture = Fixture::new();
    let dir = options_project();
    let project = fixture.project("content-refused", &dir);
    let (lane, mut control, mut content) = connection();
    let long_name = "s".repeat(budget::MAX_SEARCH_NAME_BYTES + 1);
    for (name, request, expected, message) in [
        (
            "pattern",
            serde_json::json!({"project": project, "query": "(", "regex": true}),
            error_code::INVALID_SEARCH_PATTERN,
            "not a valid regular expression",
        ),
        (
            "path",
            serde_json::json!({"project": project, "query": "x", "directory": "../outside"}),
            error_code::INVALID_PATH,
            "../outside is not a path inside a project",
        ),
        (
            "unknown",
            serde_json::json!({"project": "no-such-project", "query": "x"}),
            error_code::UNKNOWN_PROJECT,
            "no-such-project",
        ),
        (
            "mask",
            serde_json::json!({"project": project, "query": "x", "file_mask": "*.ts, a["}),
            error_code::INVALID_FILE_MASK,
            "a[ is not a valid file mask",
        ),
        (
            "name",
            serde_json::json!({"project": project, "query": "x", "search": long_name}),
            error_code::LIMIT_EXCEEDED,
            "the search's name is longer than",
        ),
    ] {
        let mut request = request;
        request["id"] = name.into();
        request["type"] = "search_project_content".into();
        if request.get("search").is_none() {
            request["search"] = "s".into();
        }
        submit(&fixture, &lane, request).await;
        match next(&mut control).await {
            Event::Error {
                id,
                code,
                message: said,
                params,
            } => {
                assert_eq!((id.as_deref(), code), (Some(name), expected));
                assert!(said.contains(message), "{name}: {said}");
                if name == "mask" {
                    assert_eq!(params["glob"], "a[");
                    assert!(params.contains_key("detail"));
                }
                if name == "name" {
                    assert_eq!(params["limit"], "search_name");
                }
            }
            other => panic!("not refused: {other:?}"),
        }
    }
    assert!(content.try_recv().is_err(), "a refused search sent events");
}

#[test]
fn a_line_is_reported_whole_with_every_match_in_utf16_units_and_without_its_line_ending() {
    let dir = ScratchDir::new("content-search-shape");
    write(
        &dir,
        "x.txt",
        "plain\r\nhéllo 😀 needle needle\r\nlast needle".as_bytes(),
    );
    let (found, summary) = run(&dir, &query("needle"));
    assert_eq!(
        found
            .iter()
            .map(|line| (line.line, line.text.as_str()))
            .collect::<Vec<_>>(),
        [(2, "héllo 😀 needle needle"), (3, "last needle")]
    );
    // h é l l o and a space are six units, the emoji two more and a space one: the first match
    // starts at 9, and each match is its own range of the one line.
    assert_eq!(
        found[0].ranges,
        [
            MatchRange { start: 9, end: 15 },
            MatchRange { start: 16, end: 22 }
        ]
    );
    assert_eq!(found[1].ranges, [MatchRange { start: 5, end: 11 }]);
    assert!(found
        .iter()
        .all(|line| line.text_start == 0 && !line.clipped_end));
    // The two lines hold three matches, which is what the summary counts.
    assert_eq!((summary.matches, summary.files), (3, 1));
}

#[test]
fn a_long_line_is_clipped_to_about_250_characters_around_its_first_match() {
    let dir = ScratchDir::new("content-search-long");
    // Longer than the window decoded around the first match, 4 * (250 + 40) bytes.
    let line = format!("{}needle{}needle", "a".repeat(1300), "b".repeat(400));
    write(&dir, "long.txt", line.as_bytes());
    let (found, _) = run(&dir, &query("needle"));
    let long = &found[0];
    assert!(long.text.chars().count() <= 260, "{}", long.text.len());
    assert!(long.text.contains("needle"));
    // 40 characters come before the first match.
    assert_eq!(long.text_start, 1260);
    assert!(long.clipped_end);
    // Ranges are offsets into the whole line, so the first match is where it is in the line, and
    // the clipped text begins at `text_start` of it.
    assert_eq!(
        long.ranges[0],
        MatchRange {
            start: 1300,
            end: 1306
        }
    );
    assert_eq!(
        long.ranges[1],
        MatchRange {
            start: 1706,
            end: 1712
        }
    );
    let at = long.text.find("needle").unwrap() as u32;
    assert_eq!(long.text_start + at, 1300);
}

#[test]
fn the_search_root_is_the_scope_without_a_directory() {
    let dir = options_project();
    let scope = crate::hostfs::canonical(&dir).unwrap();
    assert_eq!(root(&scope, None).unwrap(), scope);
    let empty = RelPath::parse("").unwrap();
    assert_eq!(root(&scope, Some(&empty)).unwrap(), scope);
    let sub: PathBuf = root(&scope, RelPath::parse("sub").as_ref()).unwrap();
    assert_eq!(sub, scope.join("sub"));
}

#[test]
fn a_regular_expression_ending_in_a_line_end_matches_before_a_crlf() {
    let dir = ScratchDir::new("content-search-crlf");
    write(
        &dir,
        "windows.txt",
        b"first needle\r\nneedle in the middle\r\nlast needle",
    );
    let regex = Query {
        regex: true,
        ..query("needle$")
    };
    let (found, _) = run(&dir, &regex);
    assert_eq!(places(&found), ["windows.txt:1", "windows.txt:3"]);
    // The range stops before the line ending, which the text leaves out.
    assert_eq!(found[0].text, "first needle");
    assert_eq!(found[0].ranges, [MatchRange { start: 6, end: 12 }]);
}

#[test]
fn a_directory_reached_through_a_link_is_refused() {
    let dir = options_project();
    let outside = ScratchDir::new("content-search-dir-link-outside");
    write(&outside, "secret.txt", b"needle\n");
    symlink(&*outside, dir.join("out-link")).unwrap();
    symlink(dir.join("sub"), dir.join("in-link")).unwrap();
    let refused = |directory: &str| {
        run_in(&dir, Some(directory), &query("needle"), &Bounds::standard()).expect_err("refused")
    };
    let inside = refused("in-link");
    let coded = inside.downcast_ref::<CodedError>().unwrap();
    assert_eq!(coded.code, error_code::UNSUPPORTED_FILE_TYPE);
    assert_eq!(coded.params["file_type"], "symlink");
    assert_eq!(
        code::<()>(Err(refused("out-link"))),
        error_code::OUTSIDE_SCOPE
    );
}

#[test]
fn a_directory_git_ignores_is_searched_when_the_search_is_limited_to_it() {
    let repo = repo_with(
        "content-search-ignored-directory",
        &[
            (b".gitignore", b"node_modules/\n"),
            (b"kept.txt", b"needle\n"),
        ],
    );
    write(&repo, "node_modules/pkg/index.js", b"needle\n");
    let (found, summary, _) = run_in(
        &repo,
        Some("node_modules/pkg"),
        &query("needle"),
        &Bounds::standard(),
    )
    .unwrap();
    assert_eq!(places(&found), ["node_modules/pkg/index.js:1"]);
    assert!(summary.complete);
}

#[test]
fn an_ignore_file_that_does_not_parse_leaves_the_search_complete() {
    // A glob Git takes but `ignore` does not parse, in the `.gitignore` above the walk's root.
    let repo = repo_with(
        "content-search-bad-ignore",
        &[(b".gitignore", b"a{b\n"), (b"sub/kept.txt", b"needle\n")],
    );
    let (found, summary, _) =
        run_in(&repo, Some("sub"), &query("needle"), &Bounds::standard()).unwrap();
    assert_eq!(places(&found), ["sub/kept.txt:1"]);
    assert!(summary.complete, "{summary:?}");
}

#[cfg(unix)]
#[test]
fn an_unreadable_file_leaves_the_search_incomplete_with_no_bound_named() {
    use std::os::unix::fs::PermissionsExt;
    // Root reads whatever the mode says.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let dir = ScratchDir::new("content-search-unreadable");
    write(&dir, "readable.txt", b"needle\n");
    write(&dir, "locked.txt", b"needle\n");
    std::fs::set_permissions(
        dir.join("locked.txt"),
        std::fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    let (found, summary) = run(&dir, &query("needle"));
    std::fs::set_permissions(
        dir.join("locked.txt"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert_eq!(places(&found), ["readable.txt:1"]);
    assert_eq!((summary.complete, summary.stopped_by), (false, None));
}

/// `ignore` reads the ignore files alone, not the index, so unlike `git` it leaves out a tracked
/// file that an ignore pattern matches (see "Searching file contents" in `PROTOCOL.md`).
#[test]
fn a_tracked_file_matching_an_ignore_pattern_is_left_out() {
    let repo = repo_with(
        "content-search-tracked-ignored",
        &[(b".gitignore", b"*.log\n"), (b"plain.txt", b"needle\n")],
    );
    write(&repo, "tracked.log", b"needle\n");
    git(&repo, &["add", "-f", "tracked.log"]);
    git(&repo, &["commit", "-q", "-m", "track an ignored file"]);
    assert_eq!(places(&run(&repo, &query("needle")).0), ["plain.txt:1"]);
    let included = Query {
        include_ignored: true,
        ..query("needle")
    };
    assert_eq!(
        places(&run(&repo, &included).0),
        ["plain.txt:1", "tracked.log:1"]
    );
}

#[test]
fn a_binary_file_s_matches_do_not_count_toward_the_bound() {
    let dir = ScratchDir::new("content-search-binary-bound");
    // The NUL comes after more than the searcher's first buffer (64 KiB), so the lines before it
    // are collected, and counted, before the file turns out binary; they would pass the bound.
    let mut binary = b"needle\nneedle\nneedle\nneedle\n".to_vec();
    binary.extend(std::iter::repeat_n(b"filler line\n", 8_000).flatten());
    binary.extend(b"\0needle\n");
    write(&dir, "binary.bin", &binary);
    write(&dir, "text.txt", b"needle\n");
    let bounds = Bounds {
        max_matches: 2,
        ..Bounds::standard()
    };
    let (found, summary, _) = run_in(&dir, None, &query("needle"), &bounds).unwrap();
    assert_eq!(places(&found), ["text.txt:1"]);
    assert!(summary.complete, "{summary:?}");
}

#[test]
fn a_file_with_a_line_past_the_line_buffer_leaves_the_search_incomplete() {
    let dir = ScratchDir::new("content-search-long-line");
    let mut long = vec![b'a'; 8 * 1024];
    long.extend(b"needle\n");
    write(&dir, "long.txt", &long);
    write(&dir, "short.txt", b"needle\n");
    let bounds = Bounds {
        max_line_bytes: 1024,
        ..Bounds::standard()
    };
    let (found, summary, _) = run_in(&dir, None, &query("needle"), &bounds).unwrap();
    assert_eq!(places(&found), ["short.txt:1"]);
    assert_eq!((summary.complete, summary.stopped_by), (false, None));
}

#[cfg(unix)]
#[test]
fn an_ignore_file_that_cannot_be_read_leaves_the_search_complete() {
    use std::os::unix::fs::PermissionsExt;
    // Root reads whatever the mode says.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let repo = repo_with(
        "content-search-unreadable-ignore",
        &[
            (b".gitignore", b"*.log\n"),
            (b"sub/.gitignore", b"*.tmp\n"),
            (b"sub/kept.txt", b"needle\n"),
        ],
    );
    let locked = [repo.join(".gitignore"), repo.join("sub/.gitignore")];
    for file in &locked {
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    // One above the walk's root, one within it.
    let limited = run_in(&repo, Some("sub"), &query("needle"), &Bounds::standard());
    let whole = run_in(&repo, None, &query("needle"), &Bounds::standard());
    for file in &locked {
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    for (found, summary, _) in [limited.unwrap(), whole.unwrap()] {
        assert_eq!(places(&found), ["sub/kept.txt:1"]);
        assert!(summary.complete, "{summary:?}");
    }
}

/// The roots whose searches fail as soon as they start, after the `ack`: a test's own scratch
/// directory, so that tests running at once do not see each other's.
static FAILING_ROOTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Whether a test asked the search of `root` to fail; see [`FAILING_ROOTS`].
pub(super) fn fails_after_the_ack(root: &Path) -> bool {
    FAILING_ROOTS
        .lock()
        .unwrap()
        .iter()
        .any(|failing| failing == root)
}

#[tokio::test]
async fn a_search_failing_after_its_ack_ends_incomplete_with_nothing_counted() {
    let fixture = Fixture::new();
    let dir = options_project();
    let project = fixture.project("content-failing", &dir);
    FAILING_ROOTS
        .lock()
        .unwrap()
        .push(crate::hostfs::canonical(&dir).unwrap());
    let (lane, mut control, mut content) = connection();
    submit(
        &fixture,
        &lane,
        serde_json::json!({
            "id": "failing", "type": "search_project_content",
            "project": project, "search": "s-failing", "query": "needle"
        }),
    )
    .await;
    assert!(matches!(next(&mut control).await, Event::Ack { .. }));
    let ended: serde_json::Value = serde_json::from_str(&next(&mut content).await.text).unwrap();
    assert_eq!(
        ended,
        serde_json::json!({
            "type": "project_content_search_ended", "search": "s-failing",
            "project": "content-failing", "matches": 0, "files": 0, "complete": false
        })
    );
    assert!(control.try_recv().is_err(), "an error followed the ack");
}

/// Set once the batch of `the_stream_s_pause_stops_the_walk_threads_reading` is being sent.
static PAUSE_BATCH_SENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Holds the read of `later.txt` back until the batch is being sent, so that its reader is still
/// to read when the readers are paused.
fn hold_later_back(path: &str) {
    if path == "later.txt" {
        crate::session::wait::wait_for(PATIENCE, || {
            PAUSE_BATCH_SENT.load(std::sync::atomic::Ordering::Relaxed)
        });
    }
}

/// The readers of `the_stream_s_pause_stops_the_walk_threads_reading`'s search waiting now.
static WALK_PAUSED_READERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[test]
fn the_stream_s_pause_stops_the_walk_threads_reading() {
    use std::sync::atomic::Ordering;
    let dir = ScratchDir::new("content-search-pause");
    write(&dir, "match.txt", b"needle\n");
    write(&dir, "later.txt", b"nothing here\n");
    let scope = crate::hostfs::canonical(&dir).unwrap();
    let bounds = Bounds {
        batch_lines: 1,
        // A thread for the read held back and one for the batch.
        walk_threads: 2,
        before_reading: Some(hold_later_back),
        waiters_seen: Some(&WALK_PAUSED_READERS),
        ..Bounds::standard()
    };
    let compiled = Compiled::new(&query("needle")).unwrap();
    let mut paused_reader = None;
    let summary = search(
        &scope,
        &scope,
        &compiled,
        &bounds,
        &AtomicBool::new(false),
        &mut |_, sending: &Sending| {
            sending.pause_readers(true);
            PAUSE_BATCH_SENT.store(true, Ordering::Relaxed);
            paused_reader = Some(crate::session::wait::wait_for(PATIENCE, || {
                WALK_PAUSED_READERS.load(Ordering::Relaxed) == 1
            }));
            sending.pause_readers(false);
            Ok(Taken::Sent)
        },
    )
    .unwrap();
    assert_eq!(paused_reader, Some(true), "the reader of later.txt read on");
    assert!(summary.complete, "{summary:?}");
}

#[cfg(unix)]
#[test]
fn a_folder_named_like_an_ignore_file_that_cannot_be_listed_leaves_the_search_incomplete() {
    use std::os::unix::fs::PermissionsExt;
    // Root lists whatever the mode says.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let repo = repo_with(
        "content-search-ignore-named-folder",
        &[(b"kept.txt", b"needle\n")],
    );
    write(&repo, "sub/.gitignore/inside.txt", b"needle\n");
    let folder = repo.join("sub/.gitignore");
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (found, summary) = run(&repo, &query("needle"));
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(places(&found), ["kept.txt:1"]);
    assert_eq!((summary.complete, summary.stopped_by), (false, None));
}

#[cfg(unix)]
#[test]
fn an_unreadable_project_file_named_like_a_repository_s_exclude_file_leaves_the_search_incomplete()
{
    use std::os::unix::fs::PermissionsExt;
    // Root reads whatever the mode says.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let dir = ScratchDir::new("content-search-info-exclude");
    write(&dir, "kept.txt", b"needle\n");
    write(&dir, "docs/info/exclude", b"needle\n");
    let locked = dir.join("docs/info/exclude");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (found, summary) = run(&dir, &query("needle"));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(places(&found), ["kept.txt:1"]);
    assert_eq!((summary.complete, summary.stopped_by), (false, None));
}

/// Lets the read of `later.txt` of `a_stream_waiting_for_room_pauses_the_search_s_readers` go.
static LATER_MAY_READ: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The readers of that test's search waiting now.
static PAUSED_READERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn hold_later_until_told(path: &str) {
    if path == "later.txt" {
        crate::session::wait::wait_for(PATIENCE, || {
            LATER_MAY_READ.load(std::sync::atomic::Ordering::Relaxed)
        });
    }
}

/// The request's own half of the pause: a batch waiting for room on the reply queue, given its
/// read slot back, pauses the search's readers.
#[tokio::test]
async fn a_stream_waiting_for_room_pauses_the_search_s_readers() {
    use std::sync::atomic::Ordering;
    let fixture = Fixture::new();
    let dir = ScratchDir::new("content-search-stream-pause");
    write(&dir, "a.txt", b"needle\n");
    write(&dir, "b.txt", b"needle\n");
    write(&dir, "later.txt", b"nothing here\n");
    let project = fixture.project("content-stream-pause", &dir);
    // One read slot, and room for one frame on the reply queue, which is read only at the end.
    let gate = Arc::new(Gate::new(1, 1 << 26));
    let (control_tx, mut control) = mpsc::channel(8);
    let (content_tx, mut content) = mpsc::channel(1);
    let lane = Arc::new(Lane::new(gate.clone(), 8, 1 << 26, control_tx, content_tx));
    submit_within(
        &fixture,
        &lane,
        serde_json::json!({
            "id": "paused", "type": "search_project_content",
            "project": project, "search": "s", "query": "needle"
        }),
        Bounds {
            batch_lines: 1,
            walk_threads: 2,
            before_reading: Some(hold_later_until_told),
            waiters_seen: Some(&PAUSED_READERS),
            ..Bounds::standard()
        },
    )
    .await;
    assert!(matches!(next(&mut control).await, Event::Ack { .. }));
    // The second batch waits behind the first: the search has given its read slot back.
    let watched = gate.clone();
    let waiting =
        crate::test_support::within(PATIENCE, move || watched.reads_available() == 1).await;
    assert!(waiting, "no batch waited for room");
    LATER_MAY_READ.store(true, Ordering::Relaxed);
    let paused =
        crate::test_support::within(PATIENCE, || PAUSED_READERS.load(Ordering::Relaxed) == 1).await;
    assert!(
        paused,
        "the reader of later.txt read while the batch waited"
    );
    let ended = loop {
        let frame: serde_json::Value =
            serde_json::from_str(&next(&mut content).await.text).unwrap();
        if frame["type"] == "project_content_search_ended" {
            break frame;
        }
    };
    assert_eq!(
        (ended["matches"].as_u64(), ended["complete"].as_bool()),
        (Some(2), Some(true))
    );
}
