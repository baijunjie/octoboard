//! Which processes cannot be kept: those that end with the launch they are attributed to whatever
//! keeps them. A keep names such a process and is refused, and a listing marks its tree unkeepable.
//! Two causes: the process is attached to the console of its launch's agent (the host ends it
//! with the agent's console), or it is in the process group of a command the agent ends as it
//! exits, where the agent does that (`Launch::ends_command_groups`).

use super::table::{self, Identity, Table};
use super::ProcessLedger;
use crate::platform::proc::{self, ProcessId};
use crate::protocol::UnkeepableReason;

/// The processes tests take as attached to their agent's console, in place of what the host says.
/// Process-wide, as a keep the request layer asks for runs on a blocking thread, and keyed by the
/// processes a test started, so that no other test's are affected.
#[cfg(test)]
pub(crate) static AGENT_CONSOLE_OVERRIDE: std::sync::Mutex<Vec<Identity>> =
    std::sync::Mutex::new(Vec::new());

/// Whether `member` is attached to the console of `agent` (`platform::proc::shares_console`).
fn shares_console(agent: &Identity, member: &Identity) -> Option<bool> {
    #[cfg(test)]
    if AGENT_CONSOLE_OVERRIDE
        .lock()
        .expect("agent console override lock poisoned")
        .contains(member)
    {
        return Some(true);
    }
    proc::shares_console(agent, member)
}

/// Whether `runner` is an ancestor of `leader` with no process that leads a process group of its
/// own between them, walking up from `leader`'s parent. The walk ends at pid 1 or 0, at a process
/// the table lacks, and after a bounded number of steps against a cycle.
fn reaches_without_group_leader(table: &Table, leader: &table::Proc, runner: ProcessId) -> bool {
    const WALK_LIMIT: usize = 64;
    let mut parent = leader.ppid;
    for _ in 0..WALK_LIMIT {
        if parent == runner {
            return true;
        }
        if parent.0 <= 1 {
            return false;
        }
        let Some(ancestor) = table.get(parent) else {
            return false;
        };
        if ancestor.pgid == Some(ancestor.id.pid) {
            return false;
        }
        parent = ancestor.ppid;
    }
    false
}

/// Why [`ProcessLedger::keep_trees`] kept nothing: the process `pid`, named `name`, of a tree it
/// was asked to keep ends with its launch whatever keeps it, for `reason`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unkeepable {
    pub reason: UnkeepableReason,
    pub pid: ProcessId,
    pub name: String,
}

impl ProcessLedger {
    /// The first of `members` that ends with the launch it is attributed to whatever keeps it,
    /// with the name `table` gives it: one attached to the console of the launch's agent
    /// ([`Self::on_agent_console`]), else one in the process group of a command the agent ends
    /// on exit ([`Self::in_ended_command_group`]).
    pub(super) fn unkeepable(&self, members: &[Identity], table: &Table) -> Option<Unkeepable> {
        let (member, reason) = self
            .on_agent_console(members)
            .map(|member| (member, UnkeepableReason::AgentConsole))
            .or_else(|| {
                self.in_ended_command_group(members, table)
                    .map(|member| (member, UnkeepableReason::AgentCommandGroup))
            })?;
        Some(Unkeepable {
            reason,
            pid: member.pid,
            name: table
                .get(member.pid)
                .map(|proc| proc.comm.clone())
                .unwrap_or_default(),
        })
    }

    /// The first of `members` attached to the console of the agent of the launch it is
    /// attributed to (`platform::proc::shares_console`), which ends it with that launch whatever
    /// keeps it. A member whose console cannot be told is taken not to be: only a console known
    /// to be shared refuses a keep.
    fn on_agent_console(&self, members: &[Identity]) -> Option<Identity> {
        let pairs: Vec<(Identity, Identity)> = {
            let book = self.book();
            members
                .iter()
                .filter_map(|member| {
                    let launch = book.launches.get(&book.attributed.get(member)?.launch)?;
                    Some((launch.agent?, *member))
                })
                .filter(|(agent, member)| agent != member)
                .collect()
        };
        pairs
            .into_iter()
            .find(|(agent, member)| shares_console(agent, member) == Some(true))
            .map(|(_, member)| member)
    }

    /// The first of `members` in the process group of a command its launch's agent ends as it
    /// exits, where the agent does that (`Launch::ends_command_groups`): a group led by a running
    /// process (the member itself may lead it) that descends from the process that runs the
    /// agent's commands, the agent or its command host, with no other group leader on the way up
    /// from the leader's parent. A command's own wrapper between the runner and the leader does
    /// not hide it, while a process that started a group of its own under a command's shell is
    /// not in the command's group, and the shell leads one on the walk. A leader that has exited
    /// leaves its group alone, as the agent ends the groups of the commands it still runs.
    fn in_ended_command_group(&self, members: &[Identity], table: &Table) -> Option<Identity> {
        let book = self.book();
        members.iter().copied().find(|member| {
            let Some(launch) = book
                .attributed
                .get(member)
                .and_then(|attributed| book.launches.get(&attributed.launch))
                .filter(|launch| launch.ends_command_groups)
            else {
                return false;
            };
            let Some(runner) = launch.command_host.or(launch.agent) else {
                return false;
            };
            table.has(&runner)
                && table
                    .get(member.pid)
                    .and_then(|proc| proc.pgid)
                    .and_then(|pgid| table.get(pgid))
                    .is_some_and(|leader| reaches_without_group_leader(table, leader, runner.pid))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::attribute::{Attributed, Launch};
    use super::super::table::Proc;
    use super::*;

    /// The launch a made-up table is read for, and its agent, `AGENT`.
    const LAUNCH: &str = "launch";
    const AGENT: u32 = 100;

    fn at(pid: u32) -> Identity {
        Identity {
            pid: ProcessId(pid),
            start_micros: 1000 + pid as u64,
        }
    }

    fn in_group(pid: u32, ppid: u32, pgid: Option<u32>, comm: &str) -> Proc {
        Proc {
            id: at(pid),
            ppid: ProcessId(ppid),
            sid: None,
            pgid: pgid.map(ProcessId),
            comm: comm.to_string(),
        }
    }

    /// A ledger following `LAUNCH`, whose agent is `AGENT` and which declares that its agent ends
    /// its command groups when `ends` is set, with `host` running the commands, and `members`
    /// attributed to it.
    fn ledger_ending_groups(
        ends: bool,
        host: Option<Identity>,
        members: &[u32],
    ) -> (Arc<ProcessLedger>, crate::test_support::ScratchDir) {
        let dir = crate::test_support::ScratchDir::new("ledger-command-groups");
        let ledger = ProcessLedger::new(&dir);
        {
            let mut book = ledger.book();
            book.launches.insert(
                LAUNCH.to_string(),
                Launch {
                    started: true,
                    agent: Some(at(AGENT)),
                    ..Launch::default()
                },
            );
            for pid in members {
                book.attributed.insert(
                    at(*pid),
                    Attributed {
                        launch: LAUNCH.to_string(),
                        sid: None,
                    },
                );
            }
        }
        if ends {
            ledger.command_groups_end(LAUNCH, host);
        }
        (ledger, dir)
    }

    /// What the ledger says of the tree of `members` in `table`: the reason and the pid it
    /// names, if it cannot be kept.
    fn refusal(
        ledger: &ProcessLedger,
        members: &[u32],
        table: &Table,
    ) -> Option<(UnkeepableReason, u32)> {
        let members: Vec<Identity> = members.iter().map(|pid| at(*pid)).collect();
        ledger
            .unkeepable(&members, table)
            .map(|refused| (refused.reason, refused.pid.0))
    }

    /// The agent, running under the daemon.
    fn agent_proc() -> Proc {
        in_group(AGENT, 1, Some(AGENT), "agent")
    }

    #[test]
    fn a_process_in_the_group_of_a_running_command_of_the_agent_cannot_be_kept() {
        // 200 is a command the agent runs, leading its group; 201 is what it started in it.
        let (ledger, _dir) = ledger_ending_groups(true, None, &[201]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(200, AGENT, Some(200), "bash"),
            in_group(201, 200, Some(200), "node"),
        ]);
        assert_eq!(
            refusal(&ledger, &[201], &table),
            Some((UnkeepableReason::AgentCommandGroup, 201))
        );
        let refused = ledger
            .unkeepable(&[at(201)], &table)
            .expect("the tree cannot be kept");
        assert_eq!(refused.name, "node");
    }

    #[test]
    fn the_command_that_leads_its_own_group_cannot_be_kept_itself() {
        let (ledger, _dir) = ledger_ending_groups(true, None, &[200]);
        let table = Table::from_procs([agent_proc(), in_group(200, AGENT, Some(200), "node")]);
        assert_eq!(
            refusal(&ledger, &[200], &table),
            Some((UnkeepableReason::AgentCommandGroup, 200))
        );
    }

    #[test]
    fn the_first_member_in_such_a_group_is_the_one_named() {
        // 300 started its own group, so only 301 is in a command's.
        let (ledger, _dir) = ledger_ending_groups(true, None, &[300, 301]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(200, AGENT, Some(200), "bash"),
            in_group(300, 200, Some(300), "setsid"),
            in_group(301, 200, Some(200), "node"),
        ]);
        let named = refusal(&ledger, &[300, 301], &table);
        assert_eq!(named, Some((UnkeepableReason::AgentCommandGroup, 301)));
    }

    #[test]
    fn a_process_that_started_a_group_of_its_own_can_be_kept() {
        // 201 left its command's group for one of its own (setsid), led by itself.
        let (ledger, _dir) = ledger_ending_groups(true, None, &[201]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(200, AGENT, Some(200), "bash"),
            in_group(201, 200, Some(201), "node"),
        ]);
        assert_eq!(refusal(&ledger, &[201], &table), None);
    }

    #[test]
    fn a_process_in_a_group_whose_leader_has_exited_can_be_kept() {
        // The command 200 finished; what it left in its group is no longer the agent's to end.
        let (ledger, _dir) = ledger_ending_groups(true, None, &[201]);
        let table = Table::from_procs([agent_proc(), in_group(201, 1, Some(200), "node")]);
        assert_eq!(refusal(&ledger, &[201], &table), None);
    }

    #[test]
    fn a_process_in_the_agents_own_group_can_be_kept() {
        // Started without a group of its own, it is in the agent's, led by the agent, whose
        // parent is not the agent.
        let (ledger, _dir) = ledger_ending_groups(true, None, &[201]);
        let table = Table::from_procs([agent_proc(), in_group(201, AGENT, Some(AGENT), "node")]);
        assert_eq!(refusal(&ledger, &[201], &table), None);
    }

    #[test]
    fn a_group_led_by_another_programs_child_is_not_a_command_group() {
        // 200 runs under 150, which is not the agent.
        let (ledger, _dir) = ledger_ending_groups(true, None, &[201]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(150, 1, Some(150), "other"),
            in_group(200, 150, Some(200), "bash"),
            in_group(201, 200, Some(200), "node"),
        ]);
        assert_eq!(refusal(&ledger, &[201], &table), None);
    }

    #[test]
    fn a_group_led_by_a_process_under_a_process_in_no_group_of_its_own_is_a_command_group() {
        // 150 is a child of the agent in the agent's group, leading none: what the agent runs
        // its command through. 200 is its command, leading a group, and 201 is in it.
        let (ledger, _dir) = ledger_ending_groups(true, None, &[201]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(150, AGENT, Some(AGENT), "runner"),
            in_group(200, 150, Some(200), "bash"),
            in_group(201, 200, Some(200), "node"),
        ]);
        assert_eq!(
            refusal(&ledger, &[201], &table),
            Some((UnkeepableReason::AgentCommandGroup, 201))
        );
    }

    #[test]
    fn the_chain_to_the_agent_may_pass_through_several_processes_in_no_group_of_their_own() {
        let (ledger, _dir) = ledger_ending_groups(true, None, &[201]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(150, AGENT, Some(AGENT), "runner"),
            in_group(160, 150, Some(AGENT), "runner"),
            in_group(200, 160, Some(200), "bash"),
            in_group(201, 200, Some(200), "node"),
        ]);
        assert_eq!(
            refusal(&ledger, &[201], &table),
            Some((UnkeepableReason::AgentCommandGroup, 201))
        );
    }

    #[test]
    fn a_group_led_by_a_process_under_a_group_leader_is_not_a_command_group() {
        // 150 leads a group of its own under the agent: the chain from 200 to the agent passes
        // through it, so 200 is what 150 runs, a child of a command and not of the agent.
        let (ledger, _dir) = ledger_ending_groups(true, None, &[201]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(150, AGENT, Some(150), "bash"),
            in_group(160, 150, Some(150), "bash"),
            in_group(200, 160, Some(200), "bash"),
            in_group(201, 200, Some(200), "node"),
        ]);
        assert_eq!(refusal(&ledger, &[201], &table), None);
    }

    #[test]
    fn a_group_whose_chain_never_reaches_the_agent_is_not_a_command_group() {
        // 150 is in a group led by a process that has exited and is parented to init, never the
        // agent.
        let (ledger, _dir) = ledger_ending_groups(true, None, &[201]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(150, 1, Some(140), "runner"),
            in_group(200, 150, Some(200), "bash"),
            in_group(201, 200, Some(200), "node"),
        ]);
        assert_eq!(refusal(&ledger, &[201], &table), None);
    }

    #[test]
    fn nothing_is_refused_once_the_agent_itself_is_gone() {
        let (ledger, _dir) = ledger_ending_groups(true, None, &[201]);
        let table = Table::from_procs([
            in_group(200, AGENT, Some(200), "bash"),
            in_group(201, 200, Some(200), "node"),
        ]);
        assert_eq!(refusal(&ledger, &[201], &table), None);
    }

    #[test]
    fn nothing_is_refused_where_the_host_has_no_process_groups() {
        let (ledger, _dir) = ledger_ending_groups(true, None, &[201]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(200, AGENT, None, "bash"),
            in_group(201, 200, None, "node"),
        ]);
        assert_eq!(refusal(&ledger, &[201], &table), None);
    }

    #[test]
    fn nothing_is_refused_for_an_agent_that_does_not_end_command_groups() {
        let (ledger, _dir) = ledger_ending_groups(false, None, &[200, 201]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(200, AGENT, Some(200), "bash"),
            in_group(201, 200, Some(200), "node"),
        ]);
        assert_eq!(refusal(&ledger, &[200, 201], &table), None);
    }

    #[test]
    fn a_member_of_no_launch_is_never_refused() {
        let (ledger, _dir) = ledger_ending_groups(true, None, &[]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(200, AGENT, Some(200), "bash"),
            in_group(201, 200, Some(200), "node"),
        ]);
        assert_eq!(refusal(&ledger, &[201], &table), None);
    }

    #[test]
    fn with_a_command_host_the_leader_must_be_the_hosts_child_not_the_agents() {
        // 150 is the server the agent's commands go through. 200 is its command, in a group of
        // its own; 210 is a direct child of the agent, which runs none of the commands itself.
        let host = at(150);
        let (ledger, _dir) = ledger_ending_groups(true, Some(host), &[201, 211]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(150, AGENT, Some(150), "server"),
            in_group(200, 150, Some(200), "bash"),
            in_group(201, 200, Some(200), "node"),
            in_group(210, AGENT, Some(210), "bash"),
            in_group(211, 210, Some(210), "node"),
        ]);
        assert_eq!(
            refusal(&ledger, &[201], &table),
            Some((UnkeepableReason::AgentCommandGroup, 201))
        );
        assert_eq!(refusal(&ledger, &[211], &table), None);
    }

    #[test]
    fn with_a_command_host_the_chain_must_reach_the_host_through_no_group_leader() {
        // 150 is the server; 160 is a process of its own that runs no group, under it.
        let host = at(150);
        let (ledger, _dir) = ledger_ending_groups(true, Some(host), &[201, 301]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(150, AGENT, Some(150), "server"),
            in_group(160, 150, Some(150), "runner"),
            in_group(200, 160, Some(200), "bash"),
            in_group(201, 200, Some(200), "node"),
            // The same through a group leader: 300 is a command, 310 its child, a leader as well.
            in_group(300, 150, Some(300), "bash"),
            in_group(310, 300, Some(310), "bash"),
            in_group(301, 310, Some(310), "node"),
        ]);
        assert_eq!(
            refusal(&ledger, &[201], &table),
            Some((UnkeepableReason::AgentCommandGroup, 201))
        );
        assert_eq!(refusal(&ledger, &[301], &table), None);
    }

    #[test]
    fn the_same_group_is_a_command_group_without_a_host_and_not_with_one() {
        let table = Table::from_procs([
            agent_proc(),
            in_group(150, AGENT, Some(150), "server"),
            in_group(210, AGENT, Some(210), "bash"),
            in_group(211, 210, Some(210), "node"),
        ]);
        let (without, _dir) = ledger_ending_groups(true, None, &[211]);
        assert_eq!(
            refusal(&without, &[211], &table),
            Some((UnkeepableReason::AgentCommandGroup, 211))
        );
        let (with, _dir) = ledger_ending_groups(true, Some(at(150)), &[211]);
        assert_eq!(refusal(&with, &[211], &table), None);
    }

    #[test]
    fn nothing_is_refused_once_the_command_host_is_gone() {
        let host = at(150);
        let (ledger, _dir) = ledger_ending_groups(true, Some(host), &[201]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(200, 150, Some(200), "bash"),
            in_group(201, 200, Some(200), "node"),
        ]);
        assert_eq!(refusal(&ledger, &[201], &table), None);
    }

    #[test]
    fn a_host_that_is_not_the_process_it_was_named_for_is_gone() {
        // The pid is running, but started later than the host recorded: another process.
        let host = Identity {
            pid: ProcessId(150),
            start_micros: 1,
        };
        let (ledger, _dir) = ledger_ending_groups(true, Some(host), &[201]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(150, AGENT, Some(150), "server"),
            in_group(200, 150, Some(200), "bash"),
            in_group(201, 200, Some(200), "node"),
        ]);
        assert_eq!(refusal(&ledger, &[201], &table), None);
    }

    #[test]
    fn declaring_the_capability_for_an_unknown_launch_changes_nothing() {
        let (ledger, _dir) = ledger_ending_groups(false, None, &[201]);
        ledger.command_groups_end("another", None);
        assert!(!ledger.book().launches.contains_key("another"));
        let table = Table::from_procs([
            agent_proc(),
            in_group(200, AGENT, Some(200), "bash"),
            in_group(201, 200, Some(200), "node"),
        ]);
        assert_eq!(refusal(&ledger, &[201], &table), None);
    }

    /// Its pids are its own: what is on a console is overridden for every test in the process.
    #[test]
    fn a_process_on_the_agents_console_is_refused_for_that_reason_first() {
        let (ledger, _dir) = ledger_ending_groups(true, None, &[801]);
        let table = Table::from_procs([
            agent_proc(),
            in_group(800, AGENT, Some(800), "bash"),
            in_group(801, 800, Some(800), "node"),
        ]);
        AGENT_CONSOLE_OVERRIDE.lock().unwrap().push(at(801));
        let refused = refusal(&ledger, &[801], &table);
        AGENT_CONSOLE_OVERRIDE
            .lock()
            .unwrap()
            .retain(|shared| *shared != at(801));
        assert_eq!(refused, Some((UnkeepableReason::AgentConsole, 801)));
    }
}
