//! Browse requests served end to end against real directories and repositories: what each project
//! shape resolves to, what a read returns or refuses, and that every path reaches exactly the bytes
//! it names.

use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use base64::Engine as _;

use super::git::tests::test_env;
use super::git::GitEnv;
use super::*;
use crate::protocol::{
    Agent, BrowseEntry, BrowseRequest, Console, ContentKind, EntryKind, LinkTarget, Project,
    ProjectAccount, ProjectSource,
};
use crate::state::AppState;
use crate::test_support::{app_state, git, git_bytes, repo_with, ScratchDir, TEST_KEY};

#[test]
fn search_request_resolves_the_stored_project_and_echoes_the_query() {
    let fixture = Fixture::new();
    let directory = ScratchDir::new("browse-search-request");
    std::fs::create_dir_all(directory.join("unopened/deeper")).unwrap();
    std::fs::write(directory.join("unopened/deeper/Needle.txt"), b"match").unwrap();
    let project = fixture.project("search-project", &directory);
    let request: BrowseRequest = serde_json::from_value(serde_json::json!({
        "id": "search-1", "slot": "files-filter", "type": "search_project_files",
        "project": project, "query": "  NEEDLE  "
    }))
    .unwrap();
    assert_eq!(request.slot.as_deref(), Some("files-filter"));
    let reply = serve(
        &fixture.state,
        request.id,
        request.body,
        &git_env,
        &AtomicBool::new(false),
    )
    .unwrap();
    let wire = serde_json::to_value(reply).unwrap();
    assert_eq!(wire["type"], "project_file_matches");
    assert_eq!(wire["id"], "search-1");
    assert_eq!(wire["project"], "search-project");
    assert_eq!(wire["query"], "  NEEDLE  ");
    assert_eq!(
        wire["paths"],
        serde_json::json!(["unopened/deeper/Needle.txt"])
    );
    assert_eq!(wire["complete"], true);
    let error = fixture
        .serve(BrowseBody::SearchProjectFiles {
            project: "unknown".into(),
            query: "needle".into(),
            include_ignored: false,
        })
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<CodedError>().unwrap().code,
        error_code::UNKNOWN_PROJECT
    );
}

pub(crate) struct Fixture {
    pub(crate) state: Arc<AppState>,
    _dir: ScratchDir,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        let (state, dir) = app_state("browse");
        let workdir = dir.join("workdir");
        std::fs::create_dir(&workdir).unwrap();
        state
            .store
            .insert_console(&Console {
                id: "console".to_string(),
                name: "Console".to_string(),
                workdir: workdir.to_string_lossy().into_owned(),
                default_agent: Agent::Claude,
                project_session_agent: None,
                claude_project_session_model: None,
                claude_project_session_effort: None,
                codex_project_session_model: None,
                codex_project_session_effort: None,
                grok_project_session_model: None,
                grok_project_session_effort: None,
                claude_account_id: None,
                codex_account_id: None,
                grok_account_id: None,
                claude_console_session_model: None,
                claude_console_session_effort: None,
                codex_console_session_model: None,
                codex_console_session_effort: None,
                grok_console_session_model: None,
                grok_console_session_effort: None,
                icon: None,
                console_session_prompt: None,
                created_at: 0,
            })
            .unwrap();
        Self { state, _dir: dir }
    }

    /// Registers `path` as project `id`, stored as given.
    pub(crate) fn project(&self, id: &str, path: &Path) -> String {
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
                claude_account_id: ProjectAccount::Inherit,
                codex_account_id: ProjectAccount::Inherit,
                grok_account_id: ProjectAccount::Inherit,
                claude_session_model: None,
                claude_session_effort: None,
                codex_session_model: None,
                codex_session_effort: None,
                grok_session_model: None,
                grok_session_effort: None,
                trust_consent: false,
                pinned: false,
                tags: Vec::new(),
                description: None,
                git_workflow: Default::default(),
            })
            .unwrap();
        id.to_string()
    }

    pub(super) fn serve(&self, body: BrowseBody) -> anyhow::Result<Event> {
        self.serve_in(&[], body)
    }

    /// [`Fixture::serve`] with `extra` added to the environment `git` runs in.
    fn serve_in(&self, extra: &[(&str, &str)], body: BrowseBody) -> anyhow::Result<Event> {
        let make = || Ok(test_env(extra));
        serve(&self.state, None, body, &make, &AtomicBool::new(false))
    }

    fn source_result(&self, project: &str) -> anyhow::Result<Event> {
        self.serve(BrowseBody::GetProjectSource {
            project: project.to_string(),
        })
    }

    pub(super) fn source(&self, project: &str) -> ProjectSourceInfo {
        match self.serve(BrowseBody::GetProjectSource {
            project: project.to_string(),
        }) {
            Ok(Event::ProjectSource { source, .. }) => source,
            other => panic!("not a source: {other:?}"),
        }
    }

    pub(super) fn read(
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

    pub(super) fn list(
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

pub(super) fn git_env() -> anyhow::Result<GitEnv> {
    Ok(test_env(&[]))
}

pub(super) fn bytes_of(file: &FileContent) -> Vec<u8> {
    match file.kind {
        ContentKind::Text => file.text.clone().unwrap().into_bytes(),
        ContentKind::Binary => base64::engine::general_purpose::STANDARD
            .decode(file.data.as_ref().unwrap())
            .unwrap(),
    }
}

/// The `code` of the coded error `result` failed with.
pub(super) fn code<T: std::fmt::Debug>(result: anyhow::Result<T>) -> crate::protocol::Code {
    let err = result.expect_err("a refusal");
    err.downcast_ref::<CodedError>()
        .unwrap_or_else(|| panic!("not coded: {err:#}"))
        .code
}

/// The `cause` param of the `source_unavailable` `result` failed with.
fn unavailable_cause<T: std::fmt::Debug>(result: anyhow::Result<T>) -> String {
    let err = result.expect_err("a refusal");
    let coded = err.downcast_ref::<CodedError>().unwrap();
    assert_eq!(coded.code, error_code::SOURCE_UNAVAILABLE);
    coded.params["cause"].clone()
}

pub(super) fn canonical(path: &Path) -> String {
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
    // A worktree on a branch with no files, made without `worktree add --orphan` (Git 2.42).
    git(
        &repo,
        &["worktree", "add", "-q", "--detach", bare.to_str().unwrap()],
    );
    git(&bare, &["checkout", "-q", "--orphan", "empty"]);
    git(&bare, &["rm", "-rfq", "."]);

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
    assert_eq!(linked_entry.scope_cause, Some("link"));
    assert_eq!(
        unavailable_cause(fixture.read(
            &project,
            Some(&linked_entry.id),
            "top.txt",
            ReadFrom::Live
        )),
        "link",
        "a link in another worktree must not widen the scope to its root"
    );
    let other_entry = git_info
        .worktrees
        .iter()
        .find(|w| w.root == canonical(&other))
        .unwrap();
    assert!(other_entry.scope_present);
    assert_eq!(other_entry.scope_cause, None);
    let other_id = &other_entry.id;
    let bare_entry = git_info
        .worktrees
        .iter()
        .find(|w| w.root == canonical(&bare))
        .unwrap();
    assert!(!bare_entry.scope_present);
    assert_eq!(bare_entry.scope_cause, Some("missing"));

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
        unavailable_cause(fixture.list(&project, Some(&bare_entry.id), "")),
        "missing"
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
    std::fs::write(project_dir.join("plain.txt"), b"x").unwrap();
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
    let file = fixture.project("file", &project_dir.join("plain.txt"));

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
        unavailable_cause(fixture.read(&gone, None, "a.txt", ReadFrom::Live)),
        "missing"
    );
    assert_eq!(
        unavailable_cause(fixture.read(&file, None, "a.txt", ReadFrom::Live)),
        "not_a_directory"
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
    let (socket, _) = tokio_tungstenite::connect_async(format!(
        "ws://127.0.0.1:{port}/ws/control?key={TEST_KEY}"
    ))
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
    assert_eq!(
        answered["ordinary"]["code"],
        error_code::UNKNOWN_SESSION.as_str()
    );
    let refused = answered
        .values()
        .filter(|event| event["code"] == error_code::LIMIT_EXCEEDED.as_str())
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

#[test]
fn host_path_actions_validate_wire_paths_and_recheck_worktree_scopes() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "host-path-actions",
        &[(b"sub/a%.txt", b"main"), (b"top.txt", b"top")],
    );
    let linked = ScratchDir::new("host-path-linked");
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
    let project = fixture.project("sub", &repo.join("sub"));
    let source = fixture.source(&project).git.unwrap();
    let worktree = source
        .worktrees
        .iter()
        .find(|entry| !entry.main)
        .unwrap()
        .id
        .clone();
    let resolve = |tree: Option<&str>, path: &str| {
        resolve_project_path(&fixture.state, &project, tree, path, &git_env)
    };
    assert_eq!(
        resolve(None, "a%25.txt").unwrap(),
        std::fs::canonicalize(repo.join("sub/a%.txt")).unwrap()
    );
    assert_eq!(
        resolve(Some(&worktree), "a%25.txt").unwrap(),
        std::fs::canonicalize(other.join("sub/a%.txt")).unwrap()
    );
    std::os::unix::fs::symlink("a%.txt", repo.join("sub/link")).unwrap();
    assert_eq!(
        resolve(None, "link").unwrap(),
        std::fs::canonicalize(repo.join("sub"))
            .unwrap()
            .join("link")
    );
    std::os::unix::fs::symlink(repo.join("top.txt"), repo.join("sub/escape")).unwrap();
    for (path, code) in [
        ("../top.txt", error_code::INVALID_PATH),
        ("/etc/passwd", error_code::INVALID_PATH),
        ("a%.txt", error_code::INVALID_PATH),
        ("missing", error_code::FILE_NOT_FOUND),
        ("escape", error_code::OUTSIDE_SCOPE),
    ] {
        let error = resolve(None, path).unwrap_err();
        assert_eq!(
            error.downcast_ref::<CodedError>().unwrap().code,
            code,
            "{path}"
        );
    }
    let error = resolve(Some("unknown-worktree"), "a%25.txt").unwrap_err();
    assert_eq!(
        error.downcast_ref::<CodedError>().unwrap().code,
        error_code::WORKTREE_UNAVAILABLE
    );
    git(
        &repo,
        &["worktree", "remove", "--force", other.to_str().unwrap()],
    );
    let error = resolve(Some(&worktree), "a%25.txt").unwrap_err();
    assert_eq!(
        error.downcast_ref::<CodedError>().unwrap().code,
        error_code::WORKTREE_UNAVAILABLE
    );
}

/// A repository whose files exercise every way Git ignores one: a root pattern, a nested
/// `.gitignore`, a whole ignored folder, and a tracked file that still matches a pattern.
fn ignoring_repo(label: &str) -> ScratchDir {
    let repo = repo_with(
        label,
        &[
            (b".gitignore", b"build/\n*.log\n"),
            (b"sub/.gitignore", b"*.tmp\n"),
            (b"keep/tracked.log", b"tracked\n"),
            (b"keep/plain.txt", b"plain\n"),
            (b"sub/plain.txt", b"plain\n"),
        ],
    );
    git(&repo, &["add", "-f", "keep/tracked.log"]);
    git(
        &repo,
        &["commit", "-q", "-m", "track a file that matches a pattern"],
    );
    for (name, body) in [
        ("build/inner/output.txt", &b"out"[..]),
        ("build/top.txt", b"out"),
        ("debug.log", b"log"),
        ("keep/other.log", b"log"),
        ("sub/scratch.tmp", b"tmp"),
        ("odd \"name\"\n.log", b"log"),
        (":(glob)magic.log", b"log"),
        ("star*.txt", b"not ignored"),
        ("root.tmp", b"not ignored here"),
    ] {
        let path = repo.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
    repo
}

impl Fixture {
    fn ignored_in(&self, project: &str, path: &str) -> Vec<String> {
        let (entries, _) = self.list(project, None, path).unwrap();
        entries
            .into_iter()
            .filter(|entry| entry.ignored)
            .map(|entry| entry.name)
            .collect()
    }

    fn search(&self, project: &str, query: &str, include_ignored: bool) -> Vec<String> {
        self.search_marked(project, query, include_ignored).0
    }

    /// The matches and, of them, the ones the reply says Git ignores.
    fn search_marked(
        &self,
        project: &str,
        query: &str,
        include_ignored: bool,
    ) -> (Vec<String>, Vec<String>) {
        match self
            .serve(BrowseBody::SearchProjectFiles {
                project: project.to_string(),
                query: query.to_string(),
                include_ignored,
            })
            .unwrap()
        {
            Event::ProjectFileMatches { paths, ignored, .. } => (paths, ignored),
            other => panic!("not matches: {other:?}"),
        }
    }
}

#[test]
fn a_listing_marks_what_git_ignores() {
    let fixture = Fixture::new();
    let repo = ignoring_repo("browse-ignored-list");
    let project = fixture.project("p", &repo);
    assert_eq!(
        fixture.ignored_in(&project, ""),
        [
            ":(glob)magic.log",
            "build",
            "debug.log",
            "odd \"name\"\n.log"
        ],
        "a folder and files named by the root .gitignore, however odd their names"
    );
    assert_eq!(
        fixture.ignored_in(&project, "sub"),
        ["scratch.tmp"],
        "a nested .gitignore applies below its folder only"
    );
    assert_eq!(
        fixture.ignored_in(&project, "keep"),
        ["other.log"],
        "a tracked file is not ignored however it matches a pattern"
    );
    assert_eq!(
        fixture.ignored_in(&project, "build"),
        ["inner", "top.txt"],
        "everything inside an ignored folder is ignored"
    );
    assert_eq!(fixture.ignored_in(&project, "build/inner"), ["output.txt"]);
}

#[test]
fn the_exclude_file_makes_an_entry_ignored_too() {
    let fixture = Fixture::new();
    let repo = repo_with("browse-ignored-exclude", &[(b"a.txt", b"a\n")]);
    std::fs::write(repo.join("secret.txt"), b"s").unwrap();
    std::fs::write(repo.join(".git/info/exclude"), b"secret.txt\n").unwrap();
    let project = fixture.project("p", &repo);
    assert_eq!(fixture.ignored_in(&project, ""), ["secret.txt"]);
}

#[test]
fn nothing_is_ignored_outside_a_repository() {
    let fixture = Fixture::new();
    let directory = ScratchDir::new("browse-ignored-none");
    std::fs::write(directory.join(".gitignore"), b"*.log\n").unwrap();
    std::fs::write(directory.join("debug.log"), b"log").unwrap();
    let project = fixture.project("p", &directory);
    let (entries, complete) = fixture.list(&project, None, "").unwrap();
    assert!(complete);
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|entry| !entry.ignored));
    assert_eq!(fixture.search(&project, "log", false), ["debug.log"]);
}

#[test]
fn a_git_that_fails_leaves_the_listing_and_the_search_unfiltered() {
    let fixture = Fixture::new();
    let repo = ignoring_repo("browse-ignored-failing");
    let project = fixture.project("p", &repo);
    // A repository whose metadata `git` cannot read fails every `git` run in it.
    std::fs::write(repo.join(".git/HEAD"), b"garbage\n").unwrap();
    let (entries, complete) = fixture.list(&project, None, "").unwrap();
    assert!(complete);
    assert!(entries.iter().all(|entry| !entry.ignored));
    assert!(fixture
        .search(&project, "debug", false)
        .contains(&"debug.log".to_string()));
}

#[test]
fn a_search_leaves_out_ignored_files_and_folders_unless_asked() {
    let fixture = Fixture::new();
    let repo = ignoring_repo("browse-ignored-search");
    let project = fixture.project("p", &repo);
    assert_eq!(
        fixture.search(&project, ".txt", false),
        ["keep/plain.txt", "star*.txt", "sub/plain.txt"]
    );
    assert_eq!(
        fixture.search(&project, ".log", false),
        ["keep/tracked.log"],
        "a tracked file that matches a pattern is still searched"
    );
    assert_eq!(
        fixture.search(&project, ".tmp", false),
        ["root.tmp"],
        "a nested .gitignore does not reach outside its folder"
    );
    assert_eq!(
        fixture.search(&project, ".txt", true),
        [
            "build/inner/output.txt",
            "build/top.txt",
            "keep/plain.txt",
            "star*.txt",
            "sub/plain.txt"
        ]
    );
    assert_eq!(
        fixture.search_marked(&project, ".txt", true).1,
        ["build/inner/output.txt", "build/top.txt"],
        "a search including ignored files says which matches Git ignores, folders' contents too"
    );
    assert!(fixture.search_marked(&project, ".txt", false).1.is_empty());
    assert_eq!(
        fixture.search(&project, ".log", true),
        [
            ":(glob)magic.log",
            "debug.log",
            "keep/other.log",
            "keep/tracked.log",
            "odd \"name\"\n.log"
        ]
    );
}

/// A listing's entries that Git ignores, and a search's matches, in `fixture` over `extra` added to
/// the environment `git` runs in.
fn ignored_with(
    fixture: &Fixture,
    extra: &[(&str, &str)],
    project: &str,
) -> (Vec<String>, Vec<String>) {
    let listing = match fixture
        .serve_in(
            extra,
            BrowseBody::ListProjectDir {
                project: project.to_string(),
                worktree: None,
                path: String::new(),
            },
        )
        .unwrap()
    {
        Event::ProjectDir { entries, .. } => entries,
        other => panic!("not a listing: {other:?}"),
    };
    let found = match fixture
        .serve_in(
            extra,
            BrowseBody::SearchProjectFiles {
                project: project.to_string(),
                query: String::new(),
                include_ignored: false,
            },
        )
        .unwrap()
    {
        Event::ProjectFileMatches { paths, .. } => paths,
        other => panic!("not matches: {other:?}"),
    };
    let ignored = listing
        .into_iter()
        .filter(|entry| entry.ignored)
        .map(|entry| entry.name)
        .collect();
    (ignored, found)
}

#[test]
fn the_global_excludes_file_makes_an_entry_ignored_too() {
    let fixture = Fixture::new();
    let repo = repo_with("browse-ignored-global", &[(b"a.txt", b"a\n")]);
    std::fs::write(repo.join("token.secret"), b"s").unwrap();
    let home = ScratchDir::new("browse-ignored-global-config");
    std::fs::write(home.join("ignore"), b"*.secret\n").unwrap();
    let config = home.join("config");
    std::fs::write(
        &config,
        format!(
            "[core]\n\texcludesFile = {}\n",
            home.join("ignore").display()
        ),
    )
    .unwrap();
    let project = fixture.project("p", &repo);
    let (ignored, found) = ignored_with(
        &fixture,
        &[("GIT_CONFIG_GLOBAL", config.to_str().unwrap())],
        &project,
    );
    assert_eq!(ignored, ["token.secret"]);
    assert_eq!(found, ["a.txt"]);
    // The positive control, in a folder of its own (a folder's entries are remembered, whatever
    // the global configuration says): without the configuration nothing is ignored.
    let control = repo_with("browse-ignored-global-control", &[(b"a.txt", b"a\n")]);
    std::fs::write(control.join("token.secret"), b"s").unwrap();
    let control = fixture.project("control", &control);
    let (ignored, found) = ignored_with(&fixture, &[], &control);
    assert!(ignored.is_empty());
    assert_eq!(found, ["a.txt", "token.secret"]);
}

#[test]
fn a_listing_longer_than_one_check_ignore_batch_is_marked_throughout() {
    let fixture = Fixture::new();
    let repo = repo_with("browse-ignored-batches", &[(b".gitignore", b"*.log\n")]);
    // Seven hundred names of two hundred bytes come to well over one batch of names.
    for index in 0..700 {
        let extension = if index % 2 == 0 { "log" } else { "txt" };
        let name = format!("{index:0>196}.{extension}");
        std::fs::write(repo.join(name), b"x").unwrap();
    }
    let project = fixture.project("p", &repo);
    let (entries, complete) = fixture.list(&project, None, "").unwrap();
    assert!(complete);
    assert_eq!(entries.len(), 702, "the files, .gitignore and .git");
    for entry in &entries {
        assert_eq!(
            entry.ignored,
            entry.name.ends_with(".log"),
            "{}",
            entry.name
        );
    }
    assert_eq!(entries.iter().filter(|entry| entry.ignored).count(), 350);
}

#[test]
fn a_cancel_during_each_git_run_ends_the_read() {
    fn cancelled(err: anyhow::Error) -> bool {
        err.downcast_ref::<Cancelled>().is_some()
    }
    // `git` stands in with a script that fails the runs `fails` names and stalls on the rest.
    let stalling = |label: &str, fails: &str| {
        let bin = ScratchDir::new(label);
        let env = crate::test_support::fake_agent(
            &bin,
            "git",
            &format!("case \"$*\" in *{fails}*) exit 128;; esac\n/bin/sleep 30\n"),
        );
        (bin.join("git").to_string_lossy().into_owned(), env, bin)
    };
    // The cancel comes a moment after the read has started.
    let cancel_soon = |read: &dyn Fn(&AtomicBool) -> anyhow::Result<()>| {
        let cancel = AtomicBool::new(false);
        let started = std::time::Instant::now();
        let result = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(400));
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            });
            read(&cancel)
        });
        assert!(started.elapsed() < std::time::Duration::from_secs(20));
        result.map(|()| false).unwrap_or_else(cancelled)
    };

    let repository = ScratchDir::new("browse-ignored-cancel-repo");
    std::fs::create_dir(repository.join(".git")).unwrap();
    std::fs::write(repository.join("a.txt"), b"a").unwrap();
    let nesting = ScratchDir::new("browse-ignored-cancel-nesting");
    std::fs::create_dir_all(nesting.join("inner/.git")).unwrap();

    // The listing's `check-ignore`.
    let (program, env, _bin) = stalling("browse-ignored-cancel-listing", "nothing-fails");
    let make = || Ok(GitEnv::new(program.clone(), env.clone()));
    let git = Git::new(&make).with_ceiling(None);
    let canonical = std::fs::canonicalize(&*repository).unwrap();
    assert!(cancel_soon(&|cancel| {
        let root = wire_path::RelPath::parse("").unwrap();
        live::list_dir(&canonical, &root, &git, cancel).map(|_| ())
    }));
    // The search's `ls-files` of the project's repository.
    assert!(cancel_soon(&|cancel| {
        live::search_files(&canonical, "a", false, &git, cancel).map(|_| ())
    }));
    // The `ls-files` of a repository nested in a project that is in none.
    let nested = std::fs::canonicalize(&*nesting).unwrap();
    assert!(cancel_soon(&|cancel| {
        live::search_files(&nested, "a", false, &git, cancel).map(|_| ())
    }));
    // The `check-ignore` of the project folder itself, after its `ls-files` refused to run.
    let (program, env, _bin) = stalling("browse-ignored-cancel-fallback", "ls-files");
    let make = || Ok(GitEnv::new(program.clone(), env.clone()));
    let git = Git::new(&make).with_ceiling(None);
    assert!(cancel_soon(&|cancel| {
        live::search_files(&canonical, "a", false, &git, cancel).map(|_| ())
    }));
}

#[test]
fn a_project_below_the_repository_root_is_judged_by_the_whole_repository() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "browse-ignored-below-root",
        &[
            (b".gitignore", b"*.log\n"),
            (b"pkg/.gitignore", b"*.tmp\n"),
            (b"pkg/plain.txt", b"plain\n"),
        ],
    );
    for name in [
        "pkg/a.log",
        "pkg/a.tmp",
        "pkg/gen/b.log",
        "pkg/gen/b.txt",
        "outside.log",
    ] {
        let path = repo.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }
    let project = fixture.project("p", &repo.join("pkg"));
    assert_eq!(fixture.ignored_in(&project, ""), ["a.log", "a.tmp"]);
    assert_eq!(fixture.ignored_in(&project, "gen"), ["b.log"]);
    assert_eq!(
        fixture.search(&project, "", false),
        [".gitignore", "gen/b.txt", "plain.txt"]
    );
    assert_eq!(
        fixture.search_marked(&project, "", true).1,
        ["a.log", "a.tmp", "gen/b.log"]
    );
}

#[test]
fn a_nested_repository_is_judged_by_its_own_rules_in_the_listing_and_in_the_search() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "browse-ignored-nested",
        &[(b".gitignore", b"*.log\n"), (b"top.txt", b"t\n")],
    );
    std::fs::write(repo.join("top.log"), b"x").unwrap();
    // A clone of its own inside the project, which ignores something the outer one does not and
    // keeps what the outer one ignores.
    let inner = repo.join("inner");
    std::fs::create_dir_all(inner.join("cache")).unwrap();
    git(&inner, &["init", "-q"]);
    std::fs::write(inner.join(".gitignore"), b"cache/\n").unwrap();
    std::fs::write(inner.join("cache/data.txt"), b"x").unwrap();
    std::fs::write(inner.join("inner.log"), b"x").unwrap();
    std::fs::write(inner.join("inner.txt"), b"x").unwrap();
    let project = fixture.project("p", &repo);
    assert_eq!(fixture.ignored_in(&project, "inner"), ["cache"]);
    assert_eq!(
        fixture.search(&project, "", false),
        [
            ".gitignore",
            "inner/.gitignore",
            "inner/inner.log",
            "inner/inner.txt",
            "top.txt"
        ],
        "the outer rules do not reach into it and the inner ones do"
    );
    assert_eq!(
        fixture.search_marked(&project, "data", true).1,
        ["inner/cache/data.txt"]
    );
}

#[test]
fn a_submodule_is_judged_by_its_own_rules_in_the_search_too() {
    let fixture = Fixture::new();
    let library = repo_with(
        "browse-ignored-library",
        &[(b".gitignore", b"generated/\n"), (b"lib.txt", b"l\n")],
    );
    std::fs::create_dir(library.join("generated")).unwrap();
    let repo = repo_with("browse-ignored-submodule", &[(b"top.txt", b"t\n")]);
    git(
        &repo,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            library.to_str().unwrap(),
            "vendor",
        ],
    );
    git(&repo, &["commit", "-q", "-m", "add the submodule"]);
    std::fs::create_dir_all(repo.join("vendor/generated")).unwrap();
    std::fs::write(repo.join("vendor/generated/out.txt"), b"x").unwrap();
    let project = fixture.project("p", &repo);
    assert_eq!(fixture.ignored_in(&project, "vendor"), ["generated"]);
    assert_eq!(
        fixture.search(&project, ".txt", false),
        ["top.txt", "vendor/lib.txt"]
    );
    assert_eq!(
        fixture.search_marked(&project, ".txt", true).1,
        ["vendor/generated/out.txt"]
    );
}

#[test]
fn a_name_that_looks_like_a_pattern_is_asked_about_as_the_file_it_is() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "browse-ignored-glob-names",
        &[
            (b".gitignore", b"*.txt\n!keep.txt\n"),
            (b"x.txt", b"tracked\n"),
        ],
    );
    git(&repo, &["add", "-f", "x.txt"]);
    git(&repo, &["commit", "-q", "-m", "track a file that matches"]);
    for name in ["*.txt", "[a].txt", "a?b.txt", "back\\slash.txt", "keep.txt"] {
        std::fs::write(repo.join(name), b"x").unwrap();
    }
    let project = fixture.project("p", &repo);
    assert_eq!(
        fixture.ignored_in(&project, ""),
        ["*.txt", "[a].txt", "a?b.txt", "back\\slash.txt"],
        "an untracked file named like a pattern is not answered for by the tracked x.txt, and a \
         pattern that un-ignores a file leaves it not ignored"
    );
}

#[test]
fn a_name_in_either_unicode_form_is_found_among_the_ignored() {
    let fixture = Fixture::new();
    let repo = repo_with("browse-ignored-unicode", &[(b".gitignore", b"*.log\n")]);
    // A decomposed name, as macOS applications write them, and a precomposed one.
    let decomposed = "cafe\u{301}.log";
    let precomposed = "caf\u{e9}-2.log";
    std::fs::write(repo.join(decomposed), b"x").unwrap();
    std::fs::write(repo.join(precomposed), b"x").unwrap();
    std::fs::write(repo.join("cafe\u{301}.txt"), b"x").unwrap();
    let project = fixture.project("p", &repo);
    let mut ignored = fixture.ignored_in(&project, "");
    ignored.sort();
    let mut expected = vec![decomposed.to_string(), precomposed.to_string()];
    expected.sort();
    assert_eq!(ignored, expected);
    assert_eq!(
        fixture.search(&project, "caf", false),
        ["cafe\u{301}.txt".to_string()]
    );
}

#[test]
fn a_project_that_is_itself_ignored_is_ignored_throughout() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "browse-ignored-itself",
        &[(b".gitignore", b"build/\n"), (b"top.txt", b"t\n")],
    );
    for name in ["build/one.txt", "build/deep/two.txt"] {
        let path = repo.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }
    // At the ignored folder and below it, `git ls-files` prints `./` and refuses to run.
    for (root, listed, found) in [
        (
            "build",
            vec!["deep", "one.txt"],
            vec!["deep/two.txt", "one.txt"],
        ),
        ("build/deep", vec!["two.txt"], vec!["two.txt"]),
    ] {
        let project = fixture.project(&root.replace('/', "-"), &repo.join(root));
        assert_eq!(fixture.ignored_in(&project, ""), listed, "{root}");
        assert!(fixture.search(&project, "", false).is_empty(), "{root}");
        let found: Vec<String> = found.into_iter().map(str::to_string).collect();
        assert_eq!(
            fixture.search_marked(&project, "", true),
            (found.clone(), found)
        );
    }
}

#[test]
fn the_ignored_entries_of_a_folder_are_remembered_until_something_that_decides_them_changes() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "browse-ignored-remembered",
        &[(b".gitignore", b"*.log\n"), (b"a.txt", b"a\n")],
    );
    std::fs::write(repo.join("a.log"), b"x").unwrap();
    std::fs::write(repo.join("c.txt"), b"x").unwrap();
    let project = fixture.project("p", &repo);
    assert_eq!(fixture.ignored_in(&project, ""), ["a.log"]);

    // A `git` that can no longer read the repository would leave everything unmarked, so a
    // listing that still marks the entry did not ask it.
    let head = std::fs::read(repo.join(".git/HEAD")).unwrap();
    std::fs::write(repo.join(".git/HEAD"), b"garbage\n").unwrap();
    assert_eq!(fixture.ignored_in(&project, ""), ["a.log"], "remembered");

    // Changing the `.gitignore` is noticed, so `git` is asked again, and fails.
    std::fs::write(repo.join(".gitignore"), b"*.log\n*.txt\n").unwrap();
    assert!(fixture.ignored_in(&project, "").is_empty(), "asked again");

    // With `git` working again, the new rules are the ones found.
    std::fs::write(repo.join(".git/HEAD"), head).unwrap();
    assert_eq!(fixture.ignored_in(&project, ""), ["a.log", "c.txt"]);
    // A new entry is asked about even though no source changed.
    std::fs::write(repo.join("b.log"), b"x").unwrap();
    assert_eq!(
        fixture.ignored_in(&project, ""),
        ["a.log", "b.log", "c.txt"]
    );
}

#[test]
fn names_that_look_like_patterns_are_judged_by_the_rules_as_the_files_they_are() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "browse-ignored-literal-rules",
        &[
            (b".gitignore", b"a?b.md\n\\*.md\nlit\\\\b\n*.keep\n"),
            (b"plain.md", b"p\n"),
        ],
    );
    // The rules name these files literally: `\*.md` is a star, `lit\\b` a backslash. `*.keep` is
    // tracked below, so it is not ignored however the rules read it.
    for name in ["a?b.md", "*.md", "lit\\b", "*.keep", "axb.md"] {
        std::fs::write(repo.join(name), b"x").unwrap();
    }
    git(&repo, &["add", "-f", "--", "*.keep"]);
    git(
        &repo,
        &["commit", "-q", "-m", "track a file named like a pattern"],
    );
    let project = fixture.project("p", &repo);
    assert_eq!(
        fixture.ignored_in(&project, ""),
        ["*.md", "a?b.md", "axb.md", "lit\\b"],
        "axb.md is ignored too, as the rule a?b.md reads"
    );
}

#[test]
fn a_pattern_named_folder_holding_a_tracked_file_is_not_ignored() {
    let fixture = Fixture::new();
    let repo = repo_with(
        "browse-ignored-tracked-in-glob-folder",
        &[(b".gitignore", b"[[]id]/\n"), (b"top.txt", b"t\n")],
    );
    std::fs::create_dir_all(repo.join("[id]")).unwrap();
    std::fs::write(repo.join("[id]/page.js"), b"x").unwrap();
    std::fs::create_dir_all(repo.join("[slug]")).unwrap();
    std::fs::write(repo.join("[slug]/page.js"), b"x").unwrap();
    std::fs::write(repo.join(".gitignore"), b"[[]id]/\n[[]slug]/\n").unwrap();
    git(&repo, &["add", "-f", "--", "[[]id]/page.js"]);
    git(
        &repo,
        &["commit", "-q", "-m", "track a file in an ignored folder"],
    );
    let project = fixture.project("p", &repo);
    assert_eq!(
        fixture.ignored_in(&project, ""),
        ["[slug]"],
        "[id] holds a tracked file, [slug] does not"
    );
}
