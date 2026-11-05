//! Looks through the file system the ledger makes — at a process's working directory, at a
//! project's directory — each bounded by a deadline.
//!
//! Such a look reads the file system the directory is on, and on a network mount that stopped
//! answering it blocks in the kernel, for as long as the mount's options say: on a hard mount,
//! forever. On the scanner thread that would end every later stop of what ended launches left; on
//! a request's blocking thread, every poll would hold one more thread of the pool every other
//! request shares. So the looks of one call run as a pass of their own
//! ([`blocking_pass::within_deadline`]), waited for only until a deadline, and a look that outran
//! it is skipped by every later call for as long as it is stuck. On macOS, reading what the system
//! reports of a process's working directory is such a look too ([`table::CWD_REPORT_MAY_BLOCK`]),
//! so those reads are a pass of their own before the looks at the directories.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;
use std::time::Duration;

use super::table::{self, Cwd, CwdReport, Identity};
use super::ProcessLedger;
use crate::blocking_pass::{self, StalledPasses, WithinDeadline};

/// How long the looks of one pass are waited for. A directory on a healthy disk answers in well
/// under a millisecond; one on a disk still waking from sleep can take a few seconds, and is left
/// out meanwhile rather than taken for one that does not answer.
const LOOK_DEADLINE: Duration = Duration::from_secs(2);

#[cfg(test)]
thread_local! {
    /// [`LOOK_DEADLINE`] for a test, on the test's own thread.
    pub(super) static LOOK_DEADLINE_OVERRIDE: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
}

fn look_deadline() -> Duration {
    #[cfg(test)]
    if let Some(deadline) = LOOK_DEADLINE_OVERRIDE.get() {
        return deadline;
    }
    LOOK_DEADLINE
}

/// How what the system reports of a process's working directory is read.
pub(super) type CwdRead = Arc<dyn Fn(Identity) -> Option<CwdReport> + Send + Sync>;

/// The looks still stuck past their deadline.
#[derive(Default)]
pub(super) struct Looks {
    /// The looks at a directory, by path.
    paths: StalledPasses<PathBuf>,
    /// The reads of what the system reports of a working directory, by process, where such a
    /// read can block ([`table::CWD_REPORT_MAY_BLOCK`]).
    reports: StalledPasses<Identity>,
    /// Every path a look was asked for, for a test.
    #[cfg(test)]
    looked: Mutex<Vec<PathBuf>>,
}

impl Looks {
    /// What `work` finds for each item of `groups`, grouped by the path the work reads for them,
    /// within the deadline of a look ([`blocking_pass::within_deadline`]). **Blocks** for up to
    /// that deadline, so the caller holds no lock of the ledger.
    fn at_paths<T, R>(
        &self,
        groups: HashMap<PathBuf, Vec<T>>,
        work: impl Fn(&T) -> Option<R> + Send + Sync + 'static,
    ) -> WithinDeadline<PathBuf, T, R>
    where
        T: Send + 'static,
        R: Send + 'static,
    {
        #[cfg(test)]
        self.looked
            .lock()
            .expect("looked paths lock poisoned")
            .extend(groups.keys().cloned());
        let pass = blocking_pass::within_deadline(&self.paths, groups, look_deadline(), work);
        for path in &pass.timed_out {
            tracing::warn!(path = %path.display(), "looking at a directory timed out; it counts as unknown");
        }
        pass
    }

    /// What `read` reports of the working directory of each of `ids`, those it reports nothing
    /// for left out. Where the read may block, the reads are a pass of their own within the
    /// deadline of a look: a process whose read has not ended by then is left out, and so is one
    /// whose read from an earlier call is still stuck. **Blocks** for up to that deadline then.
    fn reports(
        &self,
        ids: impl IntoIterator<Item = Identity>,
        read: CwdRead,
        may_block: bool,
    ) -> Vec<(Identity, CwdReport)> {
        if !may_block {
            return ids
                .into_iter()
                .filter_map(|id| Some((id, read(id)?)))
                .collect();
        }
        let groups = ids.into_iter().map(|id| (id, vec![id])).collect();
        let pass =
            blocking_pass::within_deadline(&self.reports, groups, look_deadline(), move |id| {
                read(*id)
            });
        for id in &pass.timed_out {
            tracing::warn!(
                pid = id.pid.0,
                "reading a process's working directory timed out; it counts as unknown"
            );
        }
        pass.found
    }

    /// Every path a look was asked for, for a test.
    #[cfg(test)]
    #[cfg_attr(
        windows,
        allow(
            dead_code,
            reason = "only a test of what is left in a removed directory reads it"
        )
    )]
    pub(super) fn looked(&self) -> Vec<PathBuf> {
        self.looked
            .lock()
            .expect("looked paths lock poisoned")
            .clone()
    }
}

/// Directories a process may have been left behind in, each as given and, where that differs,
/// with its links resolved, as the system reports a working directory (`/private/tmp` for `/tmp`
/// on macOS); each with the index of the directory it was made from.
#[derive(Debug, Default)]
pub(super) struct Roots {
    dirs: Vec<(usize, PathBuf)>,
    /// The directories, as given, whose look did not answer: whatever lies under one is on a file
    /// system that did not answer either, and is not looked at.
    unanswered: Vec<PathBuf>,
}

impl Roots {
    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty()
    }

    /// Whether `path` lies under one of the directories.
    pub fn holds(&self, path: &Path) -> bool {
        self.first_holding(path).is_some()
    }

    /// The first of the directories the roots were made from (by index) that `path` lies under.
    pub fn first_holding(&self, path: &Path) -> Option<usize> {
        self.dirs
            .iter()
            .filter(|(_, root)| path.starts_with(root))
            .map(|(index, _)| *index)
            .min()
    }

    /// Whether the directory `report` names is worth a look, from what the system reports alone:
    /// it may lie under one of the directories, and lies under none whose look did not answer.
    fn worth_looking(&self, report: &CwdReport) -> bool {
        self.dirs.iter().any(|(_, root)| report.may_lie_under(root))
            && !self
                .unanswered
                .iter()
                .any(|root| report.may_lie_under(root))
    }
}

impl ProcessLedger {
    /// `dirs` as [`Roots`], each resolved within the deadline of a look; one that does not answer
    /// in time stands as given. **Blocks**, for up to that deadline.
    pub(super) fn roots(&self, dirs: &[PathBuf]) -> Roots {
        let mut groups: HashMap<PathBuf, Vec<(usize, PathBuf)>> = HashMap::new();
        for (index, dir) in dirs.iter().enumerate() {
            groups
                .entry(dir.clone())
                .or_default()
                .push((index, dir.clone()));
        }
        let mut roots: Vec<(usize, PathBuf)> = dirs.iter().cloned().enumerate().collect();
        let pass = self
            .looks
            .at_paths(groups, |(_, dir)| crate::hostfs::canonical(dir).ok());
        for ((index, dir), resolved) in pass.found {
            if resolved != dir {
                roots.push((index, resolved));
            }
        }
        Roots {
            dirs: roots,
            unanswered: pass.timed_out.into_iter().chain(pass.stuck).collect(),
        }
    }

    /// The working directories of `ids` that are still running, each looked at within the
    /// deadline of a look; with `roots`, only those the system reports under one of them, the
    /// others left out without a look (see [`Roots::worth_looking`]). A directory that does not
    /// answer in time is left out too. **Blocks**, for up to that deadline, and on macOS for up to
    /// that deadline again before, while the system's reports of the directories are read.
    pub(super) fn cwds(
        &self,
        ids: impl IntoIterator<Item = Identity>,
        roots: Option<&Roots>,
    ) -> HashMap<Identity, Cwd> {
        let (read, may_block) = self.cwd_reader();
        let mut groups: HashMap<PathBuf, Vec<(Identity, CwdReport)>> = HashMap::new();
        for (id, report) in self.looks.reports(ids, read, may_block) {
            if roots.is_some_and(|roots| !roots.worth_looking(&report)) {
                continue;
            }
            groups
                .entry(report.path().to_path_buf())
                .or_default()
                .push((id, report));
        }
        #[cfg(test)]
        let injected = self
            .cwd_look
            .lock()
            .expect("cwd look lock poisoned")
            .clone();
        let look = move |(id, report): &(Identity, CwdReport)| {
            #[cfg(test)]
            if let Some(look) = &injected {
                let cwd = look(report)?;
                return id.is_running().then_some(cwd);
            }
            // Checked after the look, so what was looked at is the directory of the process that
            // had the identity.
            let cwd = report.looked()?;
            id.is_running().then_some(cwd)
        };
        self.looks
            .at_paths(groups, look)
            .found
            .into_iter()
            .map(|((id, _), cwd)| (id, cwd))
            .collect()
    }

    /// How what the system reports of a working directory is read, and whether that read may
    /// block: the system's own, or, for a test, a read that stands in for it and may.
    fn cwd_reader(&self) -> (CwdRead, bool) {
        #[cfg(test)]
        if let Some(read) = self
            .cwd_read
            .lock()
            .expect("cwd read lock poisoned")
            .clone()
        {
            return (read, true);
        }
        (Arc::new(table::cwd_report), table::CWD_REPORT_MAY_BLOCK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What lies under a directory whose look did not answer is not looked at, however it also
    /// lies under a directory that did answer, while a directory elsewhere under the answered one
    /// still is.
    #[test]
    fn a_directory_under_an_unanswered_root_is_not_worth_a_look() {
        let roots = Roots {
            dirs: vec![(0, PathBuf::from("/octoboard-test/work"))],
            unanswered: vec![PathBuf::from("/octoboard-test/work/mount")],
        };
        assert!(roots.worth_looking(&CwdReport::at("/octoboard-test/work/src")));
        assert!(!roots.worth_looking(&CwdReport::at("/octoboard-test/work/mount/deep")));
        assert!(!roots.worth_looking(&CwdReport::at("/octoboard-test/other")));
    }
}
