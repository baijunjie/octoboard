//! The bookkeeping of the sessions' record watches: which generation of a session's watch is
//! current, and how far a carried-offset watch has read.

use super::AppState;

impl AppState {
    /// Starts a new record watch for this session and returns the generation it owns.
    /// Recording it here retires whatever watch was running for the session before: its generation
    /// is no longer the one found under this id, so it stops at its next poll instead of racing the
    /// new one. See `crate::records::watch::spawn`.
    pub fn begin_record_watch(&self, id: &str) -> u64 {
        let generation = self
            .next_record_watch_generation
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.record_watch_generations
            .lock()
            .expect("record watch lock poisoned")
            .insert(id.to_string(), generation);
        generation
    }

    /// Where a carried-offset watch of the record at `path` may start reading: past what an earlier
    /// watch of the same file already read, or zero for a file not seen before.
    pub fn record_offset(&self, id: &str, path: &std::path::Path) -> u64 {
        self.record_offsets
            .lock()
            .expect("record offset lock poisoned")
            .get(id)
            .filter(|(seen, _)| seen == path)
            .map_or(0, |(_, offset)| *offset)
    }

    /// Records how far a carried-offset watch has read into the record at `path`.
    pub fn note_record_offset(&self, id: &str, path: &std::path::Path, offset: u64) {
        self.record_offsets
            .lock()
            .expect("record offset lock poisoned")
            .insert(id.to_string(), (path.to_path_buf(), offset));
    }

    /// Retires the session's record watch, if any, because the turn it was watching is over: a
    /// new turn is about to open, and a watch left current could close that one instead.
    pub fn end_record_watch(&self, id: &str) {
        self.record_watch_generations
            .lock()
            .expect("record watch lock poisoned")
            .remove(id);
    }

    /// Whether `generation` is still this session's current record watch — false once a newer
    /// watch has taken over, or the session's bookkeeping has been dropped entirely.
    pub fn record_watch_current(&self, id: &str, generation: u64) -> bool {
        self.record_watch_generations
            .lock()
            .expect("record watch lock poisoned")
            .get(id)
            == Some(&generation)
    }

    /// Runs `write` only while `generation` is still this session's current record watch, and says
    /// what it returned, or `None` for a watch retired or superseded meanwhile. The bookkeeping
    /// stays locked across `write`, so a hook starting or ending a watch waits for it instead of
    /// slipping in between the check and the write. That lock is never taken while any other lock
    /// is held, so `write` may take any lock but this one; it must not start or end a watch.
    pub fn while_record_watch_current<R>(
        &self,
        id: &str,
        generation: u64,
        write: impl FnOnce() -> R,
    ) -> Option<R> {
        let generations = self
            .record_watch_generations
            .lock()
            .expect("record watch lock poisoned");
        (generations.get(id) == Some(&generation)).then(write)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::app_state;

    /// A carried-offset watch resumes where the session's previous watch of the same file got to,
    /// and a different file starts again from zero.
    #[test]
    fn a_record_offset_is_carried_for_the_same_file_and_not_for_another() {
        let (state, _dir) = app_state("state-record-offset");
        let path = std::path::Path::new("/rollout-a.jsonl");
        assert_eq!(state.record_offset("s", path), 0);
        state.note_record_offset("s", path, 120);
        assert_eq!(state.record_offset("s", path), 120);
        assert_eq!(
            state.record_offset("s", std::path::Path::new("/rollout-b.jsonl")),
            0
        );
    }
}
