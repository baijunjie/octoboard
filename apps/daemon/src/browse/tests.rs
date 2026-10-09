//! Browse requests served end to end against real directories and repositories: what each project
//! shape resolves to, what a read returns or refuses, and that every path reaches exactly the bytes
//! it names.

use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use base64::Engine as _;

use super::git::tests::{git, git_bytes, repo_with, test_env};
use super::git::GitEnv;
use super::*;
use crate::protocol::{
    Agent, BrowseEntry, Console, ContentKind, EntryKind, LinkTarget, Project, ProjectSource,
};
use crate::state::AppState;
use crate::test_support::{app_state, ScratchDir};

struct Fixture {
    state: Arc<AppState>,
    _dir: ScratchDir,
}

impl Fixture {
    fn new() -> Self {
        let (state, dir) = app_state("browse");
        let workdir = dir.join("workdir");
        std::fs::create_dir(&workdir).unwrap();
        state
            .store
            .insert_console(&Console {
                id: "console".to_string(),
                name: "Console".to_string(),
                workdir: workdir.to_string_lossy().into_owned(),
                console_session_agent: Agent::Claude,
                default_agent: Agent::Claude,
                claude_account_id: None,
                codex_account_id: None,
                grok_account_id: None,
                icon: None,
                created_at: 0,
            })
            .unwrap();
        Self { state, _dir: dir }
    }

    /// Registers `path` as project `id`, stored as given.
    fn project(&self, id: &str, path: &Path) -> String {
        self.state
            .store
            .insert_project(&Project {
                id: id.to_string(),
                console_id: "console".to_string(),
                host_id: crate::store::LOCAL_HOST_ID.to_string(),
                name: id.to_string(),
                path: path.to_string_lossy().into_owned(),
                default_agent: None,
                source: ProjectSource::Local,
                remote_url: None,
                claude_trust_consent: false,
                pinned: false,
                tags: Vec::new(),
            })
            .unwrap();
        id.to_string()
    }

    fn serve(&self, body: BrowseBody) -> anyhow::Result<Event> {
        serve(&self.state, None, body, &git_env, &AtomicBool::new(false))
    }

    fn source_result(&self, project: &str) -> anyhow::Result<Event> {
        self.serve(BrowseBody::GetProjectSource {
            project: project.to_string(),
        })
    }

    fn source(&self, project: &str) -> ProjectSourceInfo {
        match self.serve(BrowseBody::GetProjectSource {
            project: project.to_string(),
        }) {
            Ok(Event::ProjectSource { source, .. }) => source,
            other => panic!("not a source: {other:?}"),
        }
    }

    fn read(
        &self,
        project: &str,
        worktree: Option<&str>,
        path: &str,
        from: ReadFrom,
    ) -> anyhow::Result<(ContentSource, FileContent)> {
        match self.serve(BrowseBody::ReadProjectFile {
            project: project.to_string(),
            worktree: worktree.map(str::to_string),
            path: path.to_string(),
            from,
        })? {
            Event::ProjectFile {
                project: echoed_project,
                worktree: echoed_worktree,
                path: echoed_path,
                source,
                file,
                ..
            } => {
                let echoed = (
                    echoed_project.as_str(),
                    echoed_worktree.as_deref(),
                    echoed_path,
                );
                assert_eq!(
                    echoed,
                    (project, worktree, path.to_string()),
                    "the reply's echo"
                );
                Ok((source, file))
            }
            other => panic!("not a file: {other:?}"),
        }
    }

    fn list(
        &self,
        project: &str,
        worktree: Option<&str>,
        path: &str,
    ) -> anyhow::Result<(Vec<BrowseEntry>, bool)> {
        match self.serve(BrowseBody::ListProjectDir {
            project: project.to_string(),
            worktree: worktree.map(str::to_string),
            path: path.to_string(),
        })? {
            Event::ProjectDir {
                project: echoed_project,
                worktree: echoed_worktree,
                path: echoed_path,
                entries,
                complete,
                ..
            } => {
                let echoed = (
                    echoed_project.as_str(),
                    echoed_worktree.as_deref(),
                    echoed_path,
                );
                assert_eq!(
                    echoed,
                    (project, worktree, path.to_string()),
                    "the reply's echo"
                );
                Ok((entries, complete))
            }
            other => panic!("not a listing: {other:?}"),
        }
    }
}

fn git_env() -> anyhow::Result<GitEnv> {
    Ok(test_env(&[]))
}

fn bytes_of(file: &FileContent) -> Vec<u8> {
    match file.kind {
        ContentKind::Text => file.text.clone().unwrap().into_bytes(),
        ContentKind::Binary => base64::engine::general_purpose::STANDARD
            .decode(file.data.as_ref().unwrap())
            .unwrap(),
    }
}

/// The `code` of the coded error `result` failed with.
fn code<T: std::fmt::Debug>(result: anyhow::Result<T>) -> &'static str {
    let err = result.expect_err("a refusal");
    err.downcast_ref::<CodedError>()
        .unwrap_or_else(|| panic!("not coded: {err:#}"))
        .code
}

fn canonical(path: &Path) -> String {
    wire_path::encode(std::fs::canonicalize(path).unwrap().as_os_str().as_bytes())
}

#[test]
fn each_project_shape_resolves_to_its_own_root_repository_worktree_and_scope() {
    let fixture = Fixture::new();
    let plain = ScratchDir::new("browse-plain");
    let repo = repo_with("browse-repo", &[(b"sub/inner/a.txt", b"a\n")]);
    let linked = ScratchDir::new("browse-linked");
    let linked_root = linked.join("wt");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "side",
            linked_root.to_str().unwrap(),
        ],
    );
    let links = ScratchDir::new("browse-links");
    let through_link = links.join("link");
    std::os::unix::fs::symlink(repo.join("sub"), &through_link).unwrap();

    let plain_source = fixture.source(&fixture.project("plain", &plain));
    assert!(plain_source.git.is_none() && plain_source.git_error.is_none());
    assert_eq!(plain_source.resolved_root, canonical(&plain));

    let root = fixture.source(&fixture.project("root", &repo));
    let root_git = root.git.expect("a repository");
    assert_eq!(root_git.scope, "");
    assert_eq!(root_git.common_dir, canonical(&repo.join(".git")));
    assert_eq!(root_git.worktrees.len(), 2);
    let main = root_git.worktrees.iter().find(|w| w.main).unwrap();
    assert_eq!(main.id, root_git.worktree);
    assert_eq!(main.branch.as_deref(), Some("main"));

    let sub = fixture.source(&fixture.project("sub", &repo.join("sub")));
    let sub_git = sub.git.expect("a repository");
    assert_eq!(sub_git.scope, "sub");
    assert_eq!(sub_git.repository, root_git.repository);

    let in_linked = fixture.source(&fixture.project("linked", &linked_root.join("sub")));
    let linked_git = in_linked.git.expect("a repository");
    assert_eq!(linked_git.repository, root_git.repository);
    assert_ne!(linked_git.worktree, root_git.worktree);
    assert_eq!(linked_git.scope, "sub");
    let linked_entry = linked_git
        .worktrees
        .iter()
        .find(|w| w.id == linked_git.worktree)
        .unwrap();
    assert!(!linked_entry.main);
    assert_eq!(linked_entry.branch.as_deref(), Some("side"));
    assert_eq!(linked_entry.root, canonical(&linked_root));

    let linked_through = fixture.source(&fixture.project("through", &through_link));
    assert_eq!(linked_through.root, through_link.to_string_lossy());
    assert_eq!(linked_through.resolved_root, canonical(&repo.join("sub")));
    assert_eq!(linked_through.git.expect("a repository").scope, "sub");
}

#[test]
fn a_subdirectory_maps_into_another_worktree_and_never_broadens() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "browse-map",
        &[(b"sub/a.txt", b"main copy\n"), (b"top.txt", b"top\n")],
    );
    let linked = ScratchDir::new("browse-map-linked");
    let other = linked.join("other");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "other",
            other.to_str().unwrap(),
        ],
    );
    std::fs::write(other.join("sub/a.txt"), b"other copy\n").unwrap();
    let bare = linked.join("bare");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "--orphan",
            "-b",
            "empty",
            bare.to_str().unwrap(),
        ],
    );

    // A worktree in which the scope is a link to the worktree's own root.
    let linked_scope = linked.join("linked-scope");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            linked_scope.to_str().unwrap(),
        ],
    );
    std::fs::remove_dir_all(linked_scope.join("sub")).unwrap();
    std::os::unix::fs::symlink(".", linked_scope.join("sub")).unwrap();

    let project = fixture.project("sub", &repo.join("sub"));
    let source = fixture.source(&project);
    let git_info = source.git.unwrap();
    let linked_entry = git_info
        .worktrees
        .iter()
        .find(|w| w.root == canonical(&linked_scope))
        .unwrap();
    assert!(!linked_entry.scope_present);
    assert_eq!(
        code(fixture.read(&project, Some(&linked_entry.id), "top.txt", ReadFrom::Live)),
        error_code::SOURCE_UNAVAILABLE,
        "a link in another worktree must not widen the scope to its root"
    );
    let other_id = &git_info
        .worktrees
        .iter()
        .find(|w| w.root == canonical(&other))
        .unwrap()
        .id;
    let bare_entry = git_info
        .worktrees
        .iter()
        .find(|w| w.root == canonical(&bare))
        .unwrap();
    assert!(!bare_entry.scope_present);

    let (_, file) = fixture
        .read(&project, Some(other_id), "a.txt", ReadFrom::Live)
        .unwrap();
    assert_eq!(bytes_of(&file), b"other copy\n");
    let (entries, _) = fixture.list(&project, Some(other_id), "").unwrap();
    assert_eq!(
        entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        ["a.txt"]
    );
    assert_eq!(
        code(fixture.read(&project, Some(other_id), "top.txt", ReadFrom::Live)),
        error_code::FILE_NOT_FOUND
    );
    assert_eq!(
        code(fixture.list(&project, Some(&bare_entry.id), "")),
        error_code::SOURCE_UNAVAILABLE
    );
    assert_eq!(
        code(fixture.read(&project, Some("1:2"), "a.txt", ReadFrom::Live)),
        error_code::WORKTREE_UNAVAILABLE
    );
}

#[test]
fn a_removed_worktree_or_its_path_reused_by_another_repository_invalidates_its_id() {
    let fixture = Fixture::new();
    let repo = repo_with("browse-gone", &[(b"a.txt", b"ours\n")]);
    let linked = ScratchDir::new("browse-gone-linked");
    let path = linked.join("wt");
    git(
        &repo,
        &["worktree", "add", "-q", "-b", "wt", path.to_str().unwrap()],
    );
    let project = fixture.project("p", &repo);
    let id_of = |fixture: &Fixture| {
        let source = fixture.source(&project).git.unwrap();
        source.worktrees.into_iter().find(|w| !w.main).map(|w| w.id)
    };
    let first = id_of(&fixture).unwrap();
    assert!(fixture
        .read(&project, Some(&first), "a.txt", ReadFrom::Live)
        .is_ok());

    git(&repo, &["worktree", "remove", path.to_str().unwrap()]);
    assert_eq!(
        code(fixture.read(&project, Some(&first), "a.txt", ReadFrom::Live)),
        error_code::WORKTREE_UNAVAILABLE
    );

    git(
        &repo,
        &["worktree", "add", "-q", path.to_str().unwrap(), "wt"],
    );
    let second = id_of(&fixture).unwrap();
    assert_ne!(second, first, "a worktree added again is a new source");
    // Another repository takes the path over, behind the first repository's back.
    std::fs::remove_dir_all(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    git(&path, &["init", "-q"]);
    std::fs::write(path.join("a.txt"), b"theirs\n").unwrap();
    assert_eq!(
        code(fixture.read(&project, Some(&second), "a.txt", ReadFrom::Live)),
        error_code::WORKTREE_UNAVAILABLE
    );
}

#[test]
fn awkward_names_and_binary_bodies_are_read_byte_for_byte_from_every_source() {
    let fixture = Fixture::new();
    let names: &[&[u8]] = &[
        b"with space.txt",
        b"new\nline.txt",
        b"a*[b]?.txt",
        b":(glob)x",
        b"100%.txt",
    ];
    let binary: &[u8] = b"\x89PNG\r\n\x1a\n\x00\xff\xfe binary \x00";
    let files: Vec<(&[u8], &[u8])> = names
        .iter()
        .map(|name| (*name, *name))
        .chain([(&b"image.png"[..], binary)])
        .collect();
    let repo = repo_with("browse-names", &files);
    // A decoy that a pattern reading of `a*[b]?.txt` would also match.
    std::fs::write(repo.join("aXbY.txt"), b"decoy").unwrap();
    // APFS stores only UTF-8 names, so a name that is not UTF-8 can exist here only in Git's own
    // objects — which is where a branch or another checkout brings one from.
    let not_utf8: &[u8] = b"caf\xe9.bin";
    std::fs::write(repo.join("blob-source"), not_utf8).unwrap();
    let oid = String::from_utf8(git(&repo, &["hash-object", "-w", "blob-source"])).unwrap();
    let mut cacheinfo = format!("100644,{},", oid.trim()).into_bytes();
    cacheinfo.extend_from_slice(not_utf8);
    git_bytes(
        &repo,
        &[b"update-index", b"--add", b"--cacheinfo", &cacheinfo],
    );
    git(&repo, &["add", "aXbY.txt"]);
    git(&repo, &["commit", "-q", "-m", "more"]);
    let project = fixture.project("names", &repo);
    let branch = ReadFrom::Branch {
        branch: "main".to_string(),
    };

    let (entries, complete) = fixture.list(&project, None, "").unwrap();
    assert!(complete);
    for name in names {
        let wire = wire_path::encode(name);
        assert!(
            entries
                .iter()
                .any(|e| e.name == wire && e.kind == EntryKind::File),
            "{wire:?} not listed"
        );
        for from in [ReadFrom::Live, ReadFrom::Index, branch.clone()] {
            let (_, file) = fixture.read(&project, None, &wire, from.clone()).unwrap();
            assert_eq!(bytes_of(&file), *name, "{wire:?} from {from:?}");
        }
    }
    for from in [ReadFrom::Index, branch.clone()] {
        let (_, file) = fixture.read(&project, None, "caf%E9.bin", from).unwrap();
        assert_eq!(bytes_of(&file), not_utf8);
    }
    let (source, file) = fixture.read(&project, None, "image.png", branch).unwrap();
    assert_eq!(file.kind, ContentKind::Binary);
    assert_eq!(file.media_type, Some("image/png"));
    assert_eq!(bytes_of(&file), binary);
    let ContentSource::Commit { commit, blob, .. } = source else {
        panic!("not a commit source");
    };
    let head = String::from_utf8(git(&repo, &["rev-parse", "HEAD"])).unwrap();
    assert_eq!(commit, head.trim());
    let blob_bytes = git(&repo, &["cat-file", "blob", &blob]);
    assert_eq!(blob_bytes, binary, "the blob id names what was read");
}

#[test]
fn index_and_commit_reads_report_the_object_they_read() {
    let fixture = Fixture::new();
    let repo = repo_with("browse-objects", &[(b"sub/a.txt", b"committed\n")]);
    std::fs::write(repo.join("sub/a.txt"), b"staged\n").unwrap();
    git(&repo, &["add", "sub/a.txt"]);
    std::fs::write(repo.join("sub/a.txt"), b"live\n").unwrap();
    let project = fixture.project("p", &repo.join("sub"));
    let head = String::from_utf8(git(&repo, &["rev-parse", "HEAD"]))
        .unwrap()
        .trim()
        .to_string();

    for from in [ReadFrom::Live, ReadFrom::Index] {
        let read = fixture.read(&project, None, "", from);
        assert_eq!(code(read), error_code::UNSUPPORTED_FILE_TYPE);
    }
    let (live, file) = fixture
        .read(&project, None, "a.txt", ReadFrom::Live)
        .unwrap();
    assert_eq!(bytes_of(&file), b"live\n");
    let ContentSource::Live { version, .. } = &live else {
        panic!("not a live source");
    };
    std::fs::write(repo.join("sub/a.txt"), b"rewritten\n").unwrap();
    let (rewritten, _) = fixture
        .read(&project, None, "a.txt", ReadFrom::Live)
        .unwrap();
    let ContentSource::Live {
        version: rewritten, ..
    } = rewritten
    else {
        panic!("not a live source");
    };
    assert_ne!(&rewritten, version, "a rewrite changes the version");
    // A file-level refusal names the path as the client did, inside the project.
    let missing = fixture
        .read(&project, None, "missing.txt", ReadFrom::Index)
        .unwrap_err();
    let missing = missing.downcast_ref::<CodedError>().unwrap();
    assert_eq!(missing.code, error_code::FILE_NOT_FOUND);
    assert_eq!(missing.params["path"], "missing.txt");
    assert!(matches!(live, ContentSource::Live { .. }));
    let (index, file) = fixture
        .read(&project, None, "a.txt", ReadFrom::Index)
        .unwrap();
    assert_eq!(bytes_of(&file), b"staged\n");
    let ContentSource::Index { blob, .. } = index else {
        panic!()
    };
    assert_eq!(git(&repo, &["cat-file", "blob", &blob]), b"staged\n");
    let (commit, file) = fixture
        .read(
            &project,
            None,
            "a.txt",
            ReadFrom::Commit {
                commit: head.clone(),
            },
        )
        .unwrap();
    assert_eq!(bytes_of(&file), b"committed\n");
    assert!(matches!(commit, ContentSource::Commit { commit, branch: None, .. } if commit == head));
}

#[test]
fn branch_and_commit_names_are_never_evaluated_as_revision_expressions() {
    let fixture = Fixture::new();
    let repo = repo_with("browse-refs", &[(b"a.txt", b"one\n")]);
    std::fs::write(repo.join("a.txt"), b"two\n").unwrap();
    git(&repo, &["commit", "-q", "-am", "second"]);
    let project = fixture.project("p", &repo);
    let read_branch = |branch: &str| {
        fixture.read(
            &project,
            None,
            "a.txt",
            ReadFrom::Branch {
                branch: branch.to_string(),
            },
        )
    };
    for expression in [
        "main~1",
        "main^",
        "@{-1}",
        "-x",
        "main:a.txt",
        "HEAD@{1}",
        "",
    ] {
        assert_eq!(
            code(read_branch(expression)),
            error_code::INVALID_BRANCH_NAME,
            "{expression:?}"
        );
    }
    assert_eq!(code(read_branch("HEAD")), error_code::UNKNOWN_BRANCH);
    assert_eq!(code(read_branch("missing")), error_code::UNKNOWN_BRANCH);
    // A revision lookup of `refs/heads/ghost` would fall back to this reference.
    git(&repo, &["update-ref", "refs/tags/refs/heads/ghost", "HEAD"]);
    assert_eq!(code(read_branch("ghost")), error_code::UNKNOWN_BRANCH);
    let read_commit = |commit: &str| {
        fixture.read(
            &project,
            None,
            "a.txt",
            ReadFrom::Commit {
                commit: commit.to_string(),
            },
        )
    };
    assert_eq!(code(read_commit("HEAD~1")), error_code::INVALID_COMMIT);
    assert_eq!(
        code(read_commit(&"0".repeat(40))),
        error_code::UNKNOWN_COMMIT
    );
    let tree = String::from_utf8(git(&repo, &["rev-parse", "HEAD^{tree}"])).unwrap();
    assert_eq!(code(read_commit(tree.trim())), error_code::UNKNOWN_COMMIT);
    git(&repo, &["tag", "-a", "-m", "annotated", "annotated"]);
    let tag = String::from_utf8(git(&repo, &["rev-parse", "annotated"])).unwrap();
    assert_eq!(
        code(read_commit(tag.trim())),
        error_code::INVALID_COMMIT,
        "a tag is no commit"
    );
    assert_eq!(
        code(fixture.read(
            &fixture.project("plain", &ScratchDir::new("browse-refs-plain")),
            None,
            "a.txt",
            ReadFrom::Index
        )),
        error_code::NOT_A_GIT_REPOSITORY
    );
}

#[test]
fn links_are_followed_inside_the_scope_and_refused_outside_it() {
    let fixture = Fixture::new();
    let outside = ScratchDir::new("browse-outside");
    std::fs::write(outside.join("secret"), b"not the project's").unwrap();
    let project_dir = ScratchDir::new("browse-links-project");
    std::fs::create_dir(project_dir.join("dir")).unwrap();
    std::fs::write(project_dir.join("dir/real.txt"), b"inside\n").unwrap();
    let link = |target: &Path, name: &str| {
        std::os::unix::fs::symlink(target, project_dir.join(name)).unwrap()
    };
    link(&project_dir.join("dir/real.txt"), "inside-link");
    link(Path::new("dir"), "dir-link");
    link(&outside.join("secret"), "escape");
    link(&outside, "escape-dir");
    link(&project_dir.join("nothing"), "dangling");
    let project = fixture.project("p", &project_dir);

    let (_, file) = fixture
        .read(&project, None, "inside-link", ReadFrom::Live)
        .unwrap();
    assert_eq!(bytes_of(&file), b"inside\n");
    let (_, file) = fixture
        .read(&project, None, "dir-link/real.txt", ReadFrom::Live)
        .unwrap();
    assert_eq!(bytes_of(&file), b"inside\n");
    assert_eq!(
        code(fixture.read(&project, None, "escape", ReadFrom::Live)),
        error_code::OUTSIDE_SCOPE
    );
    assert_eq!(
        code(fixture.read(&project, None, "escape-dir/secret", ReadFrom::Live)),
        error_code::OUTSIDE_SCOPE
    );
    assert_eq!(
        code(fixture.list(&project, None, "escape-dir")),
        error_code::OUTSIDE_SCOPE
    );
    assert_eq!(
        code(fixture.read(&project, None, "dangling", ReadFrom::Live)),
        error_code::FILE_NOT_FOUND
    );
    assert_eq!(
        code(fixture.read(&project, None, "../secret", ReadFrom::Live)),
        error_code::INVALID_PATH
    );

    let (entries, _) = fixture.list(&project, None, "").unwrap();
    let target = |name: &str| entries.iter().find(|e| e.name == name).unwrap().target;
    assert_eq!(target("inside-link"), Some(LinkTarget::File));
    assert_eq!(target("dir-link"), Some(LinkTarget::Directory));
    assert_eq!(target("escape"), Some(LinkTarget::Outside));
    assert_eq!(target("dangling"), Some(LinkTarget::Missing));
}

#[test]
fn a_listed_file_carries_the_version_a_live_read_reports() {
    let fixture = Fixture::new();
    let project_dir = ScratchDir::new("browse-entry-version");
    std::fs::write(project_dir.join("a.txt"), b"aaaa\n").unwrap();
    std::fs::create_dir(project_dir.join("dir")).unwrap();
    let project = fixture.project("p", &project_dir);
    let listed = |name: &str| {
        let (entries, _) = fixture.list(&project, None, "").unwrap();
        entries
            .into_iter()
            .find(|e| e.name == name)
            .unwrap()
            .version
    };
    let read = || match fixture
        .read(&project, None, "a.txt", ReadFrom::Live)
        .unwrap()
        .0
    {
        ContentSource::Live { version, .. } => version,
        other => panic!("not a live source: {other:?}"),
    };

    let before = listed("a.txt");
    assert_eq!(before, Some(read()));
    assert_eq!(listed("dir"), None, "only a regular file has a version");
    // Rewritten in place at the same size, with a modification time of its own so that the change
    // shows even where the clock is coarser than two writes.
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(project_dir.join("a.txt"))
        .unwrap();
    std::io::Write::write_all(&mut &file, b"bbbb\n").unwrap();
    file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000))
        .unwrap();
    drop(file);
    let after = listed("a.txt");
    assert_ne!(
        after, before,
        "a same-size rewrite is told apart from the listing alone"
    );
    assert_eq!(after, Some(read()));
}

#[test]
fn special_files_are_refused_without_being_opened() {
    let fixture = Fixture::new();
    let project_dir = ScratchDir::new("browse-special");
    let fifo = project_dir.join("fifo");
    let fifo_c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    // SAFETY: a plain `mkfifo` on a NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
    let _socket = std::os::unix::net::UnixListener::bind(project_dir.join("socket")).unwrap();
    let project = fixture.project("p", &project_dir);

    // Refused from their metadata: nothing is read from either, and the FIFO is never opened.
    for (name, file_type) in [("fifo", "fifo"), ("socket", "socket")] {
        let err = fixture
            .read(&project, None, name, ReadFrom::Live)
            .unwrap_err();
        let coded = err.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::UNSUPPORTED_FILE_TYPE);
        assert_eq!(coded.params["file_type"], file_type);
    }
    let (entries, _) = fixture.list(&project, None, "").unwrap();
    assert!(entries.iter().all(|e| e.kind == EntryKind::Other));
}

#[test]
fn missing_and_unreadable_sources_are_told_apart() {
    let fixture = Fixture::new();
    let project_dir = ScratchDir::new("browse-access");
    std::fs::write(project_dir.join("locked.txt"), b"x").unwrap();
    std::fs::create_dir(project_dir.join("locked-dir")).unwrap();
    std::fs::write(project_dir.join("locked-dir/inner.txt"), b"x").unwrap();
    // SAFETY: a plain query of this process's effective user.
    if unsafe { libc::geteuid() } == 0 {
        // Permissions do not stop the superuser, so there is nothing to refuse.
        return;
    }
    let chmod = |path: &Path, mode| {
        std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(mode)).unwrap()
    };
    chmod(&project_dir.join("locked.txt"), 0o000);
    chmod(&project_dir.join("locked-dir"), 0o000);
    let project = fixture.project("p", &project_dir);
    let gone = fixture.project("gone", &project_dir.join("never-existed"));

    let locked = fixture.read(&project, None, "locked.txt", ReadFrom::Live);
    let locked_dir = fixture.read(&project, None, "locked-dir/inner.txt", ReadFrom::Live);
    chmod(&project_dir.join("locked-dir"), 0o700);
    assert_eq!(code(locked), error_code::PERMISSION_DENIED);
    assert_eq!(code(locked_dir), error_code::PERMISSION_DENIED);
    assert_eq!(
        code(fixture.read(&project, None, "absent.txt", ReadFrom::Live)),
        error_code::FILE_NOT_FOUND
    );
    assert_eq!(
        code(fixture.read(&project, None, "locked-dir", ReadFrom::Live)),
        error_code::UNSUPPORTED_FILE_TYPE
    );
    assert_eq!(
        code(fixture.read(&gone, None, "a.txt", ReadFrom::Live)),
        error_code::SOURCE_UNAVAILABLE
    );
    assert_eq!(
        code(fixture.source_result(&gone)),
        error_code::SOURCE_UNAVAILABLE
    );
    assert_eq!(
        code(fixture.read("no-such-project", None, "a.txt", ReadFrom::Live)),
        error_code::UNKNOWN_PROJECT
    );
}

#[test]
fn a_file_or_blob_over_the_byte_budget_is_refused() {
    let fixture = Fixture::new();
    let big = vec![b'x'; budget::MAX_FILE_BYTES + 1];
    let repo = repo_with(
        "browse-big",
        &[
            (b"big.txt", &big),
            (b"fits.txt", &big[..budget::MAX_FILE_BYTES]),
        ],
    );
    let project = fixture.project("p", &repo);
    for from in [
        ReadFrom::Live,
        ReadFrom::Index,
        ReadFrom::Branch {
            branch: "main".into(),
        },
    ] {
        let err = fixture
            .read(&project, None, "big.txt", from.clone())
            .unwrap_err();
        let coded = err.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::LIMIT_EXCEEDED, "{from:?}");
        assert_eq!(coded.params["size"], big.len().to_string());
        let (_, file) = fixture.read(&project, None, "fits.txt", from).unwrap();
        assert_eq!(file.size, budget::MAX_FILE_BYTES as u64);
    }
}

#[test]
fn a_file_read_stops_at_its_budget_however_much_the_source_would_give() {
    let endless = std::io::repeat(b'x');
    let outcome = live::read_bounded(endless, 1000, 0, &AtomicBool::new(false));
    assert!(matches!(outcome, Err(live::ReadFailure::Exceeded)));
}

#[test]
fn a_listing_cut_at_its_budget_says_so() {
    let dir = ScratchDir::new("browse-many");
    for n in 0..20 {
        std::fs::write(dir.join(format!("f{n:02}")), b"").unwrap();
    }
    let scope = std::fs::canonicalize(&*dir).unwrap();
    let root = RelPath::parse("").unwrap();
    let never = AtomicBool::new(false);
    let list = |entries, bytes| live::list_resolved(&scope, &scope, &root, entries, bytes, &never);
    let cut = list(5, usize::MAX).unwrap();
    assert_eq!((cut.entries.len(), cut.complete), (5, false));
    let by_bytes = list(usize::MAX, 9).unwrap();
    assert_eq!((by_bytes.entries.len(), by_bytes.complete), (3, false));
    let whole = list(20, usize::MAX).unwrap();
    assert_eq!((whole.entries.len(), whole.complete), (20, true));
}

#[test]
fn a_file_rewritten_during_a_read_is_never_stitched_from_two_versions() {
    let dir = ScratchDir::new("browse-race");
    let scope = std::fs::canonicalize(&*dir).unwrap();
    let path = scope.join("busy");
    let size = 2 * 1024 * 1024;
    std::fs::write(&path, vec![b'a'; size]).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let writer = {
        let (stop, path) = (stop.clone(), path.clone());
        std::thread::spawn(move || {
            use std::io::{Seek, Write};
            let bodies = [vec![b'a'; size], vec![b'b'; size]];
            let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
            // Each round truncates the file and writes the other letter in pieces, as a program
            // saving a file does: every state the file passes through is one letter only, so a
            // body holding both was stitched from reads either side of a change.
            for body in bodies.iter().cycle() {
                if stop.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                file.set_len(0).unwrap();
                file.seek(std::io::SeekFrom::Start(0)).unwrap();
                for piece in body.chunks(64 * 1024) {
                    file.write_all(piece).unwrap();
                }
            }
        })
    };
    let rel = RelPath::parse("busy").unwrap();
    let never = AtomicBool::new(false);
    let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
    let mut changes = 0;
    while changes < 3 && std::time::Instant::now() < deadline {
        match live::read_file(&scope, &rel, budget::MAX_FILE_BYTES, &never) {
            Ok(file) => {
                let first = file.bytes.first().copied();
                assert!(
                    file.bytes.iter().all(|&b| Some(b) == first),
                    "a body stitched from two versions was returned"
                );
            }
            Err(err) => {
                assert_eq!(
                    err.downcast_ref::<CodedError>().unwrap().code,
                    error_code::SOURCE_CHANGED
                );
                changes += 1;
            }
        }
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    writer.join().unwrap();
    assert_eq!(changes, 3, "no read ever overlapped a write");
}

#[test]
fn a_home_directory_repository_is_not_taken_for_a_project_s_repository() {
    let home = ScratchDir::new("browse-home");
    let home = std::fs::canonicalize(&*home).unwrap();
    git(&home, &["init", "-q"]);
    std::fs::create_dir(home.join("project")).unwrap();
    let make = || {
        let mut env = test_env(&[]);
        env.set_ceiling(Some(home.clone()));
        Ok(env)
    };
    let git = Git::new(&make).with_ceiling(Some(home.clone()));
    let root = source::resolve_root(home.join("project").to_str().unwrap()).unwrap();
    let discovered = source::discover(&root, &git, &AtomicBool::new(false)).unwrap();
    assert!(
        matches!(discovered, source::Discovery::NotARepository),
        "{discovered:?}"
    );
}

/// A burst of large reads over a real control socket, past the per-connection bound, with an
/// ordinary request sent after it: the ordinary request is answered while the reads are still
/// being served, the reads past the bound are refused rather than queued, every read that was
/// taken is answered whole, and every retained byte is given back at the end.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_burst_of_reads_neither_starves_the_control_socket_nor_outgrows_its_bounds() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let fixture = Fixture::new();
    let project_dir = ScratchDir::new("browse-burst");
    std::fs::write(
        project_dir.join("big.bin"),
        vec![0u8; budget::MAX_FILE_BYTES],
    )
    .unwrap();
    let project = fixture.project("p", &project_dir);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = crate::server::router(fixture.state.clone());
    tokio::spawn(async move { axum::serve(listener, router).await });
    let (socket, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws/control"))
        .await
        .unwrap();
    let (mut sink, mut stream) = socket.split();

    let reads = budget::MAX_PENDING_REQUESTS + 8;
    for n in 0..reads {
        let request = serde_json::json!({
            "type": "read_project_file",
            "id": format!("read-{n}"),
            "project": project,
            "path": "big.bin",
        });
        sink.send(Message::Text(request.to_string())).await.unwrap();
    }
    let ordinary =
        serde_json::json!({"type": "list_pages", "id": "ordinary", "console_session": "none"});
    sink.send(Message::Text(ordinary.to_string()))
        .await
        .unwrap();

    let mut answered = std::collections::HashMap::new();
    let mut order = Vec::new();
    while answered.len() < reads + 1 {
        let frame = tokio::time::timeout(crate::test_support::PATIENCE, stream.next())
            .await
            .expect("every request is answered in time")
            .unwrap()
            .unwrap();
        let Message::Text(text) = frame else { continue };
        let event: serde_json::Value = serde_json::from_str(&text).unwrap();
        let Some(id) = event["id"].as_str() else {
            continue;
        };
        order.push(id.to_string());
        answered.insert(id.to_string(), event);
    }
    assert_eq!(answered["ordinary"]["code"], error_code::UNKNOWN_SESSION);
    let refused = answered
        .values()
        .filter(|event| event["code"] == error_code::LIMIT_EXCEEDED)
        .count();
    let served: Vec<_> = answered
        .values()
        .filter(|event| event["type"] == "project_file")
        .collect();
    assert!(refused >= 1, "nothing was refused past the bound");
    assert_eq!(refused + served.len(), reads);
    assert!(served
        .iter()
        .all(|event| event["file"]["size"] == budget::MAX_FILE_BYTES));
    let last_read = order
        .iter()
        .rposition(|id| answered[id]["type"] == "project_file")
        .unwrap();
    let ordinary_at = order.iter().position(|id| id == "ordinary").unwrap();
    assert!(
        ordinary_at < last_read,
        "the ordinary request waited behind every read: {order:?}"
    );
    // The writer gives a frame's bytes back just after sending it, so the last one may still be on
    // its way back when the last reply has been read.
    let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
    while fixture.state.browse_gate.retained_available() != budget::MAX_RETAINED_BYTES {
        assert!(
            std::time::Instant::now() < deadline,
            "retained bytes were never given back"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

#[test]
fn a_body_is_text_only_when_json_escaping_at_most_doubles_it() {
    let log = b"\x1b[31mred\x1b[0m and plain text\n".to_vec();
    assert_eq!(file_content(log).kind, ContentKind::Text);
    let controls = vec![0x01u8; 64];
    let binary = file_content(controls.clone());
    assert_eq!(binary.kind, ContentKind::Binary);
    assert_eq!(bytes_of(&binary), controls);
    assert_eq!(file_content(b"caf\xe9".to_vec()).kind, ContentKind::Binary);
}
