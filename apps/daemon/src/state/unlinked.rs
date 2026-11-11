//! The sessions a start left claimed for the holders whose process runs but that it could not link
//! to: their launches run on, unreachable, so each session stays claimed (as a launch under way
//! is, [`AppState::begin_launch`] refusing it) rather than interrupted, resumed or cleaned up after,
//! and a client is shown its holder as pending. `crate::coordinator::unlinked` claims them at start
//! and resolves them once each holder is linked to or known to be gone.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use super::{AppState, LaunchClaim};
use crate::protocol::HolderLink;

/// What the start left claimed for the holders it could not link to; on `AppState`.
#[derive(Default)]
pub struct Unlinked {
    inner: Mutex<Inner>,
    /// Wakes a resolution that waits for the start to have claimed what it claims.
    claimed: Condvar,
}

#[derive(Default)]
struct Inner {
    /// The start has claimed what it claims ([`AppState::unlinked_claimed`]).
    ready: bool,
    /// The holders still tried, by id.
    holders: HashSet<String>,
    /// The sessions claimed for a launch one of those holders runs, by id, each with that holder
    /// and the launch.
    sessions: HashMap<String, (String, String)>,
    /// The sessions in a running status that no recorded launch places, claimed while any holder
    /// is still tried, since any of them may run one.
    unmatched: HashSet<String>,
    /// The sessions claimed for a holder being ended off the start, which the put-off resume is
    /// for too once they have no process.
    resumable: HashSet<String>,
    /// The start puts off resuming the interrupted sessions and the process ledger's stop of what
    /// the previous run left until no holder is tried, for the sake of `unmatched` and
    /// `resumable`.
    deferred: bool,
    /// The start asked to run them while they were put off ([`AppState::defer_start_work`]), so
    /// they are left to [`AppState::finish_unlinked`].
    start_work_waits: bool,
    /// The sessions the put-off resume is for, when the user asked for one.
    resume_among: Option<HashSet<String>>,
}

/// What is left to do once no holder is tried any more ([`AppState::finish_unlinked`]).
pub struct UnlinkedFinished {
    /// The sessions claimed for want of a recorded launch, still claimed: the caller releases each
    /// ([`AppState::release_unlinked_claim`]) once it has decided what it is.
    pub unmatched: Vec<String>,
    /// The start put off resuming the interrupted sessions and the stop of what the previous run
    /// left, which are the caller's to run now.
    pub run_start_work: bool,
    /// The sessions the put-off resume is for, when the user asked for one at the start.
    pub resume_among: Option<HashSet<String>>,
}

impl Unlinked {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner())
    }
}

impl AppState {
    /// Claims session `id`, whose launch `launch` (with the holder that runs it) a holder the
    /// start could not link to runs, or that no recorded launch places (`None`) while such a holder
    /// exists. A session claimed already is left as it is.
    pub fn claim_unlinked(&self, id: &str, launch: Option<(&str, &str)>) {
        let fresh = self
            .live
            .write()
            .expect("live sessions lock poisoned")
            .launching
            .insert(id.to_string());
        if !fresh {
            return;
        }
        let mut unlinked = self.unlinked.lock();
        match launch {
            Some((holder, launch)) => {
                unlinked
                    .sessions
                    .insert(id.to_string(), (holder.to_string(), launch.to_string()));
            }
            None => {
                unlinked.unmatched.insert(id.to_string());
            }
        }
    }

    /// Claims session `id` for the adoption of a terminal of it, deciding under the lock that
    /// launches claim sessions under, so that no launch can begin between the decision and the
    /// registration of the adopted process: `Some(Some(claim))` when nothing claims it and it is
    /// not running, held until the adopted process is registered; `Some(None)` when the only claim
    /// on it is the one made for a holder the start could not link to, which the adoption takes
    /// over and [`Self::release_unlinked_claim`] releases; `None` when it runs already or a launch
    /// claims it, and the terminal is not to be adopted.
    pub fn claim_for_adoption(self: &Arc<Self>, id: &str) -> Option<Option<LaunchClaim>> {
        let mut live = self.live.write().expect("live sessions lock poisoned");
        if live.sessions.contains_key(id) {
            return None;
        }
        if live.launching.contains(id) {
            let unlinked = self.unlinked.lock();
            let ours = unlinked.sessions.contains_key(id) || unlinked.unmatched.contains(id);
            return ours.then_some(None);
        }
        live.launching.insert(id.to_string());
        drop(live);
        Some(Some(LaunchClaim::new(self.clone(), id.to_string())))
    }

    /// Marks the start's claims as made, with `holders` the holders it could not link to and
    /// `deferred` whether it puts off resuming and the stop of what the previous run left; a
    /// resolution waiting for this goes on.
    pub fn unlinked_claimed(&self, holders: Vec<String>, deferred: bool) {
        let mut unlinked = self.unlinked.lock();
        unlinked.holders.extend(holders);
        unlinked.deferred = deferred;
        unlinked.ready = true;
        drop(unlinked);
        self.unlinked.claimed.notify_all();
    }

    /// Waits until the start has made its claims. **Blocks.**
    pub fn await_unlinked_claimed(&self) {
        let mut unlinked = self.unlinked.lock();
        while !unlinked.ready {
            unlinked = self
                .unlinked
                .claimed
                .wait(unlinked)
                .unwrap_or_else(|err| err.into_inner());
        }
    }

    /// The holder `holder` is no longer tried: it was linked to and what it runs adopted, or it is
    /// gone. Returns the sessions claimed for its launches, each with its launch, still claimed:
    /// the caller releases each ([`Self::release_unlinked_claim`]) once it has decided what it is,
    /// so that nothing launches it in between.
    pub fn take_unlinked_holder(&self, holder: &str) -> Vec<(String, String)> {
        let mut unlinked = self.unlinked.lock();
        unlinked.holders.remove(holder);
        let ids: Vec<String> = unlinked
            .sessions
            .iter()
            .filter(|(_, (of, _))| of == holder)
            .map(|(id, _)| id.clone())
            .collect();
        ids.into_iter()
            .filter_map(|id| {
                let (_, launch) = unlinked.sessions.remove(&id)?;
                Some((id, launch))
            })
            .collect()
    }

    /// Releases the claim on session `id` that [`Self::claim_unlinked`] made, and publishes it.
    pub fn release_unlinked_claim(&self, id: &str) {
        // A claim released with no process adopted for it leaves its saved background work with
        // nothing to be given back to.
        self.release_saved_background(id);
        self.release_launch(id);
        self.publish_session_by_id(id);
    }

    /// Once the start's claims are made and no holder is tried any more: hands over the sessions
    /// claimed for want of a recorded launch and what the start put off, once. `None` while a
    /// holder is still tried, and every time after the first.
    pub fn finish_unlinked(&self) -> Option<UnlinkedFinished> {
        let mut unlinked = self.unlinked.lock();
        if !unlinked.ready || !unlinked.holders.is_empty() {
            return None;
        }
        if unlinked.unmatched.is_empty() && !unlinked.deferred && !unlinked.start_work_waits {
            return None;
        }
        unlinked.deferred = false;
        Some(UnlinkedFinished {
            unmatched: unlinked.unmatched.drain().collect(),
            run_start_work: std::mem::take(&mut unlinked.start_work_waits),
            resume_among: unlinked.resume_among.take(),
        })
    }

    /// Whether the start puts off resuming the interrupted sessions and the stop of what the
    /// previous run left, for a holder still tried that may run a session no record places; if
    /// so, `resume_among` is kept as the sessions the resume is for, and
    /// [`Self::finish_unlinked`] hands both back once no such holder is left. `false` when the
    /// start is to run them now.
    pub fn defer_start_work(&self, resume_among: Option<HashSet<String>>) -> bool {
        let mut unlinked = self.unlinked.lock();
        if unlinked.deferred && !unlinked.holders.is_empty() {
            unlinked.start_work_waits = true;
            unlinked.resume_among = resume_among;
            return true;
        }
        false
    }

    /// Has the put-off resume include `sessions`, claimed for a holder being ended off the start.
    pub fn mark_unlinked_resumable(&self, sessions: Vec<String>) {
        self.unlinked.lock().resumable.extend(sessions);
    }

    /// The sessions the put-off resume is for besides those interrupted at the start: those
    /// claimed for want of a recorded launch, and those of a holder being ended.
    pub fn unlinked_resumable(&self) -> Vec<String> {
        let unlinked = self.unlinked.lock();
        unlinked
            .unmatched
            .iter()
            .chain(unlinked.resumable.iter())
            .cloned()
            .collect()
    }

    /// The holders the start could not link to that are still tried.
    pub fn unlinked_holders(&self) -> HashSet<String> {
        self.unlinked.lock().holders.clone()
    }

    /// Every session claimed for a holder the start could not link to, for the daemon's stop to
    /// interrupt: the stop ends those holders.
    pub fn unlinked_sessions(&self) -> Vec<String> {
        let unlinked = self.unlinked.lock();
        unlinked
            .sessions
            .keys()
            .chain(unlinked.unmatched.iter())
            .cloned()
            .collect()
    }

    /// Every session claimed for a holder the start could not link to, each with the holder that
    /// runs its launch, `None` for one no recorded launch places.
    pub fn unlinked_claims(&self) -> Vec<(String, Option<String>)> {
        let unlinked = self.unlinked.lock();
        unlinked
            .sessions
            .iter()
            .map(|(id, (holder, _))| (id.clone(), Some(holder.clone())))
            .chain(unlinked.unmatched.iter().map(|id| (id.clone(), None)))
            .collect()
    }

    /// What a client is shown of session `id`'s holder: pending while the session is claimed for
    /// a holder the start could not link to, or its live process's holder cannot be reached.
    pub(crate) fn shown_holder_link(&self, id: &str) -> Option<HolderLink> {
        let claimed = {
            let unlinked = self.unlinked.lock();
            unlinked.sessions.contains_key(id) || unlinked.unmatched.contains(id)
        };
        let unreachable = claimed
            || self
                .live_session(id)
                .is_some_and(|live| live.holder_unlinked());
        unreachable.then_some(HolderLink::Pending)
    }
}
