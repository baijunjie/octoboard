//! The scanner: reading the process table and bringing the book up to date with it, and the
//! thread that does so every [`SCAN_INTERVAL`] while there is anything to follow, stopping what
//! ended launches left and what has worked in a removed directory for
//! [`CWD_GRACE`](super::CWD_GRACE).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Weak;
use std::time::Instant;

use super::attribute::{self, ENDED};
use super::table::{self, EnvTag, Identity, Proc, Table};
use super::{settled, ProcessLedger, SCAN_INTERVAL, STUBBORN_RETRY};

impl ProcessLedger {
    /// Reads the table and brings the book up to date with it. A `full` scan may read the
    /// environment of every process; any other only of processes started since the earliest
    /// agent this run followed. `None` when the table cannot be read.
    pub(super) fn scan(&self, full: bool) -> Option<Table> {
        let mut scans = self
            .scans
            .lock()
            .expect("process ledger scans lock poisoned");
        let Some(table) = self.read_table() else {
            tracing::warn!("listing processes failed; the process ledger is not up to date");
            return None;
        };
        scans.tags.retain(|id, _| table.has(id));
        scans.unreadable.retain(|id, _| table.has(id));
        // Which environments the attribution may ask for is decided under the book's lock, with
        // no system call; they are read outside it, so that a launch being registered on the
        // runtime never waits behind them.
        let (horizon, wanted) = {
            let mut book = self.book();
            // What a session read back would decide is decided first, as `attribute` does.
            attribute::prove_read_back(&mut book, &table);
            let horizon = if full { Some(0) } else { book.horizon };
            (
                horizon,
                attribute::tag_candidates(&book, &table, self.own, horizon),
            )
        };
        let settled_before = table::now().saturating_sub(settled().as_micros() as u64);
        let mut read: HashMap<Identity, EnvTag> = HashMap::new();
        for proc in wanted {
            if scans.tags.contains_key(&proc.id)
                || scans.unreadable.get(&proc.id) == Some(&proc.comm)
            {
                continue;
            }
            let tag = table::env_tag(proc.id);
            #[cfg(test)]
            {
                *scans.env_reads.entry(proc.id).or_default() += 1;
            }
            match tag {
                EnvTag::Unreadable => {
                    scans.unreadable.insert(proc.id, proc.comm.clone());
                }
                EnvTag::Untagged if proc.id.start >= settled_before => {}
                _ => {
                    scans.unreadable.remove(&proc.id);
                    scans.tags.insert(proc.id, tag.clone());
                }
            }
            read.insert(proc.id, tag);
        }
        let mut book = self.book();
        let instance = &self.instance;
        let tags = &scans.tags;
        let mut tag_of = |proc: &Proc| {
            if horizon.is_none_or(|horizon| proc.id.start < horizon) {
                return None;
            }
            match read.get(&proc.id).or_else(|| tags.get(&proc.id)) {
                Some(EnvTag::Tagged {
                    instance: theirs,
                    launch,
                }) if theirs == instance => Some(launch.clone()),
                _ => None,
            }
        };
        attribute::attribute(&mut book, &table, self.own, &mut tag_of);
        drop(book);
        self.advance_listings(&table);
        Some(table)
    }

    fn read_table(&self) -> Option<Table> {
        #[cfg(test)]
        if self
            .failing_reads
            .try_update(Ordering::AcqRel, Ordering::Acquire, |left| {
                left.checked_sub(1)
            })
            .is_ok()
        {
            return None;
        }
        Table::read()
    }

    /// One turn of the scanner thread, for a test outside the ledger, whose ledger has none.
    #[cfg(test)]
    pub(crate) fn tick_now(&self) {
        self.tick();
    }

    /// One turn of the scanner thread: reads the table while there is anything to follow, stops
    /// what ended launches left and what has worked in a removed directory for long enough, and
    /// writes the ledger file when it changed.
    pub(super) fn tick(&self) {
        let idle = {
            let book = self.book();
            book.launches.len() <= 1 && book.attributed.is_empty()
        };
        if !idle {
            if let Some(table) = self.scan(false) {
                let removed: HashSet<Identity> = self
                    .worked_in_removed_directories(&table)
                    .into_iter()
                    .collect();
                let mut doomed: HashSet<Identity> =
                    self.book().members_of(ENDED, &table).into_iter().collect();
                doomed.extend(&removed);
                {
                    let mut stubborn = self
                        .stubborn
                        .lock()
                        .expect("stubborn processes lock poisoned");
                    stubborn.retain(|id, _| table.has(id));
                    doomed.retain(|id| match stubborn.get(id) {
                        Some(Some(since)) => since.elapsed() >= STUBBORN_RETRY,
                        _ => true,
                    });
                }
                let removed_count = doomed.intersection(&removed).count();
                if removed_count != 0 {
                    tracing::info!(
                        count = removed_count,
                        "stopping processes whose working directory was removed"
                    );
                }
                if !doomed.is_empty() {
                    self.stop_set(doomed.into_iter().collect(), &table);
                }
            }
        }
        self.persist(false);
    }

    /// The attributed processes of launches being followed that have worked in a removed
    /// directory for [`CWD_GRACE`](super::CWD_GRACE), with their descendants of the same
    /// launch.
    fn worked_in_removed_directories(&self, table: &Table) -> Vec<Identity> {
        let grace = self.cwd_grace();
        let attributed: Vec<(Identity, String)> = self
            .book()
            .attributed
            .iter()
            .filter(|(id, attributed)| attributed.launch != ENDED && table.has(id))
            .map(|(id, attributed)| (*id, attributed.launch.clone()))
            .collect();
        // Read before the lock is taken, each look within a deadline: a directory on a mount that
        // does not answer must not hold the scanner, and counts as not removed.
        let cwds = self.cwds(attributed.iter().map(|(id, _)| *id), None);
        let missing: Vec<bool> = attributed
            .iter()
            .map(|(id, _)| cwds.get(id).is_some_and(|cwd| cwd.missing))
            .collect();
        let mut scans = self
            .scans
            .lock()
            .expect("process ledger scans lock poisoned");
        scans.missing_since.retain(|id, _| table.has(id));
        let now = Instant::now();
        let mut expired = Vec::new();
        for ((id, launch), missing) in attributed.iter().zip(missing) {
            if !missing {
                scans.missing_since.remove(id);
                continue;
            }
            let since = *scans.missing_since.entry(*id).or_insert(now);
            if now.duration_since(since) >= grace {
                expired.push((*id, launch));
            }
        }
        drop(scans);
        let mut doomed: HashSet<Identity> = HashSet::new();
        for (top, launch) in expired {
            doomed.insert(top);
            for (id, of) in &attributed {
                let below = table
                    .get(id.pid)
                    .is_some_and(|proc| table.ancestors(proc).any(|ancestor| ancestor.id == top));
                if of == launch && below {
                    doomed.insert(*id);
                }
            }
        }
        doomed.into_iter().collect()
    }
}

/// The scanner thread: reconciles when asked, and ticks every [`SCAN_INTERVAL`] for as long as
/// the ledger exists.
pub(super) fn run_scanner(ledger: Weak<ProcessLedger>) {
    loop {
        {
            let Some(ledger) = ledger.upgrade() else {
                return;
            };
            if ledger.reconcile_pending.swap(false, Ordering::AcqRel) {
                ledger.reconcile();
            }
        }
        std::thread::sleep(SCAN_INTERVAL);
        let Some(ledger) = ledger.upgrade() else {
            return;
        };
        ledger.tick();
    }
}
