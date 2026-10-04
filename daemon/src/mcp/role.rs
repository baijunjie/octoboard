//! The text Octoboard tells a session about itself: the injected role description every session
//! gets, and the hub instruction file written into a console's working directory.
//!
//! Tool names are rendered per agent rather than written out, because the prefix an agent shows
//! its model differs (see [`super::qualified_tool_name`]).
//!
//! Two constraints shape what goes where:
//!
//! - **A role is immutable for a session's lifetime.** Claude Code records an appended system
//!   prompt on the conversation's first request and replays it verbatim on every resume, so text
//!   changed later is silently ignored. Nothing here may therefore depend on anything that can
//!   change after the session starts.
//! - **A Grok hub gets no instruction file.** Grok locates a project by walking up for a `.git`
//!   directory and reads no instructions without one, and a console's working directory is not a
//!   repository. Its hub guidance travels in `--rules` instead, which is why
//!   [`hub_instructions`] is also what a Grok hub's role description carries.

use std::path::Path;

use anyhow::{Context, Result};

use crate::protocol::{Agent, Console, Role};

use super::qualified_tool_name;

/// The role description injected at launch.
///
/// A hub whose agent reads an instruction file gets a short pointer here and the detail in the file,
/// which is deliberate rather than tidy-minded: Claude Code records an appended system prompt once
/// per conversation and replays it verbatim forever, while the instruction file is re-read on every
/// launch — so anything put here can never be corrected afterwards. A Grok hub has no instruction
/// file to read, so its whole guidance has to travel in `--rules`.
pub fn role_description(role: Role, agent: Agent) -> String {
    match role {
        Role::Hub => match hub_instruction_filename(agent) {
            Some(filename) => hub_pointer(agent, filename),
            None => hub_instructions(agent),
        },
        Role::Worker => worker_description(agent),
    }
}

/// What a hub that reads an instruction file is told in its system prompt: which side of the
/// orchestration it is on, and where the rest of its instructions are.
fn hub_pointer(agent: Agent, filename: &str) -> String {
    format!(
        "You are the hub session of an Octoboard console: you decompose the user's requests, \
         dispatch them to sessions in this console's projects, follow those up and summarize. You \
         do not modify project code yourself. Your tools are the `{prefix}` ones; \
         `{filename}` in this working directory holds how to use them, and is the version to \
         follow if it ever disagrees with this.",
        prefix = qualified_tool_name(agent, "start_session"),
    )
}

/// What a project session is told. It is a worker under a hub it never talks to directly: the one
/// channel back is `report`, and anything needing a person goes to the person.
fn worker_description(agent: Agent) -> String {
    let report = qualified_tool_name(agent, "report");
    format!(
        "You are running as a project session under Octoboard, which orchestrates agent sessions \
         across several projects. A hub session dispatched this task and is waiting on its result.\n\
         \n\
         - When you finish a round of work, call `{report}`. `summary` is prose for the hub to \
           read; `status` and `open_items` are what it acts on. Use `done` only when nothing is \
           left open, `failed` when the task could not be carried out, and `needs_decision` when \
           the hub has to choose before you can go on.\n\
         - With `done` and no open items the session is wrapped up and archived once the report \
           reaches the hub, so do not report `done` while anything remains.\n\
         - Permission prompts and anything you need to ask a person go to the user directly, in \
           this terminal — not through the hub and not through `{report}`. Ask, then wait.\n\
         - Report once per round of work rather than per step.\n\
         \n\
         Everything else is the ordinary work of this project: its own instructions, conventions \
         and configuration apply unchanged."
    )
}

/// The hub's own guidance. Written into the console's working directory as an instruction file for
/// a Claude Code or Codex hub, and injected as the role description for all three.
pub fn hub_instructions(agent: Agent) -> String {
    let tool = |name: &str| qualified_tool_name(agent, name);
    format!(
        "# Octoboard hub\n\
         \n\
         You are the hub session of an Octoboard console: a control board over agent sessions in \
         several projects. The user brings you a request, you work out which project it belongs \
         to, and you dispatch it to a session there.\n\
         \n\
         ## What you do, and what you do not\n\
         \n\
         You decompose, dispatch, follow up and summarize. **You do not modify project code \
         yourself** — not a file, not a command in a project directory. Every change is made by a \
         session you start in that project. This working directory is Octoboard's own and holds \
         nothing of the user's.\n\
         \n\
         ## The tools\n\
         \n\
         - `{list_projects}` — the console's projects and what is running in each. Start here when \
           you do not already know where a request belongs.\n\
         - `{add_project}` — associate a directory, or clone a repository and associate it.\n\
         - `{start_session}` — start a session in a project and hand it a `brief`. Fill in `goal`, \
           and `context`, `acceptance` and `constraints` wherever you actually have something to \
           say; an empty field is omitted from the session's opening prompt rather than sent \
           blank. Returns the session's id.\n\
         - `{send_message}` — the next instruction for a session already running.\n\
         - `{get_session}` — a session's status and a tail of its output.\n\
         - `{archive_session}` — end and archive a session you are done with.\n\
         - `{list_archived}` / `{reopen_session}` — pick an earlier session back up instead of \
           starting over.\n\
         - `{show_page}` — push an HTML page to the report panel beside you, for anything better \
           shown than typed: a table, a comparison, choices to click. Include a form and the \
           user's answer comes back to you as a message.\n\
         \n\
         ## Working with sessions\n\
         \n\
         - A session reports back on its own; its report arrives here as a message. You do not \
           have to poll for one.\n\
         - A session reporting `done` with nothing open has already been archived by the time you \
           read it. Anything else leaves it running and awaiting your next instruction.\n\
         - **A session waiting for the user is not yours to chase.** It is at a permission prompt \
           or has asked the user something, and only they can clear it. Do not nag it and do not \
           dispatch the same work elsewhere; a message you send it is held until they are done.\n\
         - Keep the two kinds of blockage apart: a session calling `report` with `needs_decision` \
           is asking *you*; a session waiting for the user is asking *them*.\n\
         - Dispatch work that can run at the same time at the same time. Sessions in different \
           projects do not interfere with each other.\n\
         \n\
         ## Reporting to the user\n\
         \n\
         Summarize in your own words what the sessions came back with, and say plainly what is \
         unfinished, what failed, and what needs them. Do not relay a session's report verbatim.",
        list_projects = tool("list_projects"),
        add_project = tool("add_project"),
        start_session = tool("start_session"),
        send_message = tool("send_message"),
        get_session = tool("get_session"),
        archive_session = tool("archive_session"),
        list_archived = tool("list_archived"),
        reopen_session = tool("reopen_session"),
        show_page = tool("show_page"),
    )
}

/// The instruction file a hub of this agent reads out of its working directory, if it reads one at
/// all. Matched by **exact spelling** by every agent, and no agent accepts an all-lowercase name
/// (see "Agent adapters" in `docs/mvp.md`).
///
/// `None` for Grok: it needs a git root to read project instructions, a console's working
/// directory is not a repository, and making one a repository buys nothing else.
pub fn hub_instruction_filename(agent: Agent) -> Option<&'static str> {
    match agent {
        Agent::Claude => Some("CLAUDE.md"),
        Agent::Codex => Some("AGENTS.md"),
        Agent::Grok => None,
    }
}

/// Writes the console's hub instruction file, or removes it where the hub's agent reads none.
///
/// Every agent matches its instruction filename by **exact spelling**, and they disagree on which:
/// Claude Code reads `CLAUDE.md` and not `AGENTS.md` at all, Codex reads `AGENTS.md`, and Grok reads
/// several spellings of both — so on a case-sensitive volume a file left over from a console that
/// changed agents would be loaded alongside, or instead of, the right one. Every name Octoboard
/// might have written is therefore cleared before the current one is written.
pub fn write_hub_instructions(console: &Console) -> Result<()> {
    let workdir = Path::new(&console.workdir);
    std::fs::create_dir_all(workdir).with_context(|| format!("creating {}", workdir.display()))?;

    let wanted = hub_instruction_filename(console.hub_agent);
    for agent in [Agent::Claude, Agent::Codex, Agent::Grok] {
        let Some(name) = hub_instruction_filename(agent) else {
            continue;
        };
        if Some(name) == wanted {
            continue;
        }
        if let Err(err) = std::fs::remove_file(workdir.join(name)) {
            if err.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(workdir = %console.workdir, name, %err, "removing a stale hub instruction file failed");
            }
        }
    }

    if let Some(name) = wanted {
        let path = workdir.join(name);
        std::fs::write(&path, hub_instructions(console.hub_agent))
            .with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A role description that names a tool with the wrong prefix points the model at something it
    /// cannot see, and the prefix differs between the agents.
    #[test]
    fn tool_names_in_the_role_text_carry_that_agents_prefix() {
        for agent in [Agent::Claude, Agent::Codex, Agent::Grok] {
            let hub = role_description(Role::Hub, agent);
            let worker = role_description(Role::Worker, agent);
            assert!(hub.contains(&qualified_tool_name(agent, "start_session")));
            assert!(worker.contains(&qualified_tool_name(agent, "report")));
            assert!(hub_instructions(agent).contains(&qualified_tool_name(agent, "start_session")));
            if agent == Agent::Grok {
                // Grok's names have no `mcp__` prefix at all, so a leftover one is a wrong name
                // rather than a cosmetic difference.
                assert!(!hub.contains("mcp__"));
                assert!(!worker.contains("mcp__"));
            }
        }
    }

    /// The detail lives in the file where there is one, because an appended system prompt is
    /// recorded once per conversation and can never be corrected, while the file is re-read every
    /// launch. Grok reads no file, so it is the one that carries everything in its role text.
    #[test]
    fn the_detail_goes_in_the_file_where_the_agent_reads_one() {
        for agent in [Agent::Claude, Agent::Codex] {
            let role = role_description(Role::Hub, agent);
            assert!(role.len() < hub_instructions(agent).len() / 2);
            assert!(role.contains(hub_instruction_filename(agent).expect("a filename")));
        }
        assert_eq!(
            role_description(Role::Hub, Agent::Grok),
            hub_instructions(Agent::Grok)
        );
    }

    #[test]
    fn only_the_agents_that_read_one_get_an_instruction_file() {
        assert_eq!(hub_instruction_filename(Agent::Claude), Some("CLAUDE.md"));
        assert_eq!(hub_instruction_filename(Agent::Codex), Some("AGENTS.md"));
        assert_eq!(hub_instruction_filename(Agent::Grok), None);
    }
}
