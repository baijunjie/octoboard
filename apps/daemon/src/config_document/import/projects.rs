//! Where an imported project is placed: the directory choices a document's project offers on this
//! machine, the existence checks that decide them, and the association of the project with its
//! console in the directory chosen.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;

use super::{failed, ProjectAccounts};
use crate::blocking_pass::each_blocking;
use crate::coordinator;
use crate::coordinator::projects::{AddProjectRequest, NewProjectSettings};
use crate::hostfs;
use crate::platform::names;
use crate::project_git::remote_url;
use crate::protocol::{
    error_code, CodedError, DirectoryChoice, DirectoryOffers, DocumentProject, ImportStatus,
    ImportedProject, ProjectSource,
};
use crate::state::AppState;

/// The recorded path, when it is an absolute path to take as it is. A relative one, or one starting
/// with `~`, would mean something else on this machine than where it was written.
pub(super) fn exported_directory(recorded: &str) -> Option<PathBuf> {
    let path = Path::new(recorded);
    path.is_absolute()
        .then(|| hostfs::lexically_normalise(path))
}

/// Whether the recorded path is absolute in the other host family's style, the way a document
/// exported on another operating system records one. It names nothing on this machine, so it
/// derives no directory at all. Surrounding whitespace is ignored, as it is for the account and
/// clone directories.
pub(super) fn is_foreign(recorded: &str) -> bool {
    names::is_foreign_absolute(recorded.trim())
}

/// The recorded path's last segment under `clone_dir`. A document exported without paths records
/// only a directory's name, which is that segment as it stands. `None` when the path has no last
/// segment (the filesystem root, `.`, `..`, empty), and so never a path outside `clone_dir`; and
/// `None` for a path written on another operating system (see [`is_foreign`]).
pub(super) fn derived_directory(clone_dir: &str, recorded: &str) -> Option<PathBuf> {
    let clone_dir = Path::new(clone_dir);
    if !clone_dir.is_absolute() || is_foreign(recorded) {
        return None;
    }
    let name = match exported_directory(recorded) {
        Some(path) => path.file_name()?.to_owned(),
        None => bare_name(recorded)?,
    };
    Some(clone_dir.join(name))
}

/// `recorded` when it is a single plain segment, a directory's name and no path.
fn bare_name(recorded: &str) -> Option<std::ffi::OsString> {
    single_segment(recorded).map(ToOwned::to_owned)
}

/// `name` when it is a single plain path segment.
fn single_segment(name: &str) -> Option<&std::ffi::OsStr> {
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(name)), None) => Some(name),
        _ => None,
    }
}

/// The directory a clone of `remote_url` into `clone_dir` would create, when the URL may be cloned
/// from, carries no credential (as `add_project` refuses one) and names a repository.
pub(super) fn clone_directory(clone_dir: &str, remote_url: &str) -> Option<PathBuf> {
    let clone_dir = Path::new(clone_dir);
    if !clone_dir.is_absolute()
        || !remote_url::is_clonable_url(remote_url)
        || remote_url::has_credentials(remote_url)
    {
        return None;
    }
    let name = crate::project_git::clone::repo_name(remote_url)?;
    single_segment(&name)?;
    Some(clone_dir.join(name))
}

/// What a project's directory choices come to on the disk of this machine.
pub(super) struct Probe {
    pub(super) console: usize,
    pub(super) project: usize,
    pub(super) exported: Option<PathBuf>,
    pub(super) derived: Option<PathBuf>,
    pub(super) clone: Option<PathBuf>,
}

pub(super) fn text(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

/// How long the existence checks of one inspection may take in all. Generous for a local disk,
/// where they take microseconds; it is there for the path that never answers.
pub(super) const PROBE_DEADLINE: Duration = Duration::from_secs(15);

/// What a project's three choices come to on this machine's disk. A clone is offered only while
/// its target is free, since the clone would refuse otherwise; the other two only while the
/// directory exists.
pub(super) fn probe_disk(probe: Probe) -> ((usize, usize), DirectoryOffers) {
    let offers = DirectoryOffers {
        exported_path: probe.exported.filter(|path| path.is_dir()).map(text),
        default_clone_dir: probe.derived.filter(|path| path.is_dir()).map(text),
        clone: probe.clone.filter(|path| !path.exists()).map(text),
    };
    ((probe.console, probe.project), offers)
}

/// Runs `work` over `probes` on blocking threads, at most `blocking_pass::BLOCKING_WIDTH` at a
/// time, and returns what came back within `deadline`. After it the probes still running are
/// given up on, along with the ones not yet started: a blocked thread cannot be interrupted, but
/// nothing waits for it and no more are started behind it.
pub(super) async fn probe_offers(
    probes: Vec<Probe>,
    deadline: Duration,
    work: impl Fn(Probe) -> ((usize, usize), DirectoryOffers) + Send + Sync + 'static,
) -> HashMap<(usize, usize), DirectoryOffers> {
    let done = Arc::new(Mutex::new(HashMap::new()));
    let sink = done.clone();
    // Each result is kept as it arrives, since `each_blocking` returns only when all have.
    let all = each_blocking(probes, move |probe| {
        let (key, offers) = work(probe);
        sink.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key, offers);
    });
    let _ = tokio::time::timeout(deadline, all).await;
    let mut done = done.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::mem::take(&mut *done)
}

/// Associates `project` with the console `console_id` in the directory `choice` names.
pub(super) async fn place_project(
    state: &Arc<AppState>,
    console_id: &str,
    project: DocumentProject,
    accounts: ProjectAccounts,
    choice: &DirectoryChoice,
    clone_dir: &str,
) -> ImportedProject {
    let name = project.name.clone();
    let unavailable = || {
        CodedError::raised(
            error_code::IMPORT_CHOICE_UNAVAILABLE,
            format!("`{name}` has nothing to use for that choice"),
            &[("choice", &choice.kind().wire_name())],
        )
    };
    let placement: Result<(ProjectSource, Option<String>, Option<String>)> = match choice {
        DirectoryChoice::ExportedPath => exported_directory(&project.path)
            .map(|path| (ProjectSource::Local, Some(text(path)), None))
            .ok_or_else(unavailable),
        DirectoryChoice::DefaultCloneDir => derived_directory(clone_dir, &project.path)
            .map(|path| (ProjectSource::Local, Some(text(path)), None))
            .ok_or_else(unavailable),
        DirectoryChoice::Clone => project
            .remote_url
            .clone()
            .filter(|url| clone_directory(clone_dir, url).is_some())
            .map(|url| (ProjectSource::Git, None, Some(url)))
            .ok_or_else(unavailable),
        DirectoryChoice::Directory { path } => Ok((ProjectSource::Local, Some(path.clone()), None)),
    };
    let added = match placement {
        Ok((source, path, remote_url)) => {
            coordinator::projects::add_project(
                state,
                AddProjectRequest {
                    console_id: console_id.to_string(),
                    source,
                    path,
                    remote_url,
                    name: Some(project.name),
                    default_agent: project.default_agent,
                    settings: NewProjectSettings {
                        pinned: project.pinned,
                        accounts: accounts.accounts,
                        launch: project.launch,
                        git_workflow: project.git_workflow,
                    },
                    // The document says what the project's default agent is, including none.
                    detect_default_agent: false,
                    tags: Some(project.tags),
                    description: project.description,
                    // The document's values were chosen on the machine it came from.
                    check_launch: false,
                    check_text_limits: false,
                    create_directory: false,
                },
            )
            .await
        }
        Err(err) => Err(err),
    };
    match added.map(|mut added| added.pop()) {
        Ok(Some(created)) => ImportedProject {
            name,
            status: ImportStatus::Created,
            via: Some(choice.kind()),
            project: Some(created.id),
            error: None,
            account_fallbacks: accounts.fallbacks,
        },
        Ok(None) => failed(name, anyhow::anyhow!("no project was created")),
        Err(err) => failed(name, err),
    }
}
