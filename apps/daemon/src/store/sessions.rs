//! The sessions table: inserting a session, the column-scoped writers, launch overrides, colour,
//! deletion and the reads.

use super::Result;
use rusqlite::{params, Connection, OptionalExtension, Row};

use super::rows::{enum_from_row, enum_from_row_opt, enum_from_text, enum_to_text};
use super::Store;
use crate::protocol::{
    Agent, CodedError, ConsoleSessionColour, Origin, RecordTitles, Role, Session, SessionKind,
    SessionStatus,
};

/// What the agent's records gave a session, to be stored if it differs from what is.
pub struct RecordUpdate {
    /// The session as it was read, which says whose records these are.
    pub session: Session,
    /// The context size the records show: `None` keeps what is stored, `Some(None)` clears it
    /// (the records say it is not known now, as after a compaction).
    pub context: Option<Option<crate::protocol::SessionContext>>,
    pub titles: Option<RecordTitles>,
    /// The model and effort the records show, where they show one; an absent value keeps what is
    /// stored. The effort can also be cleared, `Some(None)`, where the records say none is set.
    pub observed_model: Option<String>,
    pub observed_effort: Option<Option<String>>,
    /// The observation boundary the records were read against (see
    /// `records::collector::ObservedBoundary`): the model and effort are not written once a restart
    /// has stored another.
    pub observed_boundary: Option<String>,
}

/// The statuses at which an agent is in the middle of a turn: `working`, or `waiting_user` on a
/// prompt or question inside it. One list for [`cuts_turn_short`] and [`cut_short_turn`], so the
/// rule and the SQL that writes it cannot drift apart.
const TURN_IN_PROGRESS: [SessionStatus; 2] = [SessionStatus::Working, SessionStatus::WaitingUser];

/// The rule [`cut_short_turn`] writes: an agent at `status` when its process ended was cut short.
pub fn cuts_turn_short(status: SessionStatus) -> bool {
    TURN_IN_PROGRESS.contains(&status)
}

/// The `last_turn_abnormal` assignment of a write that ends the session's process: a turn in
/// progress at that moment ([`TURN_IN_PROGRESS`]) was cut short, so it ended abnormally; any other
/// status keeps what the last turn left. `agents_own` is the SQL of the agent's own status where
/// the stored one is not it (a waiting session request's hand holds the record at `waiting_user`
/// over an agent at any status), `NULL` where it is. It counts only while the stored row still
/// has a process: a row the process has already been recorded gone from (the exit of a process an
/// archive or a shutdown ended) was settled by that first end, which a late end of its turn may
/// have revised since. SQLite evaluates it against the row as it was before the write.
fn cut_short_turn(agents_own: &str) -> String {
    let in_progress = sql_statuses(&TURN_IN_PROGRESS);
    let no_process = sql_statuses(&[SessionStatus::Archived, SessionStatus::Interrupted]);
    format!(
        "last_turn_abnormal = CASE WHEN status NOT IN ({no_process})
                                        AND COALESCE({agents_own}, status) IN ({in_progress})
                                   THEN 1 ELSE last_turn_abnormal END"
    )
}

/// `statuses` as an SQL list of their stored texts.
fn sql_statuses(statuses: &[SessionStatus]) -> String {
    statuses
        .iter()
        .map(|status| format!("'{}'", enum_to_text(status)))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) const SESSION_COLUMNS: &str =
    "id, agent, agent_session_id, console_id, project_id, host_id,
                               kind, origin, title, status, has_conversation, bound_to, colour,
                               ordinal, account_id, config_dir, pinned, started_at, ended_at,
                               role, terminal_title, model, effort, model_override,
                               effort_override, context, record_title, record_prompt, locked,
                               observed_model, observed_effort, last_turn_abnormal";

impl Store {
    /// Inserts a session that is not a console session, or a console session whose colour and
    /// ordinal are already decided (a test fixture; the daemon itself always creates a console
    /// session through [`Self::insert_console_session`] instead, which decides them).
    pub fn insert_session(&self, session: &Session) -> Result<()> {
        insert_session_row(&self.lock(), session)
    }

    /// Inserts a project session bound to `owner`, only while `owner` can still own sessions — it
    /// is a coordinator, the condition [`Self::set_binding`] keeps — and says whether it did. The
    /// check and the insert run under one lock, so a binding that lands between the caller's own
    /// check and this write either came first, and is seen here, or comes after, and sees the new
    /// session as a child of `owner` (`set_binding`'s `childless`): orchestration cannot reach a
    /// third level either way. `false` also covers an `owner` that is gone.
    pub fn insert_session_under_owner(&self, session: &Session, owner: &str) -> Result<bool> {
        let conn = self.lock();
        let can_own: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sessions AS owner WHERE owner.id = ?1
                              AND owner.role = ?2)",
            params![owner, enum_to_text(&Role::Coordinator)],
            |row| row.get(0),
        )?;
        if !can_own {
            return Ok(false);
        }
        insert_session_row(&conn, session)?;
        Ok(true)
    }

    /// Inserts a freshly-opened console session, assigning its colour and ordinal under the same
    /// lock as the row insert. With no explicit title, it uses the console's current name.
    ///
    /// The lock prevents concurrent opens from choosing the same available colour or ordinal.
    /// Ordinals are consumed before launch and are not given back if the agent fails to start.
    /// `session.colour`, `session.ordinal` and `session.title` are overwritten.
    pub fn insert_console_session(
        &self,
        mut session: Session,
        title: Option<String>,
    ) -> Result<Session> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT colour FROM sessions
             WHERE console_id = ?1 AND kind = ?2 AND status != ?3 AND colour IS NOT NULL",
        )?;
        let used_colours: Vec<String> = stmt
            .query_map(
                params![
                    session.console_id,
                    enum_to_text(&SessionKind::Console),
                    enum_to_text(&SessionStatus::Archived),
                ],
                |row| row.get(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        let held: Vec<ConsoleSessionColour> = used_colours
            .iter()
            .filter_map(|text| enum_from_text(text).ok())
            .collect();
        // Every entry already held: pick among all rather than refuse, since the badge is a
        // convenience, not a uniqueness guarantee the user was ever promised.
        let colour = assign_console_session_colour(&held, uuid::Uuid::new_v4().as_u128() as usize);
        let (ordinal, console_name): (i64, String) = conn
            .query_row(
                "UPDATE consoles SET next_console_session_ordinal = next_console_session_ordinal + 1
                 WHERE id = ?1
                 RETURNING next_console_session_ordinal - 1, name",
                params![session.console_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| CodedError::unknown_console(&session.console_id))?;
        session.colour = Some(colour);
        session.ordinal = Some(ordinal);
        session.title = title.unwrap_or(console_name);
        insert_session_row(&conn, &session)?;
        Ok(session)
    }

    /// Writes a whole record's changing fields back, for tests to put a session in a state. The
    /// daemon itself never writes a record it read earlier back whole: whatever another task wrote
    /// to the row in between would be lost, so each change goes through the method that writes its
    /// own columns ([`Self::set_session_title`], [`Self::set_running_status`],
    /// [`Self::archive_session`] and the others below). Identity, placement, the binding, the
    /// account, `pinned`, `locked` and `colour` are left alone, as those methods leave them.
    /// Returns whether a row was written: `false` means the session is gone.
    #[cfg(test)]
    pub fn update_session(&self, session: &Session) -> Result<bool> {
        let changed = self.lock().execute(
            "UPDATE sessions SET agent = ?2, agent_session_id = ?3, title = ?4, status = ?5,
                                 has_conversation = ?6, started_at = ?7, ended_at = ?8,
                                 last_turn_abnormal = ?9,
                                 context = CASE WHEN agent = ?2 AND agent_session_id IS ?3 THEN context ELSE NULL END,
                                 record_title = CASE WHEN agent = ?2 AND agent_session_id IS ?3 THEN record_title ELSE NULL END,
                                 record_prompt = CASE WHEN agent = ?2 AND agent_session_id IS ?3 THEN record_prompt ELSE NULL END,
                                 observed_model = CASE WHEN agent = ?2 AND agent_session_id IS ?3 THEN observed_model ELSE NULL END,
                                 observed_effort = CASE WHEN agent = ?2 AND agent_session_id IS ?3 THEN observed_effort ELSE NULL END,
                                 observed_boundary = CASE WHEN agent = ?2 AND agent_session_id IS ?3 THEN observed_boundary ELSE NULL END
             WHERE id = ?1",
            params![
                session.id,
                enum_to_text(&session.agent),
                session.agent_session_id,
                session.title,
                enum_to_text(&session.status),
                session.has_conversation,
                session.started_at,
                session.ended_at,
                session.paused,
            ],
        )?;
        Ok(changed > 0)
    }

    /// Runs `update`, one `UPDATE` of session `id`'s row, and returns the row as it then stands
    /// when the statement matched it, `None` when it did not. Both run under one take of the lock,
    /// so the record returned is the one the write produced.
    pub(super) fn update_session_row(
        &self,
        id: &str,
        update: &str,
        params: impl rusqlite::Params,
    ) -> Result<Option<Session>> {
        let conn = self.lock();
        if conn.execute(update, params)? == 0 {
            return Ok(None);
        }
        Ok(conn
            .query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                params![id],
                read_session,
            )
            .optional()?)
    }

    /// Writes the session's title and nothing else. Returns the record as it now stands, or `None`
    /// when the session is gone.
    pub fn set_session_title(&self, id: &str, title: &str) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET title = ?2 WHERE id = ?1",
            params![id, title],
        )
    }

    /// Records the id the agent gave the session's conversation, unless it already holds that id.
    /// What the agent's records gave the session (context, record titles, observed model and
    /// effort) belonged to the previous conversation and is dropped with it. Returns the record as
    /// it now stands, or `None` when nothing changed or the session is gone.
    pub fn set_agent_session_id(
        &self,
        id: &str,
        agent_session_id: &str,
    ) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET agent_session_id = ?2, context = NULL, record_title = NULL,
                                 record_prompt = NULL, observed_model = NULL,
                                 observed_effort = NULL, observed_boundary = NULL
             WHERE id = ?1 AND agent_session_id IS NOT ?2",
            params![id, agent_session_id],
        )
    }

    /// Records that the session has had a turn. Returns the record as it now stands, or `None`
    /// when it was already recorded or the session is gone.
    pub fn mark_conversation_started(&self, id: &str) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET has_conversation = 1 WHERE id = ?1 AND has_conversation = 0",
            params![id],
        )
    }

    /// Forgets the session's conversation, its id and what the agent's records gave it, how its
    /// last turn ended included, as for a session that has had no turn. Returns the record as it
    /// now stands, or `None` when the session is gone.
    pub fn forget_conversation(&self, id: &str) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET has_conversation = 0, agent_session_id = NULL, context = NULL,
                                 record_title = NULL, record_prompt = NULL, observed_model = NULL,
                                 observed_effort = NULL, observed_boundary = NULL,
                                 last_turn_abnormal = 0
             WHERE id = ?1",
            params![id],
        )
    }

    /// Moves a session to `status`, which must not be a no-process status, but only while the
    /// session has a process and is at another status. A running session's status follows its
    /// agent's events, which arrive on tasks of their own; matching in the statement means one that
    /// lands after an archive or an exit cannot bring the session back to life. Any status but
    /// `Idle` moves the session off its prompt and clears `last_turn_abnormal` in the same write.
    /// Returns the record as it now stands, or `None` when nothing changed or the session is gone.
    pub fn set_running_status(&self, id: &str, status: SessionStatus) -> Result<Option<Session>> {
        self.write_running_status(id, status, true)
    }

    /// [`Self::set_running_status`] to `WaitingUser` for a hand raised over the agent by something
    /// other than its own status (a waiting session request): the agent has not left its prompt,
    /// so `last_turn_abnormal` stays as its last turn left it.
    pub fn hold_waiting_user(&self, id: &str) -> Result<Option<Session>> {
        self.write_running_status(id, SessionStatus::WaitingUser, false)
    }

    /// The write of [`Self::set_running_status`], clearing `last_turn_abnormal` off the prompt
    /// only when `agents_own` (the status is the agent's own).
    fn write_running_status(
        &self,
        id: &str,
        status: SessionStatus,
        agents_own: bool,
    ) -> Result<Option<Session>> {
        debug_assert!(!status.has_no_process());
        self.update_session_row(
            id,
            "UPDATE sessions SET status = ?2,
                                 last_turn_abnormal = CASE WHEN ?6 = 0 OR ?2 = ?5
                                                           THEN last_turn_abnormal ELSE 0 END
             WHERE id = ?1 AND status != ?2 AND status NOT IN (?3, ?4)",
            params![
                id,
                enum_to_text(&status),
                enum_to_text(&SessionStatus::Interrupted),
                enum_to_text(&SessionStatus::Archived),
                enum_to_text(&SessionStatus::Idle),
                agents_own,
            ],
        )
    }

    /// Records whether the session's last turn ended abnormally, while the session has a process:
    /// what a turn of a process that has ended did is settled by the end itself (see
    /// [`Self::record_session_exit`]). Returns the record as it now stands when the flag changed,
    /// `None` when it did not, the session has no process or it is gone.
    pub fn set_last_turn_abnormal(&self, id: &str, abnormal: bool) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET last_turn_abnormal = ?2
             WHERE id = ?1 AND last_turn_abnormal != ?2 AND status NOT IN (?3, ?4)",
            params![
                id,
                abnormal,
                enum_to_text(&SessionStatus::Interrupted),
                enum_to_text(&SessionStatus::Archived),
            ],
        )
    }

    /// Archives the session as of `ended_at`, from whatever status it is at, a turn in progress
    /// recorded as cut short (`cut_short_turn`, judged on `agents_own` where the agent's own
    /// status is not the stored one); a locked session is refused by the schema. Returns the
    /// status the session was at and the record as it now stands, or `None` when the session is
    /// gone.
    pub fn archive_session(
        &self,
        id: &str,
        ended_at: i64,
        agents_own: Option<SessionStatus>,
    ) -> Result<Option<(SessionStatus, Session)>> {
        self.end_session_row(
            id,
            &format!(
                "UPDATE sessions SET status = ?2, ended_at = ?3, {} WHERE id = ?1",
                cut_short_turn("?4")
            ),
            params![
                id,
                enum_to_text(&SessionStatus::Archived),
                ended_at,
                agents_own.as_ref().map(enum_to_text),
            ],
        )
    }

    /// Interrupts the session as of `ended_at`, unless it is archived: an archive is the user's
    /// decision and outlasts the process going away. A turn in progress is recorded as cut short
    /// (`cut_short_turn`, judged on `agents_own` as in [`Self::archive_session`]). Returns the
    /// status the session was at and the record as it now stands, or `None` when the session is
    /// archived or gone.
    pub fn interrupt_session(
        &self,
        id: &str,
        ended_at: i64,
        agents_own: Option<SessionStatus>,
    ) -> Result<Option<(SessionStatus, Session)>> {
        self.end_session_row(
            id,
            &format!(
                "UPDATE sessions SET status = ?2, ended_at = ?3, {}
                 WHERE id = ?1 AND status != ?4",
                cut_short_turn("?5")
            ),
            params![
                id,
                enum_to_text(&SessionStatus::Interrupted),
                ended_at,
                enum_to_text(&SessionStatus::Archived),
                agents_own.as_ref().map(enum_to_text),
            ],
        )
    }

    /// Records that the session's process ended at `ended_at`: an archived session stays archived,
    /// anything else becomes interrupted, and a turn in progress is recorded as having ended
    /// abnormally (`cut_short_turn`, judged on `agents_own` as in [`Self::archive_session`]).
    /// Returns the status the session was at and the record as it now stands, or `None` when the
    /// session is gone.
    pub fn record_session_exit(
        &self,
        id: &str,
        ended_at: i64,
        agents_own: Option<SessionStatus>,
    ) -> Result<Option<(SessionStatus, Session)>> {
        self.end_session_row(
            id,
            &format!(
                "UPDATE sessions SET status = CASE WHEN status = ?2 THEN status ELSE ?3 END,
                                     ended_at = ?4, {}
                 WHERE id = ?1",
                cut_short_turn("?5")
            ),
            params![
                id,
                enum_to_text(&SessionStatus::Archived),
                enum_to_text(&SessionStatus::Interrupted),
                ended_at,
                agents_own.as_ref().map(enum_to_text),
            ],
        )
    }

    /// Runs `update`, a write that ends the session's process, and returns the status the session
    /// was at before it with the record as it now stands, all read under the write's lock; `None`
    /// when the session is gone or the write changed nothing.
    fn end_session_row(
        &self,
        id: &str,
        update: &str,
        params: impl rusqlite::Params,
    ) -> Result<Option<(SessionStatus, Session)>> {
        let conn = self.lock();
        let select = format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1");
        let Some(before) = conn
            .query_row(&select, params![id], read_session)
            .optional()?
        else {
            return Ok(None);
        };
        if conn.execute(update, params)? == 0 {
            return Ok(None);
        }
        let after = conn.query_row(&select, params![id], read_session)?;
        Ok(Some((before.status, after)))
    }

    /// Clears the flag the end of the session's process at `ended_at` set for a turn it cut short,
    /// for that turn's own normal end arriving after it. Only while the record is still as that
    /// end left it — without a process, and ended at `ended_at` — so it touches neither a process
    /// started since nor a later end. Returns the record as it now stands when it changed.
    pub fn clear_cut_short_turn(&self, id: &str, ended_at: i64) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET last_turn_abnormal = 0
             WHERE id = ?1 AND ended_at = ?2 AND status IN (?3, ?4) AND last_turn_abnormal = 1",
            params![
                id,
                ended_at,
                enum_to_text(&SessionStatus::Interrupted),
                enum_to_text(&SessionStatus::Archived),
            ],
        )
    }

    /// Persists what the agent's records gave sessions, only while the source still belongs to the
    /// session's current agent identity; unrelated session writes never overwrite these columns.
    /// Returns the sessions that changed.
    pub fn set_record_data(&self, updates: &[RecordUpdate]) -> Result<Vec<Session>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let mut changed = Vec::new();
        for update in updates {
            let source = &update.session;
            let session = tx
                .query_row(
                    &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                    params![source.id],
                    read_session,
                )
                .optional()?;
            let Some(mut session) = session else { continue };
            if session.agent != source.agent
                || session.agent_session_id != source.agent_session_id
                || session.config_dir != source.config_dir
            {
                continue;
            }
            let context = update
                .context
                .as_ref()
                .filter(|context| &session.context != *context);
            let titles = update
                .titles
                .as_ref()
                .filter(|titles| &session.record_titles != *titles);
            let boundary: Option<String> = tx.query_row(
                "SELECT observed_boundary FROM sessions WHERE id = ?1",
                params![source.id],
                |row| row.get(0),
            )?;
            let current_observations = boundary == update.observed_boundary;
            let observed_model = update.observed_model.as_ref().filter(|model| {
                current_observations && session.observed_model.as_ref() != Some(*model)
            });
            let observed_effort = update
                .observed_effort
                .as_ref()
                .filter(|effort| current_observations && &session.observed_effort != *effort);
            if context.is_none()
                && titles.is_none()
                && observed_model.is_none()
                && observed_effort.is_none()
            {
                continue;
            }
            if let Some(context) = context {
                tx.execute(
                    "UPDATE sessions SET context = ?2 WHERE id = ?1",
                    params![
                        source.id,
                        context.as_ref().map(serde_json::to_string).transpose()?
                    ],
                )?;
                session.context = context.clone();
            }
            if let Some(titles) = titles {
                tx.execute(
                    "UPDATE sessions SET record_title = ?2, record_prompt = ?3 WHERE id = ?1",
                    params![source.id, titles.named, titles.prompt],
                )?;
                session.record_titles = titles.clone();
            }
            if let Some(model) = observed_model {
                tx.execute(
                    "UPDATE sessions SET observed_model = ?2 WHERE id = ?1",
                    params![source.id, model],
                )?;
                session.observed_model = Some(model.clone());
            }
            if let Some(effort) = observed_effort {
                tx.execute(
                    "UPDATE sessions SET observed_effort = ?2 WHERE id = ?1",
                    params![source.id, effort],
                )?;
                session.observed_effort = effort.clone();
            }
            changed.push(session);
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Writes only the terminal title, and skips unchanged, no-process or deleted rows. Shutdown
    /// cleanup may still emit OSC titles after archiving or interruption; keep the last active
    /// title instead. The read and write share the store lock with status updates, so that
    /// boundary and concurrent renames are preserved.
    pub fn set_terminal_title(&self, id: &str, title: Option<&str>) -> Result<Option<Session>> {
        let conn = self.lock();
        let session = conn
            .query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                params![id],
                read_session,
            )
            .optional()?;
        let Some(mut session) = session else {
            return Ok(None);
        };
        if session.status.has_no_process() || session.terminal_title.as_deref() == title {
            return Ok(None);
        }
        conn.execute(
            "UPDATE sessions SET terminal_title = ?2 WHERE id = ?1",
            params![id, title],
        )?;
        session.terminal_title = title.map(str::to_owned);
        Ok(Some(session))
    }

    /// Moves a session to `status`, but only while it is still `expected`, and returns the record
    /// as it now stands when it moved. Both statements run under one lock, so a caller whose write
    /// is only valid while the session has not moved on cannot be overtaken between deciding and
    /// writing — which a write after a separate [`Self::get_session`] can be.
    /// `None` means the session was something other than `expected`, or is gone. Asking for the
    /// status it already has counts as a move, so a caller that would gain nothing from that should
    /// not ask. As with [`Self::set_running_status`], any status but `Idle` clears
    /// `last_turn_abnormal`.
    pub fn update_session_status_if(
        &self,
        id: &str,
        expected: SessionStatus,
        status: SessionStatus,
    ) -> Result<Option<Session>> {
        self.move_session_status_if(id, expected, status, None)
    }

    /// [`Self::update_session_status_if`] to `Idle` for a turn that ended abnormally, recording
    /// that in the same write.
    pub fn pause_session_if(&self, id: &str, expected: SessionStatus) -> Result<Option<Session>> {
        self.move_session_status_if(id, expected, SessionStatus::Idle, Some(true))
    }

    /// The write of [`Self::update_session_status_if`], setting `last_turn_abnormal` to `abnormal`
    /// where one is given.
    fn move_session_status_if(
        &self,
        id: &str,
        expected: SessionStatus,
        status: SessionStatus,
        abnormal: Option<bool>,
    ) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET status = ?3,
                                 last_turn_abnormal = CASE WHEN ?5 IS NOT NULL THEN ?5
                                                           WHEN ?3 = ?4 THEN last_turn_abnormal
                                                           ELSE 0 END
             WHERE id = ?1 AND status = ?2",
            params![
                id,
                enum_to_text(&expected),
                enum_to_text(&status),
                enum_to_text(&SessionStatus::Idle),
                abnormal,
            ],
        )
    }

    /// Changes a launch attempt's status and end time together, only while its expected status
    /// still holds. A later archive wins over an optimistic relaunch or its failure rollback.
    pub fn update_session_lifecycle_if(
        &self,
        id: &str,
        expected: SessionStatus,
        status: SessionStatus,
        ended_at: Option<i64>,
    ) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET status = ?3, ended_at = ?4 WHERE id = ?1 AND status = ?2",
            params![id, enum_to_text(&expected), enum_to_text(&status), ended_at],
        )
    }

    /// Records the account a session runs under and its directory, together and by nothing else:
    /// the one place either is written after the session is created. Returns the record as it now
    /// stands, or `None` when the session is gone.
    pub fn set_session_account(
        &self,
        id: &str,
        account_id: Option<&str>,
        config_dir: Option<&str>,
    ) -> Result<Option<Session>> {
        self.update_session_row(
            id,
            "UPDATE sessions SET account_id = ?2, config_dir = ?3 WHERE id = ?1",
            params![id, account_id, config_dir],
        )
    }

    pub fn set_session_pinned(&self, id: &str, pinned: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET pinned = ?2 WHERE id = ?1",
            params![id, pinned],
        )?;
        Ok(())
    }

    /// Replaces the session's model and effort overrides, each as `Session::model` states it: absent
    /// inherits, `Some(None)` leaves the choice to the agent, a value pins it. Nothing else writes
    /// these columns.
    pub fn set_session_launch_overrides(
        &self,
        id: &str,
        model: &Option<Option<String>>,
        effort: &Option<Option<String>>,
    ) -> Result<()> {
        self.write_session_launch_overrides(id, model, effort, false, None)
            .map(drop)
    }

    /// Records a session's model choice only while the user has not archived it. With `boundary`, a
    /// model change's observation boundary (`records::collector::ObservedBoundary`), it also stores
    /// that and clears the observed model and effort, in the same write.
    pub(crate) fn set_session_launch_overrides_unless_archived(
        &self,
        id: &str,
        model: &Option<Option<String>>,
        effort: &Option<Option<String>>,
        boundary: Option<&str>,
    ) -> Result<bool> {
        self.write_session_launch_overrides(id, model, effort, true, boundary)
    }

    fn write_session_launch_overrides(
        &self,
        id: &str,
        model: &Option<Option<String>>,
        effort: &Option<Option<String>>,
        unless_archived: bool,
        boundary: Option<&str>,
    ) -> Result<bool> {
        Ok(self.lock().execute(
            "UPDATE sessions SET model = ?2, effort = ?3, model_override = ?4, effort_override = ?5,
                observed_boundary = COALESCE(?7, observed_boundary),
                observed_model = CASE WHEN ?7 IS NULL THEN observed_model ELSE NULL END,
                observed_effort = CASE WHEN ?7 IS NULL THEN observed_effort ELSE NULL END
             WHERE id = ?1 AND (?6 = 0 OR status != ?8)",
            params![
                id,
                model.as_ref().and_then(|value| value.as_deref()),
                effort.as_ref().and_then(|value| value.as_deref()),
                model.is_some(),
                effort.is_some(),
                unless_archived,
                boundary,
                enum_to_text(&SessionStatus::Archived),
            ],
        )? > 0)
    }

    /// Every session's observation boundary that is set, by session id: the collector's private
    /// record of what the agent's records held when a model change last restarted the session
    /// (`records::collector::ObservedBoundary`), retained with this conversation across daemon
    /// restarts and archives. It is never part of a session's public record or a configuration.
    /// One query for a whole collector pass.
    pub(crate) fn observed_boundaries(&self) -> Result<std::collections::HashMap<String, String>> {
        let conn = self.lock();
        let mut statement = conn.prepare(
            "SELECT id, observed_boundary FROM sessions WHERE observed_boundary IS NOT NULL",
        )?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// One session's observation boundary (see [`Self::observed_boundaries`]).
    #[cfg(test)]
    pub(crate) fn observed_boundary(&self, id: &str) -> Result<Option<String>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT observed_boundary FROM sessions WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten())
    }

    #[cfg(test)]
    pub(crate) fn reset_observed_model(&self, id: &str, boundary: &str) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET observed_boundary = ?2,
            observed_model = NULL, observed_effort = NULL WHERE id = ?1",
            params![id, boundary],
        )?;
        Ok(())
    }

    /// Sets the colour of a console session. Like `pinned`, it is the user's own statement, which
    /// no other write touches.
    pub fn set_session_colour(&self, id: &str, colour: ConsoleSessionColour) -> Result<()> {
        self.lock().execute(
            "UPDATE sessions SET colour = ?2 WHERE id = ?1 AND kind = ?3",
            params![
                id,
                enum_to_text(&colour),
                enum_to_text(&SessionKind::Console)
            ],
        )?;
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.lock()
            .execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Deletes the session only while it is archived and returns the record that was removed.
    pub fn delete_session_if_archived(&self, id: &str) -> Result<Option<Session>> {
        Ok(self
            .lock()
            .query_row(
                &format!("DELETE FROM sessions WHERE id = ?1 AND status = ?2 RETURNING {SESSION_COLUMNS}"),
                params![id, enum_to_text(&SessionStatus::Archived)],
                read_session,
            )
            .optional()?)
    }

    /// Deletes the session only while it is unused: it has had no turn, has no session bound to it
    /// and is either bound to no owner or was opened by the user (one an agent started was
    /// dispatched by its owner, which expects it to exist, so it is archived instead), whatever
    /// its status. The returned identity comes from the same statement as the delete, so
    /// native-record cleanup cannot use an earlier hook snapshot.
    pub fn delete_session_if_unused(&self, id: &str) -> Result<Option<Session>> {
        Ok(self
            .lock()
            .query_row(
                &format!(
                    "DELETE FROM sessions
                     WHERE id = ?1 AND has_conversation = 0
                       AND (bound_to IS NULL OR origin = ?2)
                       AND NOT EXISTS (SELECT 1 FROM sessions WHERE bound_to = ?1)
                     RETURNING {SESSION_COLUMNS}"
                ),
                params![id, enum_to_text(&Origin::User)],
                read_session,
            )
            .optional()?)
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let conn = self.lock();
        let session = conn
            .query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                params![id],
                read_session,
            )
            .optional()?;
        Ok(session)
    }

    /// The session `id`, or the `unknown_session` refusal when there is none.
    pub fn require_session(&self, id: &str) -> Result<Session> {
        self.get_session(id)?
            .ok_or_else(|| CodedError::unknown_session(id).into())
    }

    /// A session other than `except` that records `agent_session_id` as its agent's conversation
    /// id, live or archived and of any project, if there is one.
    pub fn session_by_agent_session_id(
        &self,
        agent_session_id: &str,
        except: Option<&str>,
    ) -> Result<Option<Session>> {
        let conn = self.lock();
        let session = conn
            .query_row(
                &format!(
                    "SELECT {SESSION_COLUMNS} FROM sessions
                     WHERE agent_session_id = ?1 AND id IS NOT ?2 LIMIT 1"
                ),
                params![agent_session_id, except],
                read_session,
            )
            .optional()?;
        Ok(session)
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {SESSION_COLUMNS} FROM sessions ORDER BY started_at"
        ))?;
        let rows = stmt
            .query_map([], read_session)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Moves every session that a previous daemon left mid-flight to `interrupted`. The daemon
    /// owns the agent processes, so none of them survived its exit — whatever the stored status
    /// says — and a turn one was in was cut short. Archived sessions are already without a process
    /// on purpose and are left alone. Only the stored status is left to judge a turn on: a session
    /// request's hand was the previous daemon's memory, so one it held over an agent at its prompt
    /// when it went down without stopping its sessions reads as a turn cut short.
    pub fn mark_live_sessions_interrupted(&self) -> Result<usize> {
        let changed = self.lock().execute(
            &format!(
                "UPDATE sessions SET status = ?1, ended_at = ?2, {}
                 WHERE status NOT IN (?1, ?3)",
                cut_short_turn("NULL")
            ),
            params![
                enum_to_text(&SessionStatus::Interrupted),
                crate::time::now_millis(),
                enum_to_text(&SessionStatus::Archived),
            ],
        )?;
        Ok(changed)
    }
}

/// The colour for a new console session: the entry `roll` selects among the palette entries not
/// in `held` (the other non-archived console sessions of its console), or among the whole palette
/// once every entry is held. `roll` is reduced modulo the number of candidates, so any uniformly
/// random number gives a uniform pick; see `Store::insert_console_session`.
fn assign_console_session_colour(
    held: &[ConsoleSessionColour],
    roll: usize,
) -> ConsoleSessionColour {
    let palette = ConsoleSessionColour::PALETTE;
    let free: Vec<ConsoleSessionColour> = palette
        .iter()
        .copied()
        .filter(|candidate| !held.contains(candidate))
        .collect();
    let candidates: &[ConsoleSessionColour] = if free.is_empty() { &palette } else { &free };
    candidates[roll % candidates.len()]
}

/// Shared by [`Store::insert_session`] and [`Store::insert_console_session`], which differ only in
/// how `colour`, `ordinal` and `title` are decided before this runs.
fn insert_session_row(conn: &Connection, session: &Session) -> Result<()> {
    conn.execute(
        "INSERT INTO sessions
            (id, agent, agent_session_id, console_id, project_id, host_id, kind, origin, title,
             status, has_conversation, bound_to, colour, ordinal, account_id, config_dir, pinned,
             started_at, ended_at, role, terminal_title, model, effort, model_override,
             effort_override, context, record_title, record_prompt, locked, observed_model, observed_effort,
             last_turn_abnormal)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                 ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32)",
        params![
            session.id,
            enum_to_text(&session.agent),
            session.agent_session_id,
            session.console_id,
            session.project_id,
            session.host_id,
            enum_to_text(&session.kind),
            enum_to_text(&session.origin),
            session.title,
            enum_to_text(&session.status),
            session.has_conversation,
            session.bound_to,
            session.colour.as_ref().map(enum_to_text),
            session.ordinal,
            session.account_id,
            session.config_dir,
            session.pinned,
            session.started_at,
            session.ended_at,
            enum_to_text(&session.role),
            session.terminal_title,
            session.model.as_ref().and_then(|value| value.as_deref()),
            session.effort.as_ref().and_then(|value| value.as_deref()),
            session.model.is_some(),
            session.effort.is_some(),
            session.context.as_ref().map(serde_json::to_string).transpose()?,
            session.record_titles.named,
            session.record_titles.prompt,
            session.locked,
            session.observed_model,
            session.observed_effort,
            session.paused,
        ],
    )?;
    Ok(())
}

pub(super) fn read_session(row: &Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get(0)?,
        agent: enum_from_row::<Agent>(row, 1)?,
        agent_session_id: row.get(2)?,
        console_id: row.get(3)?,
        project_id: row.get(4)?,
        host_id: row.get(5)?,
        kind: enum_from_row::<SessionKind>(row, 6)?,
        origin: enum_from_row::<Origin>(row, 7)?,
        title: row.get(8)?,
        status: enum_from_row::<SessionStatus>(row, 9)?,
        background_tasks: Vec::new(),
        switching_account: false,
        paused: row.get(31)?,
        prompt_on_launch: false,
        default_effort: None,
        role: enum_from_row::<Role>(row, 19)?,
        has_conversation: row.get(10)?,
        bound_to: row.get(11)?,
        colour: enum_from_row_opt::<ConsoleSessionColour>(row, 12)?,
        ordinal: row.get(13)?,
        account_id: row.get(14)?,
        config_dir: row.get(15)?,
        pinned: row.get(16)?,
        locked: row.get(28)?,
        observed_model: row.get(29)?,
        observed_effort: row.get(30)?,
        started_at: row.get(17)?,
        ended_at: row.get(18)?,
        terminal_title: row.get(20)?,
        record_titles: RecordTitles {
            named: row.get(26)?,
            prompt: row.get(27)?,
        },
        context: row
            .get::<_, Option<String>>(25)?
            .and_then(|text| serde_json::from_str(&text).ok()),
        model: row.get::<_, bool>(23)?.then(|| row.get(21)).transpose()?,
        effort: row.get::<_, bool>(24)?.then(|| row.get(22)).transpose()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{error_code, error_parts};

    use crate::store::test_support::*;

    /// A session that is stored is read back, and one that is not is refused with the
    /// `unknown_session` error naming the id that was asked for.
    #[test]
    fn requiring_a_session_reads_a_stored_one_and_refuses_an_unknown_one_with_its_code() {
        let path = temp_db("require-session");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");
        store.insert_session(&session(None)).expect("session");

        assert_eq!(
            store.require_session("session-1").expect("stored").id,
            "session-1"
        );

        let refused: anyhow::Error = store.require_session("nope").expect_err("unknown").into();
        assert_eq!(
            error_parts(&refused),
            error_parts(&CodedError::unknown_session("nope"))
        );
        assert_eq!(error_parts(&refused).0, error_code::UNKNOWN_SESSION);
        assert_eq!(error_parts(&refused).2, "unknown session nope");
    }

    /// A session without overrides inherits. Overrides, including explicit nulls, survive
    /// unrelated writes, account changes and opening the database again.
    #[test]
    fn session_launch_overrides_survive_reopening() {
        let path = temp_db("session-launch-overrides");
        {
            let store = Store::open(&path).unwrap();
            store.insert_console(&console(None, None, None)).unwrap();
            store.insert_session(&session(None)).unwrap();
        }
        let store = Store::open(&path).unwrap();
        let kept = store.get_session("session-1").unwrap().unwrap();
        assert_eq!((kept.model, kept.effort), (None, None));
        let cases = [
            (Some(Some("opus".to_string())), Some(None)),
            (Some(None), Some(Some("high".to_string()))),
            (None, Some(Some("medium".to_string()))),
            (Some(Some("sonnet".to_string())), None),
        ];
        for (index, (model, effort)) in cases.iter().enumerate() {
            let mut opened = session(None);
            opened.id = format!("override-{index}");
            opened.model = model.clone();
            opened.effort = effort.clone();
            let mut stored = store.insert_console_session(opened, None).unwrap();
            stored.status = SessionStatus::Archived;
            stored.model = None;
            stored.effort = None;
            store.update_session(&stored).unwrap();
            store
                .set_session_account(&stored.id, Some("other"), Some("/other"))
                .unwrap();
        }
        drop(store);
        let store = Store::open(&path).unwrap();
        for (index, expected) in cases.into_iter().enumerate() {
            let stored = store
                .get_session(&format!("override-{index}"))
                .unwrap()
                .unwrap();
            assert_eq!((stored.model, stored.effort), expected);
            assert_eq!(stored.status, SessionStatus::Archived);
            assert_eq!(stored.config_dir.as_deref(), Some("/other"));
        }
    }

    /// A reporter whose conclusion only holds while the session has not moved on writes through
    /// this, so the check and the write must be one step: it moves the session only from the status
    /// it expected, and says which happened.
    #[test]
    fn a_conditional_status_write_moves_the_session_only_from_the_status_it_expected() {
        let path = temp_db("conditional-status");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut waiting = session(None);
        waiting.status = SessionStatus::WaitingUser;
        store.insert_session(&waiting).expect("insert");

        // From the expected status it moves, and hands back the record as it now stands.
        let moved = store
            .update_session_status_if("session-1", SessionStatus::WaitingUser, SessionStatus::Idle)
            .expect("conditional update");
        assert_eq!(moved.map(|s| s.status), Some(SessionStatus::Idle));

        // The same write a second time finds the session already moved on and leaves it alone —
        // this is the stale reporter whose conclusion no longer holds.
        let again = store
            .update_session_status_if("session-1", SessionStatus::WaitingUser, SessionStatus::Idle)
            .expect("conditional update");
        assert!(again.is_none());

        // And a status the session is not in cannot be overwritten by one that expects it.
        store
            .update_session_status_if("session-1", SessionStatus::Idle, SessionStatus::Working)
            .expect("conditional update")
            .expect("the session was idle, so this one moves it");
        let stale = store
            .update_session_status_if("session-1", SessionStatus::WaitingUser, SessionStatus::Idle)
            .expect("conditional update");
        assert!(stale.is_none());
        assert_eq!(
            store.get_session("session-1").unwrap().unwrap().status,
            SessionStatus::Working,
            "a write whose expected status did not hold must not have landed"
        );

        // An id that is not there is simply no move, not an error.
        assert!(store
            .update_session_status_if("nobody", SessionStatus::WaitingUser, SessionStatus::Idle)
            .expect("conditional update")
            .is_none());
    }

    /// Each write of a session's changing fields writes its own columns, so writers on tasks of
    /// their own — a rename, the hook that names the conversation, the one that records its first
    /// turn — cannot undo each other or the user's pin; and a write to a deleted session reports
    /// that nothing was written.
    #[test]
    fn session_writes_leave_each_others_columns_alone() {
        let path = temp_db("session-columns");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_session(&session(None)).expect("insert");

        store.set_session_pinned("session-1", true).unwrap();
        assert!(store
            .set_agent_session_id("session-1", "conversation-1")
            .unwrap()
            .is_some());
        assert!(store
            .mark_conversation_started("session-1")
            .unwrap()
            .is_some());
        let renamed = store
            .set_session_title("session-1", "Renamed")
            .unwrap()
            .expect("renamed");
        assert_eq!(renamed.agent_session_id.as_deref(), Some("conversation-1"));
        assert!(renamed.has_conversation);
        assert!(renamed.pinned);
        let running = store
            .set_running_status("session-1", SessionStatus::Working)
            .unwrap()
            .expect("working");
        assert_eq!(running.title, "Renamed");
        assert_eq!(running.agent_session_id.as_deref(), Some("conversation-1"));

        // Nothing to change is no write.
        assert!(store
            .set_agent_session_id("session-1", "conversation-1")
            .unwrap()
            .is_none());
        assert!(store
            .mark_conversation_started("session-1")
            .unwrap()
            .is_none());
        assert!(store
            .set_running_status("session-1", SessionStatus::Working)
            .unwrap()
            .is_none());

        assert!(store
            .archive_session("session-1", 1, None)
            .unwrap()
            .is_some());
        assert!(store
            .delete_session_if_archived("session-1")
            .unwrap()
            .is_some());
        assert!(store
            .set_session_title("session-1", "Gone")
            .unwrap()
            .is_none());
        assert!(store
            .archive_session("session-1", 2, None)
            .unwrap()
            .is_none());
        assert!(store
            .record_session_exit("session-1", 2, None)
            .unwrap()
            .is_none());
        drop(store);
    }

    /// A running status reported after the session lost its process — a Stop hook landing after
    /// an archive, a late event after the process ended — leaves it as it was; and the process
    /// going away keeps an archive while interrupting anything else.
    #[test]
    fn a_no_process_session_stays_so_whatever_its_agent_reports() {
        let path = temp_db("session-no-process");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_session(&session(None)).expect("insert");
        store
            .set_running_status("session-1", SessionStatus::Working)
            .unwrap()
            .expect("working");

        let (before, archived) = store
            .archive_session("session-1", 10, None)
            .unwrap()
            .expect("archived");
        assert_eq!(before, SessionStatus::Working);
        assert_eq!(archived.status, SessionStatus::Archived);
        assert!(store
            .set_running_status("session-1", SessionStatus::Idle)
            .unwrap()
            .is_none());
        assert!(store
            .interrupt_session("session-1", 20, None)
            .unwrap()
            .is_none());
        let (before, exited) = store
            .record_session_exit("session-1", 20, None)
            .unwrap()
            .expect("exited");
        assert_eq!(before, SessionStatus::Archived);
        assert_eq!(exited.status, SessionStatus::Archived);
        assert_eq!(exited.ended_at, Some(20));

        store
            .update_session_lifecycle_if(
                "session-1",
                SessionStatus::Archived,
                SessionStatus::Working,
                None,
            )
            .unwrap()
            .expect("relaunched");
        let (before, exited) = store
            .record_session_exit("session-1", 30, None)
            .unwrap()
            .expect("exited");
        assert_eq!(before, SessionStatus::Working);
        assert_eq!(exited.status, SessionStatus::Interrupted);
        assert_eq!(exited.ended_at, Some(30));
        assert!(store
            .set_running_status("session-1", SessionStatus::WaitingUser)
            .unwrap()
            .is_none());
        drop(store);
    }

    /// Whether the last turn ended abnormally survives the store being opened again and the
    /// relaunch back to the prompt; a status off the prompt clears it, `Idle` keeps it, and a
    /// session without a process takes no flag of a turn.
    #[test]
    fn how_the_last_turn_ended_is_stored_and_cleared_off_the_prompt() {
        let path = temp_db("session-last-turn");
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
            store.insert_session(&session(None)).expect("insert");
            let paused = store
                .set_last_turn_abnormal("session-1", true)
                .unwrap()
                .expect("set");
            assert!(paused.paused);
            assert!(
                store
                    .set_last_turn_abnormal("session-1", true)
                    .unwrap()
                    .is_none(),
                "an unchanged flag is no change"
            );
            store.interrupt_session("session-1", 10, None).unwrap();
            assert!(
                store
                    .set_last_turn_abnormal("session-1", false)
                    .unwrap()
                    .is_none(),
                "a session without a process takes no flag"
            );
        }
        let store = Store::open(&path).expect("reopened");
        let relaunched = store
            .update_session_lifecycle_if(
                "session-1",
                SessionStatus::Interrupted,
                SessionStatus::Idle,
                None,
            )
            .unwrap()
            .expect("relaunched");
        assert!(relaunched.paused, "the flag outlives the process");

        // Idle keeps it; any status off the prompt clears it, in either status write.
        store
            .set_running_status("session-1", SessionStatus::WaitingUser)
            .unwrap();
        assert!(!store.require_session("session-1").unwrap().paused);
        store.set_last_turn_abnormal("session-1", true).unwrap();
        store
            .set_running_status("session-1", SessionStatus::Idle)
            .unwrap();
        assert!(store.require_session("session-1").unwrap().paused);
        let working = store
            .update_session_status_if("session-1", SessionStatus::Idle, SessionStatus::Working)
            .unwrap()
            .expect("moved");
        assert!(!working.paused);

        // A conditional pause sets it with the move, and only when the session moved.
        assert!(store
            .pause_session_if("session-1", SessionStatus::WaitingUser)
            .unwrap()
            .is_none());
        assert!(!store.require_session("session-1").unwrap().paused);
        let paused = store
            .pause_session_if("session-1", SessionStatus::Working)
            .unwrap()
            .expect("moved");
        assert_eq!(paused.status, SessionStatus::Idle);
        assert!(paused.paused);
        let idle = store
            .update_session_status_if("session-1", SessionStatus::Idle, SessionStatus::Idle)
            .unwrap()
            .expect("moved");
        assert!(idle.paused, "a move to idle keeps it");

        // A hand raised over the agent is not the agent leaving its prompt.
        let held = store.hold_waiting_user("session-1").unwrap().expect("held");
        assert_eq!(held.status, SessionStatus::WaitingUser);
        assert!(held.paused, "a request's hand keeps it");

        // A conversation forgotten takes how its last turn ended with it.
        let forgotten = store
            .forget_conversation("session-1")
            .unwrap()
            .expect("forgotten");
        assert!(!forgotten.paused);
    }

    /// Each way a process ends records a turn in progress — the agent working, or waiting on the
    /// user inside it — as cut short, and leaves the flag as the last turn left it at any other
    /// status.
    #[test]
    fn a_process_ending_mid_turn_records_the_turn_as_cut_short() {
        type End = fn(&Store);
        let ends: [(&str, End); 4] = [
            ("exit", |store| {
                store.record_session_exit("session-1", 10, None).unwrap();
            }),
            ("interrupt", |store| {
                store.interrupt_session("session-1", 10, None).unwrap();
            }),
            ("archive", |store| {
                store.archive_session("session-1", 10, None).unwrap();
            }),
            ("daemon gone", |store| {
                store.mark_live_sessions_interrupted().unwrap();
            }),
        ];
        for (label, end) in ends {
            for (status, before, after) in [
                (SessionStatus::Working, false, true),
                (SessionStatus::WaitingUser, false, true),
                (SessionStatus::Idle, false, false),
                (SessionStatus::Idle, true, true),
                (SessionStatus::Starting, false, false),
            ] {
                let path = temp_db("session-cut-short");
                let store = Store::open(&path).expect("store");
                store
                    .insert_console(&console(None, None, None))
                    .expect("console");
                store
                    .insert_session(&Session {
                        status,
                        paused: before,
                        ..session(None)
                    })
                    .expect("insert");
                end(&store);
                let ended = store.require_session("session-1").unwrap();
                assert!(ended.status.has_no_process(), "{label} {status:?}");
                assert_eq!(ended.paused, after, "{label} {status:?} {before}");
            }
        }
    }

    /// A late clear of a turn cut short holds only while the record is as that end left it: not
    /// for another end, and not once the session is running again.
    #[test]
    fn a_turn_cut_short_is_cleared_only_while_the_record_stands_as_its_end_left_it() {
        let path = temp_db("session-cut-short-clear");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store
            .insert_session(&Session {
                status: SessionStatus::Working,
                ..session(None)
            })
            .expect("insert");
        store.record_session_exit("session-1", 10, None).unwrap();
        assert!(store
            .clear_cut_short_turn("session-1", 9)
            .unwrap()
            .is_none());
        let cleared = store
            .clear_cut_short_turn("session-1", 10)
            .unwrap()
            .expect("cleared");
        assert!(!cleared.paused);

        store
            .update_session_lifecycle_if(
                "session-1",
                SessionStatus::Interrupted,
                SessionStatus::Idle,
                None,
            )
            .unwrap()
            .expect("relaunched");
        store.set_last_turn_abnormal("session-1", true).unwrap();
        assert!(store
            .clear_cut_short_turn("session-1", 10)
            .unwrap()
            .is_none());
    }

    /// Where the agent's own status is given apart from the stored one — a request's hand holding
    /// the record at waiting for the user — the end judges the turn on the agent's.
    #[test]
    fn a_process_ending_judges_the_turn_on_the_agents_own_status_where_it_is_given() {
        type End = fn(&Store, Option<SessionStatus>);
        let ends: [(&str, End); 3] = [
            ("exit", |store, own| {
                store.record_session_exit("session-1", 10, own).unwrap();
            }),
            ("interrupt", |store, own| {
                store.interrupt_session("session-1", 10, own).unwrap();
            }),
            ("archive", |store, own| {
                store.archive_session("session-1", 10, own).unwrap();
            }),
        ];
        for (label, end) in ends {
            for (own, after) in [
                (SessionStatus::Idle, false),
                (SessionStatus::Working, true),
                (SessionStatus::WaitingUser, true),
            ] {
                let path = temp_db("session-cut-short-own");
                let store = Store::open(&path).expect("store");
                store
                    .insert_console(&console(None, None, None))
                    .expect("console");
                store
                    .insert_session(&Session {
                        status: SessionStatus::WaitingUser,
                        ..session(None)
                    })
                    .expect("insert");
                end(&store, Some(own));
                let ended = store.require_session("session-1").unwrap();
                assert_eq!(ended.paused, after, "{label} {own:?}");
            }
        }
    }

    /// Information held for an interrupted session is read oldest first, survives the store being
    /// opened again, is dropped by id or for the whole session, and goes with the session.
    #[test]
    fn information_held_for_a_session_survives_reopening_and_goes_with_the_session() {
        let path = temp_db("pending-info");
        {
            let store = Store::open(&path).expect("store");
            store
                .insert_console(&console(None, None, None))
                .expect("console");
            store.insert_session(&session(None)).expect("session");
            store.add_kept_message("session-1", "one").unwrap();
            store.add_kept_message("session-1", "two").unwrap();
        }
        let store = Store::open(&path).expect("reopened");
        let held = store.kept_messages("session-1").unwrap();
        assert_eq!(
            held.iter()
                .map(|(_, text)| text.as_str())
                .collect::<Vec<_>>(),
            ["one", "two"]
        );
        store.delete_kept_messages(&[held[0].0]).unwrap();
        assert_eq!(store.kept_messages("session-1").unwrap().len(), 1);
        store.add_kept_message("session-1", "three").unwrap();
        store.clear_kept_messages("session-1").unwrap();
        assert!(store.kept_messages("session-1").unwrap().is_empty());
        store.add_kept_message("session-1", "four").unwrap();
        store.delete_session("session-1").unwrap();
        assert!(store.kept_messages("session-1").unwrap().is_empty());
    }

    fn open_console_session(store: &Store, id: &str) -> ConsoleSessionColour {
        store
            .insert_console_session(console_session(id), None)
            .expect("inserted")
            .colour
            .expect("a console session always gets a colour")
    }

    /// While a free palette entry remains, a new console session never takes one a live console
    /// session of its console holds: opening as many as the palette has entries hands out each
    /// entry exactly once, whatever the random picks were.
    #[test]
    fn a_console_sessions_colour_is_never_one_already_held_while_one_is_free() {
        let path = temp_db("colour-assignment");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");

        let palette = ConsoleSessionColour::PALETTE;
        let mut assigned: Vec<ConsoleSessionColour> = (0..palette.len())
            .map(|i| open_console_session(&store, &format!("session-{i}")))
            .collect();
        assigned.sort_by_key(|colour| palette.iter().position(|entry| entry == colour));
        assert_eq!(assigned, palette);
    }

    /// The pick is uniform over the free entries: every roll lands on a free entry, and the rolls
    /// together reach each of them.
    #[test]
    fn assign_picks_only_among_the_free_entries() {
        let palette = ConsoleSessionColour::PALETTE;
        let held = [palette[0], palette[palette.len() - 1]];
        let picks: Vec<ConsoleSessionColour> = (0..palette.len() * 3)
            .map(|roll| assign_console_session_colour(&held, roll))
            .collect();
        assert!(picks.iter().all(|pick| !held.contains(pick)));
        for entry in palette {
            assert_eq!(
                picks.contains(&entry),
                !held.contains(&entry),
                "{entry:?} is reachable exactly when it is free"
            );
        }
    }

    /// Once every palette entry is held, assignment picks among all of them rather than refusing
    /// — the badge is a convenience, not a promise of uniqueness once the console runs more
    /// console sessions at once than the palette has room for.
    #[test]
    fn assign_picks_among_the_whole_palette_once_every_entry_is_held() {
        let held = ConsoleSessionColour::PALETTE;
        let picks: Vec<ConsoleSessionColour> = (0..held.len())
            .map(|roll| assign_console_session_colour(&held, roll))
            .collect();
        assert_eq!(picks, held);

        let path = temp_db("colour-wrap");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        for i in 0..held.len() {
            open_console_session(&store, &format!("session-{i}"));
        }
        // With every entry held, the next open is still not refused and still gets a colour.
        open_console_session(&store, "session-overflow");
    }

    /// Archiving a console session frees its colour for reuse — only a *non-archived* console
    /// session's colour counts as held, since the badge only ever needs to tell apart the console
    /// sessions currently showing side by side.
    #[test]
    fn an_archived_console_sessions_colour_is_free_for_reuse() {
        let path = temp_db("colour-archive-frees");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let mut sessions: Vec<Session> = (0..ConsoleSessionColour::PALETTE.len())
            .map(|i| {
                store
                    .insert_console_session(console_session(&format!("session-{i}")), None)
                    .expect("inserted")
            })
            .collect();
        let mut archived = sessions.remove(0);
        let freed = archived.colour.expect("a colour");
        archived.status = SessionStatus::Archived;
        store.update_session(&archived).expect("archived");

        assert_eq!(
            open_console_session(&store, "session-next"),
            freed,
            "the archived console session's colour is the only free entry, so it is the pick"
        );
    }

    /// A colour the user chose replaces the assigned one, survives a stale write-back of the
    /// record, counts as held for the next console session, and is ignored for a project session.
    #[test]
    fn a_chosen_console_session_colour_is_kept_and_counts_as_held() {
        let path = temp_db("colour-chosen");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        let first = store
            .insert_console_session(console_session("session-0"), None)
            .expect("inserted");
        let chosen = ConsoleSessionColour::PALETTE
            .into_iter()
            .find(|entry| Some(*entry) != first.colour)
            .unwrap();

        store.set_session_colour("session-0", chosen).unwrap();
        assert!(store.update_session(&first).unwrap());
        let reread = store.get_session("session-0").unwrap().unwrap();
        assert_eq!(reread.colour, Some(chosen));

        // The other entries are all free but the chosen one: the next picks never take it.
        for i in 1..ConsoleSessionColour::PALETTE.len() {
            assert_ne!(
                open_console_session(&store, &format!("session-{i}")),
                chosen,
                "the chosen colour is held"
            );
        }

        let mut project_session = session(None);
        project_session.id = "session-project".to_string();
        project_session.kind = SessionKind::Project;
        store.insert_session(&project_session).expect("insert");
        store.set_session_colour("session-project", chosen).unwrap();
        assert_eq!(
            store
                .get_session("session-project")
                .unwrap()
                .unwrap()
                .colour,
            None
        );
    }

    /// A console session opened in a console that is gone is refused as `unknown_console`, not
    /// reported as an internal failure.
    #[test]
    fn a_console_session_in_a_missing_console_is_an_unknown_console() {
        let path = temp_db("console-session-no-console");
        let store = Store::open(&path).expect("store");

        let refused: anyhow::Error = store
            .insert_console_session(console_session("session-0"), None)
            .expect_err("no such console")
            .into();
        assert_eq!(error_parts(&refused).0, error_code::UNKNOWN_CONSOLE);
        assert_eq!(error_parts(&refused).2, "unknown console console-1");
    }

    /// The ordinal is one past the highest ever handed out in this console, kept on the console
    /// record itself — not derived from which console sessions still exist — so deleting the one
    /// that held the highest ordinal does not let a later console session reuse it.
    #[test]
    fn a_console_sessions_ordinal_is_never_reused_even_after_its_session_is_deleted() {
        let path = temp_db("ordinal-high-water-mark");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");

        let first = store
            .insert_console_session(console_session("session-0"), None)
            .expect("inserted");
        let second = store
            .insert_console_session(console_session("session-1"), None)
            .expect("inserted");
        assert_eq!(first.ordinal, Some(1));
        assert_eq!(second.ordinal, Some(2));

        let mut archived = second;
        archived.status = SessionStatus::Archived;
        store.update_session(&archived).expect("archived");
        assert!(store
            .delete_session_if_archived(&archived.id)
            .expect("delete")
            .is_some());

        let third = store
            .insert_console_session(console_session("session-2"), None)
            .expect("inserted");
        assert_eq!(
            third.ordinal,
            Some(3),
            "ordinal 2 was used and must not be handed out again, even though session-1 is gone"
        );
    }

    /// Defaults use the console name, including for later sessions; an explicit title is kept.
    #[test]
    fn a_console_sessions_default_title_uses_its_console_name() {
        let path = temp_db("console-session-title");
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");

        let defaulted = store
            .insert_console_session(console_session("session-0"), None)
            .expect("inserted");
        assert_eq!(defaulted.title, "Console");

        let second = store
            .insert_console_session(console_session("session-0b"), None)
            .expect("inserted");
        assert_eq!(second.title, "Console");

        let mut renamed = console(None, None, None);
        renamed.name = "Renamed console".to_string();
        store.update_console(&renamed).expect("renamed");
        let after_rename = store
            .insert_console_session(console_session("session-0c"), None)
            .expect("inserted");
        assert_eq!(after_rename.title, "Renamed console");
        assert_eq!(
            store.get_session(&defaulted.id).unwrap().unwrap().title,
            "Console"
        );

        let named = store
            .insert_console_session(
                console_session("session-1"),
                Some("Investigate the outage".to_string()),
            )
            .expect("inserted");
        assert_eq!(named.title, "Investigate the outage");
    }

    #[test]
    fn observation_boundaries_reject_stale_models_and_follow_the_retained_conversation() {
        let path = temp_db("model-boundary-source");
        let store = Store::open(&path).unwrap();
        store.insert_console(&console(None, None, None)).unwrap();
        let mut source = session(None);
        store.insert_session(&source).unwrap();
        let boundary = r#"{"files":{}}"#;
        store.reset_observed_model(&source.id, boundary).unwrap();
        let update = |boundary: Option<&str>| RecordUpdate {
            session: source.clone(),
            context: Some(Some(crate::protocol::SessionContext {
                tokens: 123,
                window: None,
                used_percent: None,
            })),
            titles: Some(RecordTitles {
                named: Some("Current title".to_string()),
                prompt: None,
            }),
            observed_model: Some("new-model".to_string()),
            observed_effort: Some(Some("high".to_string())),
            observed_boundary: boundary.map(str::to_string),
        };
        store.set_record_data(&[update(None)]).unwrap();
        let after = store.get_session(&source.id).unwrap().unwrap();
        assert!(after.observed_model.is_none());
        assert_eq!(after.context.unwrap().tokens, 123);
        assert_eq!(after.record_titles.named.as_deref(), Some("Current title"));
        store.set_record_data(&[update(Some(boundary))]).unwrap();
        assert_eq!(
            store
                .get_session(&source.id)
                .unwrap()
                .unwrap()
                .observed_model
                .as_deref(),
            Some("new-model")
        );
        source.status = SessionStatus::Archived;
        store.update_session(&source).unwrap();
        assert_eq!(
            store.observed_boundary(&source.id).unwrap().as_deref(),
            Some(boundary)
        );
        source.agent_session_id = Some("another-conversation".to_string());
        store.update_session(&source).unwrap();
        assert!(store.observed_boundary(&source.id).unwrap().is_none());
        store.reset_observed_model(&source.id, boundary).unwrap();
        store.delete_session(&source.id).unwrap();
        assert!(store.observed_boundary(&source.id).unwrap().is_none());
    }

    #[test]
    fn context_survives_unrelated_writes_and_rejects_stale_sources() {
        let path = temp_db("context");
        let store = Store::open(&path).unwrap();
        store.insert_console(&console(None, None, None)).unwrap();
        let mut source = session(Some("/account-a"));
        source.agent_session_id = Some("native-1".to_owned());
        store.insert_session(&source).unwrap();
        assert!(store
            .get_session(&source.id)
            .unwrap()
            .unwrap()
            .context
            .is_none());
        let context = crate::protocol::SessionContext {
            tokens: 120,
            window: Some(475_000),
            used_percent: Some(2),
        };
        let update = |session: &Session, context| RecordUpdate {
            observed_boundary: None,
            session: session.clone(),
            context,
            observed_model: None,
            observed_effort: None,
            titles: None,
        };
        assert_eq!(
            store
                .set_record_data(&[update(&source, Some(Some(context.clone())))])
                .unwrap()
                .len(),
            1
        );
        assert!(store
            .set_record_data(&[update(&source, Some(Some(context.clone())))])
            .unwrap()
            .is_empty());
        source.title = "Renamed".to_owned();
        source.status = SessionStatus::Archived;
        store.update_session(&source).unwrap();
        assert_eq!(
            store.get_session(&source.id).unwrap().unwrap().context,
            Some(context.clone())
        );
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.get_session(&source.id).unwrap().unwrap().context,
            Some(context.clone())
        );
        store
            .set_session_account(&source.id, None, Some("/account-b"))
            .unwrap();
        let changed = crate::protocol::SessionContext {
            tokens: 999,
            ..context.clone()
        };
        assert!(store
            .set_record_data(&[update(&source, Some(Some(changed)))])
            .unwrap()
            .is_empty());
        let mut current = store.get_session(&source.id).unwrap().unwrap();
        // The records can also say the size is not known now, which clears it.
        assert_eq!(
            store
                .set_record_data(&[update(&current, Some(None))])
                .unwrap()
                .len(),
            1
        );
        assert!(store
            .get_session(&source.id)
            .unwrap()
            .unwrap()
            .context
            .is_none());
        store
            .set_record_data(&[update(&current, Some(Some(context.clone())))])
            .unwrap();
        current.agent_session_id = Some("native-2".to_owned());
        store.update_session(&current).unwrap();
        assert!(store
            .get_session(&source.id)
            .unwrap()
            .unwrap()
            .context
            .is_none());
        assert!(store
            .set_record_data(&[update(&source, Some(Some(context)))])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn record_titles_persist_and_follow_the_agent_session() {
        let path = temp_db("record-titles");
        let store = Store::open(&path).unwrap();
        store.insert_console(&console(None, None, None)).unwrap();
        store.insert_session(&session(None)).unwrap();
        let mut source = store.get_session("session-1").unwrap().unwrap();
        assert_eq!(source.record_titles, RecordTitles::default());
        let titles = RecordTitles {
            named: Some("Named".to_owned()),
            prompt: Some("Prompt".to_owned()),
        };
        let updates = [RecordUpdate {
            observed_boundary: None,
            session: source.clone(),
            context: None,
            observed_model: None,
            observed_effort: None,
            titles: Some(titles.clone()),
        }];
        assert_eq!(store.set_record_data(&updates).unwrap().len(), 1);
        assert!(store.set_record_data(&updates).unwrap().is_empty());
        source.title = "Renamed".to_owned();
        store.update_session(&source).unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store
                .get_session("session-1")
                .unwrap()
                .unwrap()
                .record_titles,
            titles
        );
        source.agent_session_id = Some("another".to_owned());
        store.update_session(&source).unwrap();
        assert_eq!(
            store
                .get_session("session-1")
                .unwrap()
                .unwrap()
                .record_titles,
            RecordTitles::default(),
            "another conversation does not inherit the titles"
        );
    }

    #[test]
    fn observed_model_and_effort_persist_and_follow_the_agent_session() {
        let path = temp_db("observed");
        let store = Store::open(&path).unwrap();
        store.insert_console(&console(None, None, None)).unwrap();
        store.insert_session(&session(None)).unwrap();
        let mut source = store.get_session("session-1").unwrap().unwrap();
        let updates = [RecordUpdate {
            observed_boundary: None,
            session: source.clone(),
            context: None,
            observed_model: Some("claude-opus-5-5".to_owned()),
            observed_effort: Some(Some("high".to_owned())),
            titles: None,
        }];
        assert_eq!(store.set_record_data(&updates).unwrap().len(), 1);
        assert!(store.set_record_data(&updates).unwrap().is_empty());
        source.title = "Renamed".to_owned();
        store.update_session(&source).unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        let stored = store.get_session("session-1").unwrap().unwrap();
        assert_eq!(stored.observed_model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(stored.observed_effort.as_deref(), Some("high"));
        let unset = [RecordUpdate {
            observed_boundary: None,
            session: stored,
            context: None,
            observed_model: None,
            observed_effort: Some(None),
            titles: None,
        }];
        assert_eq!(store.set_record_data(&unset).unwrap().len(), 1);
        let stored = store.get_session("session-1").unwrap().unwrap();
        assert_eq!(stored.observed_model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(stored.observed_effort, None);
        assert!(store.set_record_data(&unset).unwrap().is_empty());
        source.agent_session_id = Some("another".to_owned());
        store.update_session(&source).unwrap();
        let stored = store.get_session("session-1").unwrap().unwrap();
        assert_eq!(
            (stored.observed_model, stored.observed_effort),
            (None, None)
        );
    }

    #[test]
    fn terminal_titles_survive_unrelated_writes_and_reopening() {
        let path = temp_db("terminal-title");
        let store = Store::open(&path).unwrap();
        store.insert_console(&console(None, None, None)).unwrap();
        store.insert_session(&session(None)).unwrap();
        let mut stale = store.get_session("session-1").unwrap().unwrap();
        assert!(stale.terminal_title.is_none());
        assert!(store
            .set_terminal_title("session-1", Some("Task title"))
            .unwrap()
            .is_some());
        assert!(store
            .set_terminal_title("session-1", Some("Task title"))
            .unwrap()
            .is_none());
        stale.title = "Renamed".to_string();
        stale.status = SessionStatus::Archived;
        store.update_session(&stale).unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        let stored = store.get_session("session-1").unwrap().unwrap();
        assert_eq!(stored.terminal_title.as_deref(), Some("Task title"));
        assert_eq!(stored.title, "Renamed");
        assert_eq!(stored.status, SessionStatus::Archived);
        for status in [SessionStatus::Archived, SessionStatus::Interrupted] {
            let mut ended = store.get_session("session-1").unwrap().unwrap();
            ended.status = status;
            store.update_session(&ended).unwrap();
            for title in [None, Some("Shutdown title")] {
                assert!(store
                    .set_terminal_title("session-1", title)
                    .unwrap()
                    .is_none());
                assert_eq!(
                    store
                        .get_session("session-1")
                        .unwrap()
                        .unwrap()
                        .terminal_title
                        .as_deref(),
                    Some("Task title"),
                    "cleanup cannot replace the title of a session without a process"
                );
            }
        }
        let mut resumed = store.get_session("session-1").unwrap().unwrap();
        resumed.status = SessionStatus::Idle;
        store.update_session(&resumed).unwrap();
        assert_eq!(
            store
                .set_terminal_title("session-1", Some("Resumed task"))
                .unwrap()
                .unwrap()
                .terminal_title
                .as_deref(),
            Some("Resumed task")
        );
        assert!(store
            .set_terminal_title("session-1", None)
            .unwrap()
            .unwrap()
            .terminal_title
            .is_none());
        store.delete_session("session-1").unwrap();
        assert!(store
            .set_terminal_title("session-1", Some("Late title"))
            .unwrap()
            .is_none());
    }
}
