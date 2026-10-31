//! The records of which session works on which branch, which outlive the session.

use anyhow::Result;
use rusqlite::{params, Connection};

use super::Store;

/// The session working on one branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchOrigin {
    pub repository: String,
    /// The branch's name below `refs/heads/`, as a wire path.
    pub branch: String,
    /// When the branch was created, in milliseconds since the epoch.
    pub created_at: i64,
    /// The commit and the second the branch was created at: its first reflog entry, or in a
    /// reftable repository, which has no reflog files, the report's commit and time.
    pub birth_commit: String,
    pub birth_time: i64,
    /// The session working on the branch now, and when it became so (milliseconds since the
    /// epoch).
    pub responsible_session: String,
    pub responsible_since: i64,
    /// The responsible session's title when it became so, so the row outlives that session.
    pub responsible_title: String,
    /// The branch this one was created from, as a wire path; `None` when it was created from
    /// something that is no local branch.
    pub target_branch: Option<String>,
}

/// What [`Store::move_branch_responsibility`] did, by branch name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchMove {
    pub moved: Vec<String>,
    pub unmoved: Vec<String>,
}

impl Store {
    /// Makes every later read or write of the branch records fail.
    #[cfg(test)]
    pub fn drop_branch_origins(&self) -> Result<()> {
        self.lock().execute_batch("DROP TABLE branch_origins")?;
        Ok(())
    }

    /// Every origin recorded for the repository. A row is checked against its branch and dropped
    /// only when the Git mode lists the branches, so until then it can describe a branch deleted,
    /// or deleted and made again, since: a reader that acts on the rows holds each against the
    /// branch's first reflog entry itself.
    pub fn branch_origins(&self, repository: &str) -> Result<Vec<BranchOrigin>> {
        let conn = self.lock();
        read_branch_origins(&conn, repository)
    }

    /// Applies one reconcile of a repository's origins: removes the rows of `dropped`, writes
    /// `put` over whatever those branches had, and trims the repository to `keep` rows when
    /// anything was written. Returns what the repository holds afterwards.
    ///
    /// One transaction for the lot, and the read of the result inside it: every write here follows
    /// from a snapshot of the same rows, so a second reconcile landing between the delete and the
    /// insert of the first would delete a live branch's row or overwrite a fresh attribution with
    /// a stale one. It is also what keeps a repository with hundreds of branches to one journalled
    /// transaction on the first look at it, rather than one per branch.
    pub fn apply_branch_origins(
        &self,
        repository: &str,
        dropped: &[String],
        put: &[BranchOrigin],
        keep: usize,
    ) -> Result<Vec<BranchOrigin>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        {
            let mut delete =
                tx.prepare("DELETE FROM branch_origins WHERE repository = ?1 AND branch = ?2")?;
            for branch in dropped {
                delete.execute(params![repository, branch])?;
            }
            let mut insert = tx.prepare(
                "INSERT OR REPLACE INTO branch_origins
                     (repository, branch, created_at, birth_commit, birth_time,
                      responsible_session_id, responsible_since, responsible_title,
                      target_branch)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for origin in put {
                insert.execute(params![
                    repository,
                    origin.branch,
                    origin.created_at,
                    origin.birth_commit,
                    origin.birth_time,
                    origin.responsible_session,
                    origin.responsible_since,
                    origin.responsible_title,
                    origin.target_branch,
                ])?;
            }
        }
        if !put.is_empty() {
            // The oldest past the cap go, by the time their branch was created.
            tx.execute(
                "DELETE FROM branch_origins WHERE repository = ?1 AND branch NOT IN (
                     SELECT branch FROM branch_origins WHERE repository = ?1
                     ORDER BY created_at DESC, branch LIMIT ?2)",
                params![repository, keep as i64],
            )?;
        }
        let rows = read_branch_origins(&tx, repository)?;
        tx.commit()?;
        Ok(rows)
    }

    /// Makes `to`, called `to_title`, the responsible session of each of `branches` whose
    /// responsible session is `from`, as of `since` (milliseconds), and says which moved and which
    /// did not: a branch that has no row, or whose responsible session is another by now, stays as
    /// it is. One transaction for the lot, each branch compared and set in one statement, so of two
    /// handovers racing for a branch only one lands.
    pub fn move_branch_responsibility(
        &self,
        repository: &str,
        branches: &[String],
        from: &str,
        to: &str,
        to_title: &str,
        since: i64,
    ) -> Result<BranchMove> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let mut result = BranchMove::default();
        {
            let mut update = tx.prepare(
                "UPDATE branch_origins SET responsible_session_id = ?4, responsible_since = ?5,
                     responsible_title = ?6
                 WHERE repository = ?1 AND branch = ?2 AND responsible_session_id = ?3",
            )?;
            for branch in branches {
                if update.execute(params![repository, branch, from, to, since, to_title])? > 0 {
                    result.moved.push(branch.clone());
                } else {
                    result.unmoved.push(branch.clone());
                }
            }
        }
        tx.commit()?;
        Ok(result)
    }

    /// Removes every origin recorded for the repository.
    pub fn delete_repository_branch_origins(&self, repository: &str) -> Result<()> {
        self.lock().execute(
            "DELETE FROM branch_origins WHERE repository = ?1",
            params![repository],
        )?;
        Ok(())
    }
}

fn read_branch_origins(conn: &Connection, repository: &str) -> Result<Vec<BranchOrigin>> {
    let mut stmt = conn.prepare(
        "SELECT repository, branch, created_at, birth_commit, birth_time,
                responsible_session_id, responsible_since, responsible_title,
                target_branch
         FROM branch_origins WHERE repository = ?1 ORDER BY branch",
    )?;
    let rows = stmt
        .query_map(params![repository], |row| {
            Ok(BranchOrigin {
                repository: row.get(0)?,
                branch: row.get(1)?,
                created_at: row.get(2)?,
                birth_commit: row.get(3)?,
                birth_time: row.get(4)?,
                responsible_session: row.get(5)?,
                responsible_since: row.get(6)?,
                responsible_title: row.get(7)?,
                target_branch: row.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::store::test_support::*;

    /// The branch records are not the sessions' rows: deleting the project takes the sessions with
    /// it and leaves the attribution.
    #[test]
    fn branch_origins_outlive_their_session() {
        let path = temp_db("branch-origins");
        let origin = BranchOrigin {
            repository: "/r/.git".to_string(),
            branch: "feature".to_string(),
            created_at: 7,
            birth_commit: "abc".to_string(),
            birth_time: 7,
            responsible_session: "session-1".to_string(),
            responsible_since: 7,
            responsible_title: "Title".to_string(),
            target_branch: Some("main".to_string()),
        };
        let put = |store: &Store| {
            store
                .apply_branch_origins("/r/.git", &[], std::slice::from_ref(&origin), 10)
                .expect("origin");
        };
        let store = Store::open(&path).expect("store");
        store
            .insert_console(&console(None, None, None))
            .expect("console");
        store.insert_project(&project(false)).expect("project");
        assert!(store.branch_origins("/r/.git").unwrap().is_empty());

        put(&store);
        store.delete_project("project-1").expect("project deleted");
        assert_eq!(store.branch_origins("/r/.git").unwrap(), [origin]);
    }

    /// Each branch of a handover is compared and set on its own: the ones the expected session
    /// holds move, the rest are reported and stay.
    #[test]
    fn a_handover_moves_only_the_branches_the_expected_session_holds() {
        let path = temp_db("branch-handover");
        let store = Store::open(&path).expect("store");
        let origin = |branch: &str, session: &str| BranchOrigin {
            repository: "/r/.git".to_string(),
            branch: branch.to_string(),
            created_at: 7,
            birth_commit: "abc".to_string(),
            birth_time: 7,
            responsible_session: session.to_string(),
            responsible_since: 7,
            responsible_title: "Title".to_string(),
            target_branch: None,
        };
        store
            .apply_branch_origins("/r/.git", &[], &[origin("a", "s1"), origin("b", "s2")], 10)
            .unwrap();
        let names = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();

        let moved = store
            .move_branch_responsibility("/r/.git", &names(&["a", "b", "c"]), "s1", "s3", "Three", 9)
            .unwrap();
        assert_eq!(moved.moved, names(&["a"]));
        assert_eq!(moved.unmoved, names(&["b", "c"]));

        let rows = store.branch_origins("/r/.git").unwrap();
        let a = &rows[0];
        assert_eq!(
            (a.responsible_session.as_str(), a.responsible_since),
            ("s3", 9)
        );
        assert_eq!(a.responsible_title, "Three");
        assert_eq!(rows[1], origin("b", "s2"));
        let other = store
            .move_branch_responsibility("/elsewhere/.git", &names(&["a"]), "s3", "s4", "Four", 10)
            .unwrap();
        assert!(other.moved.is_empty());
    }
}
