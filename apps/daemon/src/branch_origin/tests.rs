use std::collections::BTreeSet;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use super::git_hooks::{injected, interpreter, render, write_dir, Rendering, BUILT_IN_HOOKS};
use super::*;
use crate::browse::compare;
use crate::browse::git::GitEnv;
use crate::protocol::{
    now_millis, Agent, Console, Origin, Project, ProjectAccount, ProjectSource, Role, SessionStatus,
};
use crate::store::LOCAL_HOST_ID;
use crate::test_support::{
    app_state, fixture_git, git, isolated_git_env, repo_with, ScratchDir, ScratchFile, PATIENCE,
    SYSTEM_GIT,
};

/// The dispatchers every test here runs git with, written once per test binary under a fixed
/// name, as a fresh executable per test would pay macOS's first-exec cost each time. Their report
/// goes to a stand-in for `octoboardd git-ref` that appends what it was started with — its
/// directory, `GIT_DIR` and arguments — to the file `OCTOBOARD_TEST_REPORTS` names.
fn dispatchers() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| dispatchers_for(&system_exec_path(), &[]))
}

/// What `git --exec-path` says for the git the tests run, which is what git exports to its hooks.
fn system_exec_path() -> String {
    let output = Command::new(SYSTEM_GIT)
        .arg("--exec-path")
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// Dispatchers written as for a git with `exec_path` that reads `system_files`.
fn dispatchers_for(exec_path: &str, system_files: &[PathBuf]) -> PathBuf {
    let state = std::env::temp_dir().join("octoboardd-test-githooks/state");
    std::fs::create_dir_all(&state).unwrap();
    dispatchers_with(exec_path, system_files, &state)
}

/// [`dispatchers_for`] with a state directory of the caller's.
fn dispatchers_with(exec_path: &str, system_files: &[PathBuf], state: &Path) -> PathBuf {
    let temp = std::env::temp_dir();
    let stand_in = installed(
        "octoboardd-test-git-ref",
        "#!/bin/sh\nprintf '%s\\t%s\\t%s\\n' \"$PWD\" \"${GIT_DIR-}\" \"$*\" >>\"$OCTOBOARD_TEST_REPORTS\"\n",
        0o755,
    );
    let root = temp.join("octoboardd-test-githooks");
    let names: BTreeSet<String> = BUILT_IN_HOOKS.iter().map(|n| n.to_string()).collect();
    let script = render(&Rendering {
        interpreter: interpreter(),
        self_exe: &stand_in.to_string_lossy(),
        exec_path,
        state,
        system_files,
    });
    write_dir(&root, &names, &script).unwrap()
}

/// A file of `body` under a fixed name in the temporary directory, written by renaming one of this
/// process's own into place: a test binary of another worktree may be reading or running it.
fn installed(name: &str, body: &str, mode: u32) -> PathBuf {
    let temp = std::env::temp_dir();
    let path = temp.join(name);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(body) {
        let own = temp.join(format!("{name}.{}", std::process::id()));
        std::fs::write(&own, body).unwrap();
        std::fs::set_permissions(&own, std::fs::Permissions::from_mode(mode)).unwrap();
        std::fs::rename(&own, &path).unwrap();
    }
    path
}

/// A session of `project` that started a minute ago and is still running.
fn session_record(id: &str, project: &str) -> Session {
    Session {
        id: id.to_string(),
        agent: Agent::Claude,
        agent_session_id: None,
        console_id: "console".to_string(),
        project_id: Some(project.to_string()),
        host_id: LOCAL_HOST_ID.to_string(),
        role: Role::Project,
        origin: Origin::User,
        title: format!("Session {id}"),
        status: SessionStatus::Working,
        terminal_title: None,
        record_titles: Default::default(),
        background_tasks: Vec::new(),
        switching_account: false,
        default_effort: None,
        context: None,
        observed_model: None,
        observed_effort: None,
        model: None,
        effort: None,
        has_conversation: true,
        bound_to: None,
        lead: false,
        colour: None,
        ordinal: None,
        account_id: None,
        config_dir: None,
        pinned: false,
        locked: false,
        started_at: now_millis() - 60_000,
        ended_at: None,
    }
}

/// A project of the fixture's console at `path`.
fn project(id: &str, path: &Path) -> Project {
    Project {
        id: id.to_string(),
        console_id: "console".to_string(),
        host_id: LOCAL_HOST_ID.to_string(),
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
    }
}

struct Fixture {
    state: Arc<AppState>,
    repo: ScratchDir,
    common_dir: PathBuf,
    reports: ScratchFile,
    dir: ScratchDir,
}

impl Fixture {
    fn new() -> Self {
        let (state, dir) = app_state("branch-origin");
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
        let repo = repo_with("branch-origin-repo", &[(b"a", b"a")]);
        state
            .store
            .insert_project(&project("project", &repo))
            .unwrap();
        let common_dir = repository_of(&repo).unwrap();
        let fixture = Self {
            state,
            repo,
            common_dir,
            reports: ScratchFile::new("branch-origin-reports", "reports"),
            dir,
        };
        fixture.session("a");
        fixture.session("b");
        fixture
    }

    fn session(&self, id: &str) {
        self.state
            .store
            .insert_session(&session_record(id, "project"))
            .unwrap();
    }

    /// `git <args>` in `dir` as the agent of `session` runs it: with the variables a project
    /// session is launched with, after any `GIT_CONFIG_*` entries of the user's own in `user`.
    fn session_git(&self, session: &str, dir: &Path, user: &[(&str, &str)]) -> Command {
        self.session_git_with(dispatchers(), session, dir, user)
    }

    /// [`Fixture::session_git`] pointed at `hooks` rather than the usual dispatchers.
    fn session_git_with(
        &self,
        hooks: &Path,
        session: &str,
        dir: &Path,
        user: &[(&str, &str)],
    ) -> Command {
        let slot = user
            .iter()
            .find(|(key, _)| *key == "GIT_CONFIG_COUNT")
            .map_or(0, |(_, count)| count.parse().unwrap());
        let mut command = fixture_git(dir);
        command
            .envs(user.iter().copied())
            .envs(injected(slot, hooks, session, 1))
            .env("OCTOBOARD_TEST_REPORTS", &*self.reports);
        command
    }

    /// Runs [`Fixture::session_git`] and fails the test on a non-zero exit.
    fn as_session(&self, session: &str, args: &[&str]) {
        let output = self
            .session_git(session, &self.repo, &[])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// The reports the stand-in has received once there are `count` of them.
    fn reports(&self, count: usize) -> Vec<(String, RefReport)> {
        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            let text = std::fs::read_to_string(&*self.reports).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            if lines.len() >= count {
                return lines.into_iter().map(parse_report).collect();
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{} of {count} reports arrived",
                lines.len()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Hands every report received, once there are `count`, to the daemon.
    fn deliver(&self, count: usize) {
        for (session, report) in self.reports(count) {
            ref_created(&self.state, &session, &report);
        }
    }

    fn env(&self) -> GitEnv {
        GitEnv::new(SYSTEM_GIT.to_string(), isolated_git_env())
    }

    fn list(&self) -> BranchList {
        compare::branches_in(
            &self.env(),
            &self.common_dir,
            &self.common_dir,
            true,
            &AtomicBool::new(false),
        )
        .unwrap()
    }

    fn rows(&self) -> Vec<BranchOrigin> {
        self.state
            .store
            .branch_origins(&repo_key(&self.common_dir))
            .unwrap()
    }

    fn responsible(&self, branch: &str) -> Option<String> {
        self.rows()
            .into_iter()
            .find(|row| row.branch == branch)
            .map(|row| row.responsible_session)
    }

    fn reconcile_full(&self) -> Vec<BranchOrigin> {
        reconcile(&self.state, &self.common_dir, &self.list()).unwrap()
    }
}

/// One line of the stand-in's: the session it reported for and the report `octoboardd git-ref`
/// would have posted.
fn parse_report(line: &str) -> (String, RefReport) {
    let mut fields = line.splitn(3, '\t');
    let cwd = PathBuf::from(fields.next().unwrap());
    let git_dir = fields.next().unwrap();
    let args: Vec<&str> = fields.next().unwrap().split(' ').collect();
    let session = args[args.iter().position(|a| *a == "--session").unwrap() + 1].to_string();
    let refs: Vec<OsString> = args[args.iter().position(|a| *a == "--").unwrap() + 1..]
        .iter()
        .map(OsString::from)
        .collect();
    let git_dir = (!git_dir.is_empty()).then(|| std::ffi::OsStr::new(git_dir));
    let report = crate::git_ref_mode::report_of(&cwd, git_dir, &refs, now_millis() / 1000);
    (session, report)
}

/// Every line of a git trace2 event log naming a `git rev-parse` started. Traced rather than caught
/// by a stand-in `git` on `PATH`: inside a hook git puts its own exec path first on `PATH`, so a
/// stand-in is never reached.
fn rev_parses(trace: &Path) -> usize {
    std::fs::read_to_string(trace)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("\"event\":\"start\"") && line.contains("\"rev-parse\""))
        .count()
}

fn write_hook(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn succeeds(output: Output) -> bool {
    output.status.success()
}

// -- attribution --------------------------------------------------------------

/// Two sessions working in one repository at the same moment, which no timing could tell apart.
#[test]
fn branches_two_sessions_create_at_once_go_each_to_its_own() {
    let fixture = Fixture::new();
    std::thread::scope(|scope| {
        for session in ["a", "b"] {
            let fixture = &fixture;
            scope.spawn(move || {
                for n in 0..4 {
                    fixture.as_session(session, &["branch", &format!("{session}-{n}")]);
                }
            });
        }
    });

    fixture.deliver(8);

    let rows = fixture.rows();
    assert_eq!(rows.len(), 8);
    for row in rows {
        assert_eq!(
            Some(row.responsible_session.as_str()),
            row.branch.split('-').next()
        );
        assert_eq!(
            row.responsible_title,
            format!("Session {}", row.responsible_session)
        );
    }
}

/// A clone has the same commits, and a branch of the same name made there at the same moment would
/// match the project's own reflog: only the repository the report names tells them apart.
#[test]
fn a_report_about_a_repository_the_sessions_project_is_not_in_is_ignored() {
    let fixture = Fixture::new();
    let clone = ScratchDir::new("branch-origin-clone");
    git(
        &fixture.repo,
        &[
            "clone",
            "-q",
            &fixture.repo.to_string_lossy(),
            &clone.to_string_lossy(),
        ],
    );
    let output = fixture
        .session_git("a", &clone, &[])
        .args(["branch", "elsewhere"])
        .output()
        .unwrap();
    assert!(succeeds(output));
    git(&fixture.repo, &["branch", "elsewhere"]);

    fixture.deliver(1);

    assert!(fixture.rows().is_empty());
    let key = repo_key(&repository_of(&clone).unwrap());
    assert!(fixture.state.store.branch_origins(&key).unwrap().is_empty());
}

#[test]
fn a_created_branch_is_its_creating_sessions_to_work_on_from_the_time_of_its_creation() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "topic"]);
    fixture.deliver(1);

    let row = fixture.rows().pop().unwrap();
    assert_eq!(row.responsible_session, "a");
    assert_eq!(row.responsible_since, row.created_at);
    assert_eq!(row.responsible_title, "Session a");
}

#[test]
fn a_handover_moves_the_responsible_session_only_from_the_expected_one() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "mine"]);
    fixture.as_session("b", &["branch", "theirs"]);
    fixture.deliver(2);
    let before = fixture.rows();
    let names = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();

    let moved = hand_over(
        &fixture.state,
        &fixture.common_dir,
        &names(&["mine", "theirs", "absent"]),
        "a",
        "c",
        "Session c",
    )
    .unwrap();

    assert_eq!(moved.moved, names(&["mine"]));
    assert_eq!(moved.unmoved, names(&["theirs", "absent"]));
    for row in fixture.rows() {
        let old = before.iter().find(|o| o.branch == row.branch).unwrap();
        if row.branch == "mine" {
            assert_eq!(row.responsible_session, "c");
            assert_eq!(row.responsible_title, "Session c");
            assert!(row.responsible_since > old.responsible_since);
        } else {
            assert_eq!(row.responsible_session, old.responsible_session);
            assert_eq!(row.responsible_title, old.responsible_title);
            assert_eq!(row.responsible_since, old.responsible_since);
        }
    }
    // The branch is no longer `a`'s to give: a second handover from it moves nothing.
    let again = hand_over(
        &fixture.state,
        &fixture.common_dir,
        &names(&["mine"]),
        "a",
        "d",
        "Session d",
    )
    .unwrap();
    assert!(again.moved.is_empty());
}

#[test]
fn a_handover_keeps_the_first_characters_of_a_long_title() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "mine"]);
    fixture.deliver(1);
    let title = format!("é{}", "x".repeat(100));

    hand_over(
        &fixture.state,
        &fixture.common_dir,
        &["mine".to_string()],
        "a",
        "c",
        &title,
    )
    .unwrap();

    let row = fixture.rows().pop().unwrap();
    assert_eq!(
        row.responsible_title,
        title.chars().take(80).collect::<String>()
    );
    assert_eq!(row.responsible_title.chars().count(), 80);
}

#[test]
fn a_renamed_branch_keeps_its_responsible_session_and_a_copy_gets_none() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "old"]);
    fixture.deliver(1);
    hand_over(
        &fixture.state,
        &fixture.common_dir,
        &["old".to_string()],
        "a",
        "c",
        "Session c",
    )
    .unwrap();
    git(&fixture.repo, &["branch", "-m", "old", "new"]);

    let rows = fixture.reconcile_full();
    assert_eq!(fixture.responsible("old"), None);
    assert_eq!(fixture.responsible("new").as_deref(), Some("c"));
    let row = rows.iter().find(|row| row.branch == "new").unwrap();
    assert_eq!(row.responsible_title, "Session c");

    // git reports no transaction for a copy; a branch made after it shows the report would have
    // arrived by then.
    fixture.as_session("b", &["branch", "-c", "new", "copy"]);
    fixture.as_session("b", &["branch", "after"]);
    fixture.deliver(2);
    assert_eq!(fixture.responsible("copy"), None);
    assert_eq!(fixture.responsible("after").as_deref(), Some("b"));
    assert_eq!(fixture.reports(2).len(), 2);
}

#[test]
fn a_branch_made_again_under_the_same_name_does_not_inherit_the_attribution() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "topic"]);
    fixture.deliver(1);
    assert_eq!(fixture.responsible("topic").as_deref(), Some("a"));

    // A different tip, so the new reflog's first entry cannot equal the old one even within the
    // same second.
    git(&fixture.repo, &["branch", "-q", "-D", "topic"]);
    git(
        &fixture.repo,
        &["commit", "-q", "--allow-empty", "-m", "next"],
    );
    git(&fixture.repo, &["branch", "topic"]);
    fixture.reconcile_full();

    assert_eq!(fixture.responsible("topic"), None);
}

/// A report that comes after its branch was deleted and made again by someone else is about a
/// branch that is no longer there.
#[test]
fn a_report_about_a_branch_since_made_again_is_ignored() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "topic"]);
    let reports = fixture.reports(1);
    git(&fixture.repo, &["branch", "-q", "-D", "topic"]);
    git(
        &fixture.repo,
        &["commit", "-q", "--allow-empty", "-m", "next"],
    );
    git(&fixture.repo, &["branch", "topic"]);

    for (session, report) in reports {
        ref_created(&fixture.state, &session, &report);
    }

    assert!(fixture.rows().is_empty());
}

/// `git branch -f`, `checkout -B` and `update-ref` without an old value name none for a branch
/// that exists, just as a creation does.
#[test]
fn a_forced_update_of_an_existing_branch_is_not_a_creation() {
    let fixture = Fixture::new();
    git(
        &fixture.repo,
        &["commit", "-q", "--allow-empty", "-m", "next"],
    );
    git(&fixture.repo, &["branch", "theirs"]);
    fixture.as_session("a", &["branch", "-f", "theirs", "HEAD~"]);
    fixture.as_session("a", &["branch", "mine"]);

    fixture.deliver(1);

    assert_eq!(fixture.responsible("theirs"), None);
    assert_eq!(fixture.responsible("mine").as_deref(), Some("a"));
    assert_eq!(
        fixture.reports(1).len(),
        1,
        "the forced update was never reported"
    );
}

/// git reports no transaction for a rename or a copy today; should one ever be reported, the new
/// name's reflog says where the branch came from. And a branch whose reflog `git gc` has emptied
/// begins it again with whatever moves it next, which is no creation.
#[test]
fn a_branch_whose_reflog_does_not_begin_with_its_creation_has_no_birth() {
    let fixture = Fixture::new();
    let now = now_millis() / 1000;
    let rev = |rev: &str| {
        let out = String::from_utf8(git(&fixture.repo, &["rev-parse", rev])).unwrap();
        out.trim().to_string()
    };
    git(
        &fixture.repo,
        &["commit", "-q", "--allow-empty", "-m", "next"],
    );
    git(&fixture.repo, &["branch", "made"]);
    git(&fixture.repo, &["branch", "-m", "made", "renamed"]);
    git(&fixture.repo, &["branch", "-c", "renamed", "copied"]);
    git(&fixture.repo, &["branch", "expired"]);
    git(
        &fixture.repo,
        &["reflog", "expire", "--expire=all", "refs/heads/expired"],
    );
    git(&fixture.repo, &["branch", "-f", "expired", "HEAD~"]);
    git(&fixture.repo, &["branch", "fresh"]);

    let birth = |branch: &str, at: &str| birth_of(&fixture.common_dir, branch, &rev(at), now);
    assert_eq!(birth("renamed", "HEAD"), None);
    assert_eq!(birth("copied", "HEAD"), None);
    assert_eq!(birth("expired", "HEAD~"), None);
    assert!(birth("fresh", "HEAD").is_some());
}

// -- the dispatcher -----------------------------------------------------------

/// A user's `reference-transaction` hook can refuse a transaction while it is being prepared.
#[test]
fn a_users_reference_transaction_hook_still_aborts_and_nothing_is_reported() {
    let fixture = Fixture::new();
    let hook = fixture.common_dir.join("hooks/reference-transaction");
    let seen = fixture.dir.join("seen");
    write_hook(
        &hook,
        &format!(
            "[ \"$1\" = prepared ] && grep -q refused && exit 1\n\
             [ \"$1\" = committed ] && cat >>'{}'\nexit 0",
            seen.display()
        ),
    );

    let refused = fixture
        .session_git("a", &fixture.repo, &[])
        .args(["branch", "refused"])
        .output()
        .unwrap();
    assert!(!succeeds(refused), "the user's hook refused it");
    fixture.as_session("a", &["branch", "allowed"]);
    fixture.deliver(1);

    assert_eq!(fixture.responsible("refused"), None);
    assert_eq!(fixture.responsible("allowed").as_deref(), Some("a"));
    assert!(git(&fixture.repo, &["branch", "--list", "refused"]).is_empty());
    assert_eq!(
        fixture.reports(1).len(),
        1,
        "the refused branch was never reported"
    );
    let seen = std::fs::read_to_string(&seen).unwrap();
    assert!(seen.contains(" refs/heads/allowed\n"), "{seen}");
}

/// A sandbox may refuse every write outside the workspace: the dispatcher then reports nothing,
/// says nothing, and leaves the user's hook to run as it would.
#[test]
fn a_state_directory_that_cannot_be_written_costs_only_the_report() {
    let fixture = Fixture::new();
    let seen = fixture.dir.join("seen");
    write_hook(
        &fixture.common_dir.join("hooks/reference-transaction"),
        &format!("echo \"$1\" >>'{}'\ncat >/dev/null", seen.display()),
    );
    let hooks = dispatchers_with(
        &system_exec_path(),
        &[],
        Path::new("/nonexistent/octoboard-state"),
    );

    let output = fixture
        .session_git_with(&hooks, "a", &fixture.repo, &[])
        .args(["branch", "sandboxed"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert_eq!(
        std::fs::read_to_string(&seen).unwrap(),
        "prepared\ncommitted\n"
    );
}

/// A transaction of hundreds of references is spooled rather than read line by line, and the
/// user's hook still gets every update.
#[test]
fn a_long_transaction_is_reported_and_handed_to_the_users_hook_whole() {
    let fixture = Fixture::new();
    let seen = fixture.dir.join("seen");
    write_hook(
        &fixture.common_dir.join("hooks/reference-transaction"),
        &format!(
            "[ \"$1\" = committed ] && cat >>'{}'\nexit 0",
            seen.display()
        ),
    );
    let head = String::from_utf8(git(&fixture.repo, &["rev-parse", "HEAD"])).unwrap();
    let updates: String = (0..300)
        .map(|n| format!("create refs/heads/bulk-{n:03} {}\n", head.trim()))
        .collect();
    let mut child = fixture
        .session_git("a", &fixture.repo, &[])
        .args(["update-ref", "--stdin"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(&mut child.stdin.take().unwrap(), updates.as_bytes()).unwrap();
    assert!(child.wait().unwrap().success());

    fixture.deliver(1);

    assert_eq!(fixture.rows().len(), 300);
    assert_eq!(std::fs::read_to_string(&seen).unwrap().lines().count(), 300);
}

/// Husky and its like keep their hooks under the worktree and point `core.hooksPath` at them by a
/// relative path; a user may also export an entry of their own.
#[test]
fn a_hooks_path_of_the_users_own_still_runs_its_hooks() {
    let fixture = Fixture::new();
    let marker = fixture.repo.join("ran");
    write_hook(
        &fixture.repo.join(".husky/_/pre-commit"),
        &format!("echo husky >>'{}'", marker.display()),
    );
    git(&fixture.repo, &["config", "core.hooksPath", ".husky/_"]);
    fixture.as_session("a", &["commit", "-q", "--allow-empty", "-m", "one"]);
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "husky\n");

    git(&fixture.repo, &["config", "--unset", "core.hooksPath"]);
    let exported = fixture.dir.join("exported");
    write_hook(
        &exported.join("pre-commit"),
        &format!("echo exported >>'{}'", marker.display()),
    );
    let exported = exported.to_string_lossy().into_owned();
    let user = [
        ("GIT_CONFIG_COUNT", "1"),
        ("GIT_CONFIG_KEY_0", "core.hooksPath"),
        ("GIT_CONFIG_VALUE_0", exported.as_str()),
    ];
    let output = fixture
        .session_git("a", &fixture.repo, &user)
        .args(["commit", "-q", "--allow-empty", "-m", "two"])
        .output()
        .unwrap();
    assert!(succeeds(output));
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap(),
        "husky\nexported\n"
    );
}

/// Every hook git looks for starts a dispatcher, several per commit, so one with nothing of the
/// user's to run must not start a `git` of its own to find that out — and must still find a hook
/// the user does have.
#[test]
fn the_fast_path_starts_no_git_and_still_runs_a_hook_the_user_has() {
    let fixture = Fixture::new();
    let trace = fixture.dir.join("trace");
    let commit = |message: &str| {
        let output = fixture
            .session_git("a", &fixture.repo, &[])
            .env("GIT_TRACE2_EVENT", &trace)
            .args(["commit", "-q", "--allow-empty", "-m", message])
            .output()
            .unwrap();
        assert!(succeeds(output));
    };

    commit("none");
    assert_eq!(rev_parses(&trace), 0, "no hook of the user's");

    let marker = fixture.repo.join("ran");
    write_hook(
        &fixture.common_dir.join("hooks/pre-commit"),
        &format!("echo ran >>'{}'", marker.display()),
    );
    commit("own");
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "ran\n");
    assert_eq!(rev_parses(&trace), 0, "found without git");

    // The control: configuration that could move the hooks has git asked.
    git(&fixture.repo, &["config", "core.hooksPath", ".git/hooks"]);
    commit("asked");
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "ran\nran\n");
    assert!(rev_parses(&trace) > 0);
}

/// Each place a `core.hooksPath` could come from that the dispatcher cannot read for itself sends
/// it to git, the slow path, rather than to the repository's own hooks directory.
#[test]
fn anything_that_could_move_the_hooks_has_git_asked() {
    let system = installed(
        "octoboardd-test-system-gitconfig",
        "[core]\n\thooksPath = .git/hooks\n",
        0o644,
    );
    let other_build = dispatchers_for("/elsewhere/libexec/git-core", &[]);
    let with_system = dispatchers_for(&system_exec_path(), std::slice::from_ref(&system));
    type Setup = fn(&Fixture, &Path, &Path, &mut Command);
    let cases: [(&str, Option<&Path>, Setup); 9] = [
        ("an exported entry of the user's", None, |_, _, hooks, c| {
            // The user's entry first, the injected one after it, as a launch lays them out.
            c.env("GIT_CONFIG_COUNT", "2")
                .env("GIT_CONFIG_KEY_0", "core.hooksPath")
                .env("GIT_CONFIG_VALUE_0", ".git/hooks")
                .env("GIT_CONFIG_KEY_1", "core.hooksPath")
                .env("GIT_CONFIG_VALUE_1", hooks);
        }),
        ("git -c", None, |_, _, _, c| {
            c.args(["-c", "include.path=/dev/null"]);
        }),
        ("GIT_CONFIG_GLOBAL", None, |_, moved, _, c| {
            c.env("GIT_CONFIG_GLOBAL", moved);
        }),
        ("~/.gitconfig", None, |f, moved, _, c| {
            std::fs::copy(moved, f.dir.join(".gitconfig")).unwrap();
            c.env_remove("GIT_CONFIG_GLOBAL").env("HOME", &*f.dir);
        }),
        (
            "an include in the repository's configuration",
            None,
            |f, _, _, _| {
                git(&f.repo, &["config", "include.path", "/dev/null"]);
            },
        ),
        ("config.worktree", None, |f, moved, _, _| {
            std::fs::copy(moved, f.common_dir.join("config.worktree")).unwrap();
        }),
        ("GIT_CONFIG_SYSTEM", None, |_, moved, _, c| {
            c.env_remove("GIT_CONFIG_NOSYSTEM")
                .env("GIT_CONFIG_SYSTEM", moved);
        }),
        (
            "a system file this git build reads",
            Some(&with_system),
            |_, _, _, _| {},
        ),
        ("another git build", Some(&other_build), |_, _, _, _| {}),
    ];
    for (source, hooks, setup) in cases {
        let fixture = Fixture::new();
        let moved = fixture.dir.join("moved");
        std::fs::write(&moved, "[core]\n\thooksPath = .git/hooks\n").unwrap();
        let trace = fixture.dir.join("trace");
        let hooks = hooks.unwrap_or(dispatchers());
        let mut command = fixture.session_git_with(hooks, "a", &fixture.repo, &[]);
        setup(&fixture, &moved, hooks, &mut command);
        let output = command
            .env("GIT_TRACE2_EVENT", &trace)
            .args(["commit", "-q", "--allow-empty", "-m", "m"])
            .output()
            .unwrap();
        assert!(succeeds(output), "{source}");
        assert!(rev_parses(&trace) > 0, "{source}");
    }
}

/// A push into a non-bare repository's checked-out branch updates its worktree unless a
/// push-to-checkout hook does, and the dispatcher's presence must not count as one. git clears
/// `GIT_CONFIG_*` for the receiving end of a push it makes itself, so the dispatcher only meets
/// this behind a server a session starts (`git daemon`, `git http-backend`); it is run here the
/// way `receive-pack` runs it.
#[test]
fn a_push_to_checkout_with_no_hook_of_the_users_updates_the_worktree() {
    let fixture = Fixture::new();
    let theirs = ScratchDir::new("branch-origin-push-target");
    git(
        &fixture.repo,
        &[
            "clone",
            "-q",
            &fixture.repo.to_string_lossy(),
            &theirs.to_string_lossy(),
        ],
    );
    std::fs::write(fixture.repo.join("pushed"), "pushed").unwrap();
    git(&fixture.repo, &["add", "pushed"]);
    git(&fixture.repo, &["commit", "-q", "-m", "push"]);
    let pushed = String::from_utf8(git(&fixture.repo, &["rev-parse", "HEAD"])).unwrap();
    git(&theirs, &["fetch", "-q", "origin"]);

    let git_dir = theirs.join(".git");
    let output = Command::new(dispatchers().join("push-to-checkout"))
        .arg(pushed.trim())
        .current_dir(&git_dir)
        .env_clear()
        .envs(isolated_git_env())
        .envs(injected(0, dispatchers(), "a", 1))
        .env("GIT_DIR", &git_dir)
        .env("GIT_WORK_TREE", &*theirs)
        .output()
        .unwrap();
    assert!(succeeds(output));

    assert_eq!(
        std::fs::read_to_string(theirs.join("pushed")).unwrap(),
        "pushed"
    );
}

// -- the record's lifetime ----------------------------------------------------

#[test]
fn a_row_goes_with_its_branch_but_only_on_a_full_listing() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "gone"]);
    fixture.deliver(1);
    git(&fixture.repo, &["branch", "-q", "-D", "gone"]);

    let cut = BranchList {
        complete: false,
        ..fixture.list()
    };
    reconcile(&fixture.state, &fixture.common_dir, &cut).unwrap();
    assert!(
        fixture.responsible("gone").is_some(),
        "a cut listing proves nothing"
    );

    fixture.reconcile_full();
    assert!(fixture.responsible("gone").is_none());
}

#[test]
fn a_listing_older_than_the_last_record_does_not_remove_it() {
    let fixture = Fixture::new();
    // A reply listed before the branch was made, as a client's request that overlapped the report
    // carries.
    let stale = fixture.list();
    fixture.as_session("a", &["switch", "-q", "-c", "feature"]);
    fixture.deliver(1);

    let mut list = stale;
    annotate(&fixture.state, &fixture.common_dir, &mut list);

    assert!(
        fixture.responsible("feature").is_some(),
        "the branch is missing from the listing because it was made after it"
    );
}

#[test]
fn deleting_a_project_leaves_the_origins_of_another_in_the_same_repository() {
    let fixture = Fixture::new();
    let inner = fixture.repo.join("inner");
    std::fs::create_dir(&inner).unwrap();
    fixture
        .state
        .store
        .insert_project(&project("other", &inner))
        .unwrap();
    fixture.as_session("a", &["branch", "topic"]);
    fixture.deliver(1);

    fixture.state.store.delete_project("project").unwrap();
    project_deleted(&fixture.state, &fixture.repo.to_string_lossy());
    assert!(
        fixture.responsible("topic").is_some(),
        "another project is still in the repository"
    );

    fixture.state.store.delete_project("other").unwrap();
    project_deleted(&fixture.state, &inner.to_string_lossy());
    assert!(fixture.rows().is_empty(), "the last project's deletion");
}

/// The rows of a repository are written under its gate, a project's deletion included: a report
/// holding the gate has read the rows it is about to write, and would put back what the deletion
/// just removed — leaving a repository with rows, no project to clear them and nothing that runs
/// here for it again.
#[test]
fn deleting_a_project_waits_for_a_write_holding_the_gate() {
    let fixture = Fixture::new();
    fixture.as_session("a", &["branch", "topic"]);
    fixture.deliver(1);

    let gate = fixture.state.branch_origins.gate(&fixture.common_dir);
    let held = gate.lock().unwrap();
    let (done, deleted) = std::sync::mpsc::channel();
    let state = Arc::clone(&fixture.state);
    let path = fixture.repo.to_string_lossy().into_owned();
    let worker = std::thread::spawn(move || {
        state.store.delete_project("project").unwrap();
        project_deleted(&state, &path);
        done.send(()).unwrap();
    });

    // The worker is at the gate once it is past the store's deletion; before that, nothing has
    // been waited for.
    let start = std::time::Instant::now();
    while fixture
        .state
        .store
        .get_project("project")
        .unwrap()
        .is_some()
    {
        assert!(start.elapsed() < PATIENCE, "the worker never started");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(fixture.responsible("topic").is_some());
    assert!(
        deleted.try_recv().is_err(),
        "the deletion waits for the gate"
    );
    drop(held);

    deleted
        .recv_timeout(PATIENCE)
        .expect("the deletion goes on");
    worker.join().unwrap();
    assert!(fixture.rows().is_empty());
}

/// A late listing must not set the in-memory record of the repository up again either: nothing is
/// left to clear it, and it would be held until the daemon restarts.
#[test]
fn a_listing_after_the_last_project_went_leaves_no_record_of_the_repository() {
    let fixture = Fixture::new();
    let mut list = fixture.list();
    fixture.state.store.delete_project("project").unwrap();
    project_deleted(&fixture.state, &fixture.repo.to_string_lossy());
    assert!(fixture.state.branch_origins.holds_no_repository());

    annotate(&fixture.state, &fixture.common_dir, &mut list);

    assert!(fixture.state.branch_origins.holds_no_repository());
}

#[test]
fn rows_past_the_cap_are_dropped_oldest_first() {
    let fixture = Fixture::new();
    let key = repo_key(&fixture.common_dir);
    let put: Vec<BranchOrigin> = (0..5)
        .map(|n| BranchOrigin {
            repository: key.clone(),
            branch: format!("b{n}"),
            created_at: n,
            birth_commit: "c".to_string(),
            birth_time: n,
            responsible_session: "a".to_string(),
            responsible_since: n,
            responsible_title: "Session a".to_string(),
        })
        .collect();
    fixture
        .state
        .store
        .apply_branch_origins(&key, &[], &put, 2)
        .unwrap();
    let kept: Vec<_> = fixture.rows().into_iter().map(|row| row.branch).collect();
    assert_eq!(kept, ["b3", "b4"]);
}
