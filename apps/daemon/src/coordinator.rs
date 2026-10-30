//! The coordinator role: what each control-socket request does to the stored consoles, projects and
//! sessions, and which host-role work it triggers — including the launch flow every way of starting
//! a session goes through.
//!
//! Writing into a running session and everything built on it (the console session's reports,
//! synthesis, automatic archiving) is `crate::reporting`'s, because the console session's tools and
//! the hook callback reach it without going through a control-socket request at all.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::account_dirs::{account_dir_key, normalize_account_dir};
use crate::account_login;
use crate::account_quotas;
use crate::adapter;
use crate::agent_dialogs;
use crate::agent_records;
use crate::availability;
use crate::branch_origin;
use crate::config_export;
use crate::config_import;
use crate::env_shell;
use crate::git_push;
use crate::git_remote;
use crate::git_restore;
use crate::git_status;
use crate::git_switch;
use crate::git_workflow::{
    GitWorkflowOverride, GitWorkflowOverrideChanges, GitWorkflowSettings,
    GitWorkflowSettingsChanges,
};
use crate::hostfs;
use crate::launch_settings::{
    self, normalize_launch_setting, normalize_session_launch_setting, trimmed_or_none,
    LaunchChange, LaunchCheck,
};
use crate::mcp;
use crate::open_directory;
use crate::open_terminal;
use crate::process_ledger::HeldForLaunch;
use crate::project_trash;
use crate::protocol::{
    error_code, error_parts, notice_code, now_millis, Account, Agent, ApprovalReviewer,
    Availability, ClaudeApprovalMode, CodedError, Console, ConsoleSessionColour,
    ConversationLocation, Event, ModelChangeOutcome, Notice, Origin, Present, Project,
    ProjectAccount, ProjectSource, RequestBody, Role, Session, SessionBranchItem,
    SessionBranchTarget, SessionStatus, TerminalAppearance,
};
use crate::relocate;
use crate::reporting;
use crate::session::LiveSession;
use crate::session_history;
use crate::state::{AppState, Deletion};
use crate::store::{ReorderOutcome, Store, LOCAL_HOST_ID};
use crate::switch_continue;
use crate::term;
use crate::trust;

/// Handles one request. The returned event, when there is one, is the answer to that request and
/// goes to the asking socket only; everything that changed state has already been broadcast. For
/// the tests, which have no socket to queue a reply on; the server uses [`handle_deferred`].
#[cfg(test)]
pub async fn handle(
    state: &Arc<AppState>,
    request_id: Option<String>,
    body: RequestBody,
) -> Result<Option<Event>> {
    // Any follow-up runs as it is dropped, here: this caller has no reply to queue first.
    handle_deferred(state, request_id, body)
        .await
        .map(|(reply, _followup)| reply)
}

/// [`handle`], for a caller that sends the reply itself: what a request still has to do after its
/// reply has been queued comes back as a [`Followup`], to drop once the reply is on its way.
pub async fn handle_deferred(
    state: &Arc<AppState>,
    request_id: Option<String>,
    body: RequestBody,
) -> Result<(Option<Event>, Option<Followup>)> {
    let mut followup = None;
    let reply = handle_inner(state, request_id, body, &mut followup).await?;
    Ok((reply, followup))
}

async fn handle_inner(
    state: &Arc<AppState>,
    request_id: Option<String>,
    body: RequestBody,
    followup: &mut Option<Followup>,
) -> Result<Option<Event>> {
    let reveal = matches!(&body, RequestBody::RevealProjectPath { .. });
    match body {
        RequestBody::CreateConsole {
            name,
            default_agent,
            project_session_agent,
            claude_account_id,
            codex_account_id,
            grok_account_id,
            claude_console_session_model,
            claude_console_session_effort,
            codex_console_session_model,
            codex_console_session_effort,
            grok_console_session_model,
            grok_console_session_effort,
            claude_project_session_model,
            claude_project_session_effort,
            codex_project_session_model,
            codex_project_session_effort,
            grok_project_session_model,
            grok_project_session_effort,
            icon,
            console_session_prompt,
        } => {
            let new = NewConsole {
                name,
                default_agent,
                project_session_agent,
                claude_account_id,
                codex_account_id,
                grok_account_id,
                claude_console_session_model,
                claude_console_session_effort,
                codex_console_session_model,
                codex_console_session_effort,
                grok_console_session_model,
                grok_console_session_effort,
                claude_project_session_model,
                claude_project_session_effort,
                codex_project_session_model,
                codex_project_session_effort,
                grok_project_session_model,
                grok_project_session_effort,
                icon,
                console_session_prompt,
            };
            // The refusals that need no wait first, so a console that would be refused for its
            // name or an account is not first held up asking an agent for its models.
            let icon = check_new_console(state, &new)?;
            check_new_console_launch(state, &new).await?;
            store_new_console(state, new, icon)?;
            Ok(None)
        }

        RequestBody::UpdateConsole {
            console: id,
            name,
            default_agent,
            project_session_agent,
            claude_account_id,
            codex_account_id,
            grok_account_id,
            claude_console_session_model,
            claude_console_session_effort,
            codex_console_session_model,
            codex_console_session_effort,
            grok_console_session_model,
            grok_console_session_effort,
            claude_project_session_model,
            claude_project_session_effort,
            codex_project_session_model,
            codex_project_session_effort,
            grok_project_session_model,
            grok_project_session_effort,
            icon,
            console_session_prompt,
        } => {
            let project_launch = launch_settings::changes_of([
                (
                    Agent::Claude,
                    claude_project_session_model,
                    claude_project_session_effort,
                ),
                (
                    Agent::Codex,
                    codex_project_session_model,
                    codex_project_session_effort,
                ),
                (
                    Agent::Grok,
                    grok_project_session_model,
                    grok_project_session_effort,
                ),
            ]);
            let launch = launch_settings::changes_of([
                (
                    Agent::Claude,
                    claude_console_session_model,
                    claude_console_session_effort,
                ),
                (
                    Agent::Codex,
                    codex_console_session_model,
                    codex_console_session_effort,
                ),
                (
                    Agent::Grok,
                    grok_console_session_model,
                    grok_console_session_effort,
                ),
            ]);
            update_console(
                state,
                &id,
                ConsoleChanges {
                    name,
                    default_agent,
                    project_session_agent,
                    accounts: [
                        (Agent::Claude, claude_account_id),
                        (Agent::Codex, codex_account_id),
                        (Agent::Grok, grok_account_id),
                    ]
                    .into_iter()
                    .filter_map(|(agent, account)| Some((agent, account?)))
                    .collect(),
                    launch,
                    project_launch,
                    icon,
                    console_session_prompt,
                },
            )
            .await?;
            Ok(None)
        }

        RequestBody::DeleteConsole { console: id } => {
            let _removal = state.claim_session_removal();
            ensure_unlocked(
                state
                    .store
                    .list_sessions()?
                    .into_iter()
                    .filter(|session| session.console_id == id),
            )?;
            let console = state
                .store
                .get_console(&id)?
                .ok_or_else(|| CodedError::unknown_console(&id))?;
            if state.has_live_sessions_where(|session| session.console_id == id)? {
                return Err(CodedError::raised(
                    error_code::CONSOLE_HAS_RUNNING_SESSIONS,
                    "this console still has running or starting sessions; archive the running ones, or wait for the starting ones to finish",
                    &[],
                ));
            }
            // Read before the delete below: `projects.console_id` is `ON DELETE CASCADE`, so the
            // store drops this console's projects with it, and nothing else would ever clean up
            // their git statuses afterwards — the daemon keeps no timer of its own that would
            // notice they are gone.
            let projects: Vec<Project> = state
                .store
                .list_projects()?
                .into_iter()
                .filter(|project| project.console_id == id)
                .collect();
            // Its sessions cascade the same way, and their saved output is Octoboard's own.
            let session_ids = session_ids_where(state, |session| session.console_id == id)?;
            state.store.delete_console(&id)?;
            let project_ids: Vec<&str> =
                projects.iter().map(|project| project.id.as_str()).collect();
            state.forget_deleted_projects(&project_ids);
            state.forget_cascaded_sessions(&session_ids);
            state.stalled_directory_passes.forget(&id);
            for project in &projects {
                crate::branch_origin::project_deleted(state, &project.path);
            }
            crate::shell_terminal::close_projects(state, &project_ids).await;
            // Their processes, which no session owns, go with the projects.
            crate::process_requests::stop_project_processes(state, &projects).await;
            // The working directory is Octoboard's own, under `~/.octoboard/consoles/`; no project
            // directory is ever touched by this.
            if let Err(err) = mcp::console_instructions::remove_console_workdir(&console.workdir) {
                if err.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(workdir = %console.workdir, %err, "removing the console working directory failed");
                }
            }
            state.broadcast(Event::ConsoleDeleted { console: id });
            Ok(None)
        }

        RequestBody::AddProject {
            console_id,
            source,
            path,
            remote_url,
            name,
            default_agent,
            claude_account_id,
            codex_account_id,
            grok_account_id,
            claude_session_model,
            claude_session_effort,
            codex_session_model,
            codex_session_effort,
            grok_session_model,
            grok_session_effort,
            detect_default_agent,
            tags,
            description,
            git_workflow,
        } => {
            add_project(
                state,
                AddProjectRequest {
                    console_id,
                    source,
                    path,
                    remote_url,
                    name,
                    default_agent,
                    settings: NewProjectSettings {
                        pinned: false,
                        claude_account_id: claude_account_id.unwrap_or_default(),
                        codex_account_id: codex_account_id.unwrap_or_default(),
                        grok_account_id: grok_account_id.unwrap_or_default(),
                        claude_session_model,
                        claude_session_effort,
                        codex_session_model,
                        codex_session_effort,
                        grok_session_model,
                        grok_session_effort,
                        git_workflow: git_workflow.unwrap_or_default(),
                    },
                    detect_default_agent,
                    tags,
                    description,
                    check_launch: true,
                    check_text_limits: true,
                },
            )
            .await?;
            Ok(None)
        }

        RequestBody::UpdateProject {
            project: id,
            name,
            default_agent,
            claude_account_id,
            codex_account_id,
            grok_account_id,
            claude_session_model,
            claude_session_effort,
            codex_session_model,
            codex_session_effort,
            grok_session_model,
            grok_session_effort,
            pinned,
            tags,
            description,
            git_workflow,
        } => {
            let launch = launch_settings::changes_of([
                (Agent::Claude, claude_session_model, claude_session_effort),
                (Agent::Codex, codex_session_model, codex_session_effort),
                (Agent::Grok, grok_session_model, grok_session_effort),
            ]);
            update_project(
                state,
                &id,
                ProjectChanges {
                    name,
                    default_agent,
                    accounts: [
                        (Agent::Claude, claude_account_id),
                        (Agent::Codex, codex_account_id),
                        (Agent::Grok, grok_account_id),
                    ]
                    .into_iter()
                    .filter_map(|(agent, account)| Some((agent, account?.unwrap_or_default())))
                    .collect(),
                    launch,
                    pinned,
                    tags,
                    description,
                    git_workflow: git_workflow.unwrap_or_default(),
                    ..ProjectChanges::default()
                },
            )
            .await?;
            Ok(None)
        }

        RequestBody::DeleteProject {
            project: id,
            stop_sessions,
            trash_directory,
            force_remove,
            inspection,
        } => {
            let _removal = state.claim_session_removal();
            let deleted = state
                .store
                .get_project(&id)?
                .ok_or_else(|| CodedError::unknown_project(&id))?;
            let in_project = |session: &Session| session.project_id.as_deref() == Some(&id);
            ensure_unlocked(state.store.list_sessions()?.into_iter().filter(in_project))?;
            // A session still being launched has no process to stop, so it refuses either way.
            let refused = if stop_sessions {
                state.has_launching_sessions_where(in_project)?
            } else {
                state.has_live_sessions_where(in_project)?
            };
            if refused {
                return Err(CodedError::raised(
                    error_code::PROJECT_HAS_RUNNING_SESSIONS,
                    "this project still has running or starting sessions; archive the running ones, or wait for the starting ones to finish",
                    &[],
                ));
            }
            // Checked before any session is stopped, so a refusal leaves everything as it was.
            let to_trash = if trash_directory {
                project_trash::directory_to_trash(
                    state,
                    &deleted,
                    &state.store.list_projects()?,
                    inspection.as_deref(),
                )
                .await?
            } else {
                None
            };
            // What goes along with the folder is found while the folder, and the records that say
            // what is bound to the project's sessions, are still there.
            // A failure to find it out leaves everything outside the folder, and is told once the
            // removal stands. The worktrees the inspection before held back stay held back.
            let (along, undetermined) = if to_trash.is_some() {
                let held_back = inspection.as_deref().map_or_else(Vec::new, |token| {
                    state.unresolved_removals.held_back(token, &deleted.id)
                });
                let (state, project) = (Arc::clone(state), deleted.clone());
                match tokio::task::spawn_blocking(move || {
                    branch_origin::cleanup::project::plan(
                        &state,
                        &project,
                        branch_origin::cleanup::project::Reads::Removal,
                        &held_back,
                    )
                })
                .await?
                {
                    Ok(along) => (along, None),
                    Err(err) => (None, Some(format!("{err:#}"))),
                }
            } else {
                (None, None)
            };
            if stop_sessions {
                stop_project_sessions(state, in_project)?;
            }
            // Ended before the folder is moved, if it is, so no shell is left running in it (a
            // tool still writing there by absolute path could recreate the moved folder); one
            // started during the move is ended with the record below.
            crate::shell_terminal::close_project(state, &id).await;
            // The project's processes, which no session owns, go with it, as the confirmation
            // said (`inspect_project_removal`'s `processes`), the folder moved or not: stopped
            // here, before the move, and not after it, since removing the project stops them
            // whether or not its folder goes to the Trash.
            crate::process_requests::stop_project_processes(state, std::slice::from_ref(&deleted))
                .await;
            // The directory goes first, once nothing runs in it, so a failure leaves the project
            // in place for a retry. Without `trash_directory` only the association goes away: the
            // directory is the user's. The repository is found first, as it cannot be once the
            // directory is gone.
            let repository = crate::branch_origin::repository_of(Path::new(&deleted.path));
            if let Some(directory) = to_trash {
                // Off the runtime's worker: a move across volumes copies.
                let trash = state.host_tools.get().trash;
                tokio::task::spawn_blocking(move || trash(&directory)).await??;
                // Requests run concurrently, so a session may have been started in the project
                // while the folder moved, and nothing held it back. The folder is gone by now, so
                // the project is removed regardless: such a session is ended here as well as it
                // can be, and one still being launched, which cannot be ended, is left to fail
                // once its record goes.
                if let Err(err) = stop_project_sessions(state, in_project) {
                    tracing::warn!(project = %id, %err, "ending sessions started during the move to the Trash failed");
                }
            }
            // The project's sessions cascade with it, so their saved output is collected first.
            let session_ids = session_ids_where(state, in_project)?;
            state.store.delete_project(&id)?;
            state.forget_deleted_projects(&[id.as_str()]);
            state.forget_cascaded_sessions(&session_ids);
            crate::shell_terminal::close_project(state, &id).await;
            crate::branch_origin::project_deleted_in(state, repository);
            state.broadcast(Event::ProjectDeleted {
                project: id.clone(),
            });
            if let Some(detail) = undetermined {
                let failure = CodedError::raised(
                    error_code::PROJECT_BRANCHES_NOT_DETERMINED,
                    format!(
                        "what goes with the project folder could not be determined, so the \
                         worktrees outside it, and its branches if the repository stays, were \
                         left: {detail}"
                    ),
                    &[("name", &deleted.name), ("detail", &detail)],
                );
                state.broadcast(Event::from_error(None, &failure));
            }
            if let Some(along) = along {
                let state = Arc::clone(state);
                tokio::spawn(async move {
                    let missed = branch_origin::cleanup::project::carry_out_once_stopped(
                        state.clone(),
                        session_ids,
                        along,
                        force_remove,
                    )
                    .await;
                    report_not_removed(&state, &missed);
                });
            }
            Ok(None)
        }

        RequestBody::InspectProjectRemoval { project } => {
            let project = state
                .store
                .get_project(&project)?
                .ok_or_else(|| CodedError::unknown_project(&project))?;
            // A removal that would be refused takes nothing along: its refusal is the answer, and
            // a client lists nothing for it. One a path that could not be resolved decided is kept
            // for the removal to repeat, so it takes nothing along either.
            project_trash::inspected(state, &project, &state.store.list_projects()?).await?;
            let id = project.id.clone();
            let deleted_project = project.clone();
            let planning = Arc::clone(state);
            let (items, held_back) = tokio::task::spawn_blocking(move || {
                Ok::<_, anyhow::Error>(
                    branch_origin::cleanup::project::plan(
                        &planning,
                        &project,
                        branch_origin::cleanup::project::Reads::Inspection,
                        &[],
                    )?
                    .map(|along| (along.items(), along.held_back()))
                    .unwrap_or_default(),
                )
            })
            .await??;
            let processes =
                crate::process_requests::project_processes(state, &deleted_project).await?;
            // The worktrees it held back, not listed as going, are kept for the removal confirmed
            // after it to hold back too.
            let inspection = if held_back.is_empty() {
                None
            } else {
                state
                    .unresolved_removals
                    .keep_held_back(state, &id, held_back)
            };
            Ok(Some(Event::ProjectRemovalInspected {
                id: request_id,
                items,
                processes,
                inspection,
            }))
        }

        RequestBody::ListDir { path } => {
            let expanded = hostfs::expand(&path);
            let entries = hostfs::list_dir(&expanded)?;
            Ok(Some(Event::DirListing {
                id: request_id,
                path: expanded.to_string_lossy().into_owned(),
                entries,
            }))
        }

        RequestBody::DetectDirectoryAgent { path } => {
            let directory = absolute_path(&path)?;
            hostfs::require_directory(&directory)?;
            let agent = detect_project_agent(state, &directory);
            Ok(Some(Event::AgentDetected {
                id: request_id,
                agent,
            }))
        }

        RequestBody::ProbeGitRemote { remote_url } => {
            // Asked as the user types, so the shell environment is the cached one rather than a
            // fresh login shell per probe.
            let probed = tokio::task::spawn_blocking(move || {
                let shell_env = crate::env_shell::cached_snapshot()
                    .context("snapshotting the shell environment")?;
                hostfs::probe_remote_agent(&remote_url, &shell_env)
            })
            .await??;
            Ok(Some(Event::AgentDetected {
                id: request_id,
                agent: probed.filter(|agent| agent_is_available(state, *agent)),
            }))
        }

        RequestBody::ListAgentModels { agent, account } => {
            let account_id = checked_account_reference(state, agent, account)?;
            let listing = launch_settings::agent_listing(state, agent, &account_id).await?;
            Ok(Some(Event::AgentModels {
                id: request_id,
                agent,
                account: account_id,
                models: listing.models,
                unavailable: listing.unavailable,
            }))
        }

        RequestBody::LocateConversationToResume {
            project,
            agent_session_id,
        } => {
            let location = locate_conversation(state, &project, &agent_session_id).await?;
            Ok(Some(Event::ConversationLocated {
                id: request_id,
                location,
            }))
        }

        RequestBody::RefreshAccountQuotas { agent, account } => {
            let scope = match (agent, account) {
                (Some(agent), account) => {
                    checked_account_reference(state, agent, account.clone())?;
                    account_quotas::Scope {
                        agent: Some(agent),
                        account,
                        ..Default::default()
                    }
                }
                (None, Some(account)) => {
                    let stored = state
                        .store
                        .get_account(&account)?
                        .ok_or_else(|| CodedError::unknown_account(&account))?;
                    account_quotas::Scope {
                        agent: Some(stored.agent),
                        account: Some(account),
                        ..Default::default()
                    }
                }
                (None, None) => account_quotas::Scope::default(),
            };
            // The readings take seconds; each is announced as it lands.
            let state = state.clone();
            tokio::spawn(async move { account_quotas::refresh_requested(&state, &scope).await });
            Ok(None)
        }

        RequestBody::StartAccountLogin { agent, account } => {
            refuse_if_agent_unavailable(state, agent)?;
            // Read once: the id and the directory the login runs against are the same row's.
            let stored = match &account {
                Some(id) => Some(
                    state
                        .store
                        .get_account(id)?
                        .filter(|stored| stored.agent == agent)
                        .ok_or_else(|| CodedError::unknown_account(id))?,
                ),
                None => None,
            };
            let config_dir = stored.map(|stored| stored.config_dir);
            let resolved_dir = config_dir
                .clone()
                .or_else(|| state.agent_availability_of(agent).default_account_dir);
            account_login::AccountLogins::start(
                state,
                account_login::Start {
                    agent,
                    account: account.clone(),
                    config_dir: config_dir.clone(),
                    resolved_dir,
                },
            )
            .await?;
            // A delete or repoint between the read and the login being registered missed the
            // cancel it makes: that cancel follows its write, so the write is seen here.
            if let Some(id) = &account {
                let current = state.store.get_account(id)?.map(|stored| stored.config_dir);
                if current != config_dir {
                    let _ = state.account_logins.cancel(agent, account);
                }
            }
            Ok(None)
        }

        RequestBody::CancelAccountLogin { agent, account } => {
            state.account_logins.cancel(agent, account)?;
            Ok(None)
        }

        RequestBody::ExportConfiguration {
            console,
            consoles,
            include_settings,
            include_accounts,
            include_paths,
        } => {
            let selection = config_export::Selection::from_request(
                console,
                consoles,
                include_settings,
                include_accounts,
                include_paths,
            )?;
            Ok(Some(Event::ConfigurationExported {
                id: request_id,
                document: config_export::build(&state.store, &selection)?,
            }))
        }

        RequestBody::InspectConfiguration { document } => {
            let inspection = config_import::inspect(state, &document).await?;
            Ok(Some(Event::ConfigurationInspected {
                id: request_id,
                settings: inspection.settings,
                accounts: inspection.accounts,
                consoles: inspection.consoles,
            }))
        }

        RequestBody::ImportConfiguration {
            document,
            resolutions,
            apply_settings,
            apply_accounts,
            account_resolutions,
            apply_consoles,
        } => {
            let imported = config_import::apply(
                state,
                &document,
                resolutions,
                apply_settings,
                apply_accounts.then_some(account_resolutions),
                apply_consoles,
            )
            .await?;
            Ok(Some(Event::ConfigurationImported {
                id: request_id,
                settings_applied: imported.settings_applied,
                settings_error: imported.settings_error,
                accounts: imported.accounts,
                consoles: imported.consoles,
            }))
        }

        RequestBody::OpenSession {
            console_id,
            project_id,
            agent,
            account,
            model,
            effort,
            task,
            title,
            bound_to,
            resume_agent_session_id,
            terminal_appearance,
        } => {
            let (session, launch) = open_session_detached(
                state,
                OpenRequest {
                    console_id,
                    project_id,
                    agent,
                    account,
                    model,
                    effort,
                    task,
                    title,
                    origin: Origin::User,
                    bound_to,
                    resume_agent_session_id,
                    session_id: None,
                    terminal_appearance,
                },
            )
            .await?;
            *followup = Some(launch);
            Ok(Some(Event::SessionOpened {
                id: request_id,
                session: state.wire_session(session),
            }))
        }

        RequestBody::ResumeSession {
            session,
            terminal_appearance,
        } => {
            resume_session(state, &session, None, terminal_appearance).await?;
            Ok(None)
        }

        RequestBody::ArchiveSession {
            session,
            force_remove,
            keep,
            keep_process_ids,
        } => {
            // Before the archive ends the processes, which the exit path then stops; kept after
            // the record is known so an unknown session keeps nothing.
            if !keep_process_ids.is_empty() {
                state.session_record(&session)?;
                let kept = crate::process_requests::keep(state, keep_process_ids.clone()).await?;
                let missed: Vec<&String> = keep_process_ids
                    .iter()
                    .filter(|id| !kept.contains(id))
                    .collect();
                if !missed.is_empty() {
                    tracing::warn!(
                        session = %session, ids = ?missed,
                        "process trees the user chose to keep were no longer found, or were a console session's, which keeps none; whatever is left of them is stopped with the session"
                    );
                }
            }
            let archived = archive_session_group(state, &session, true)?;
            remove_branches_once_stopped(state, archived, force_remove, keep);
            Ok(None)
        }

        RequestBody::SwitchSessionAccount {
            session,
            account,
            terminal_appearance,
        } => {
            switch_session_account(
                state,
                &session,
                account,
                terminal_appearance,
                env_shell::launch_snapshot,
            )
            .await?;
            Ok(None)
        }

        RequestBody::SetSessionModel {
            session,
            model,
            effort,
            terminal_appearance,
        } => {
            let change =
                set_session_model(state, &session, model, effort, terminal_appearance).await?;
            let (outcome, continued) = match change {
                ModelChange::Unchanged => (ModelChangeOutcome::Unchanged, None),
                ModelChange::Recorded => (ModelChangeOutcome::Recorded, None),
                ModelChange::Restarted { continued } => {
                    (ModelChangeOutcome::Restarted, Some(continued))
                }
            };
            Ok(Some(Event::SessionModelSet {
                id: request_id,
                session,
                outcome,
                continued,
            }))
        }

        RequestBody::HandOffSession {
            session,
            agent,
            account,
            model,
            effort,
            terminal_appearance,
        } => {
            let moved = crate::handoff::operation::hand_off(
                state,
                crate::handoff::operation::Handoff {
                    session,
                    agent,
                    account,
                    model,
                    effort,
                    title: None,
                    note: None,
                    asked_by: None,
                    terminal_appearance,
                },
            )
            .await?;
            Ok(Some(Event::SessionOpened {
                id: request_id,
                session: state.wire_session(moved.receiving),
            }))
        }

        RequestBody::InspectSessionRecords { sessions } => {
            let state = Arc::clone(state);
            let presence = tokio::task::spawn_blocking(move || {
                inspect_session_records(&state, &sessions, agent_records::Locator::default())
            })
            .await??;
            Ok(Some(Event::SessionRecordsInspected {
                id: request_id,
                presence,
            }))
        }

        RequestBody::ReadSessionHistory { session } => {
            let record = state.session_record(&session)?;
            let messages = session_history::of_session_blocking(state, record).await?;
            Ok(Some(Event::SessionHistory {
                id: request_id,
                session,
                unavailable: messages.is_none(),
                messages: messages.unwrap_or_default(),
            }))
        }

        RequestBody::InspectSessionBranches {
            sessions,
            processes,
        } => {
            let ids = processes.then(|| sessions.clone());
            let inspected = Arc::clone(state);
            let items = tokio::task::spawn_blocking(move || {
                let mut found = Vec::new();
                for id in &sessions {
                    found.extend(inspected.store.get_session(id)?);
                }
                branch_origin::cleanup::inspect(&inspected, &found)
            })
            .await??;
            let processes = match ids {
                Some(ids) => {
                    crate::process_requests::long_running(
                        state,
                        ids,
                        crate::process_requests::ask_after(),
                    )
                    .await?
                }
                None => Vec::new(),
            };
            Ok(Some(Event::SessionBranchesInspected {
                id: request_id,
                items,
                processes,
            }))
        }

        RequestBody::ListProcesses { target } => {
            let trees = crate::process_requests::list(state, target).await?;
            Ok(Some(Event::ProcessesListed {
                id: request_id,
                trees,
            }))
        }

        RequestBody::StopProcesses { ids } => {
            let ids = crate::process_requests::stop(state, ids).await?;
            Ok(Some(Event::ProcessesStopped {
                id: request_id,
                ids,
            }))
        }

        RequestBody::KeepProcesses { ids } => {
            let ids = crate::process_requests::keep(state, ids).await?;
            Ok(Some(Event::ProcessesKept {
                id: request_id,
                ids,
            }))
        }

        RequestBody::DeleteSession {
            session,
            trash_agent_records,
            only_if_unused,
            force_remove,
            keep,
        } => {
            let deleted = if only_if_unused {
                delete_unused_session(state, &session)?
            } else {
                delete_session(state, &session)?
            };
            remove_branches_once_stopped(state, deleted.clone(), force_remove, keep);
            let records_to_trash = deleted
                .into_iter()
                .filter(|session| trash_agent_records || !session.has_conversation)
                .collect();
            trash_agent_records_after_delete(state, records_to_trash).await;
            Ok(None)
        }

        RequestBody::DeleteArchivedSessions {
            console,
            project,
            console_session,
            trash_agent_records,
            force_remove,
            keep,
        } => {
            let deleted = delete_archived_sessions(
                state,
                &console,
                project.as_deref(),
                console_session.as_deref(),
            )?;
            remove_branches_once_stopped(state, deleted.clone(), force_remove, keep);
            if trash_agent_records {
                trash_agent_records_after_delete(state, deleted).await;
            }
            Ok(None)
        }

        RequestBody::SetSessionPinned {
            session: id,
            pinned,
        } => {
            set_session_pinned(state, &id, pinned)?;
            Ok(None)
        }

        RequestBody::SetSessionLocked {
            session: id,
            locked,
        } => {
            set_session_locked(state, &id, locked)?;
            Ok(None)
        }

        RequestBody::SetSessionColour {
            session: id,
            colour,
        } => {
            set_session_colour(state, &id, colour)?;
            Ok(None)
        }

        RequestBody::SendMessage { session, text } => {
            send_message(state, &session, text).await?;
            Ok(None)
        }

        RequestBody::InterruptSession { session } => {
            let crate::interrupt::Interrupted {
                interrupted,
                status,
                note,
            } = crate::interrupt::interrupt_session(state, &session).await?;
            Ok(Some(Event::SessionInterrupted {
                id: request_id,
                session,
                interrupted,
                status,
                note,
            }))
        }

        RequestBody::RenameSession { session: id, title } => {
            rename_session(state, &id, &title)?;
            Ok(None)
        }

        RequestBody::ReleaseSession { session: id } => {
            let Released {
                session: released,
                running,
                former_owner,
            } = release_session(state, &id, None, None)?;
            // Nobody asked for the notices: a session that cannot be told now is still released,
            // and each notice is told, and its failure logged, on its own.
            let (told, told_owner) = tokio::join!(
                mcp::tell_released(state, &released, running, None),
                mcp::tell_former_owner(state, &former_owner, &released),
            );
            if let Err(err) = told {
                tracing::warn!(session = %id, %err, "telling a released session failed");
            }
            if let Err(err) = told_owner {
                tracing::warn!(
                    session = %former_owner, %err, "telling the owner of a released session failed"
                );
            }
            Ok(None)
        }

        RequestBody::ListPages { console_session } => {
            // A project session can have pages too; only a session that does not exist is unknown.
            state.session_record(&console_session)?;
            let pages = state.store.list_pages(&console_session)?;
            Ok(Some(Event::PageList {
                id: request_id,
                console_session_id: console_session,
                pages,
            }))
        }

        RequestBody::SubmitPage {
            page: page_id,
            data,
        } => {
            submit_page(state, &page_id, data).await?;
            Ok(None)
        }

        RequestBody::ConfirmTrust {
            session,
            remember,
            trust_parent_dir,
        } => {
            trust::confirm(state, &session, remember, trust_parent_dir).await?;
            Ok(None)
        }

        RequestBody::RemoveTrustedDirectory { path } => {
            trust::remove_trusted_directory(state, &path)?;
            Ok(None)
        }

        // An approval is answered with the session it started, a refusal with an ack.
        RequestBody::AnswerSessionRequest {
            request_id: asked,
            approve,
            agent,
            account,
            model,
            effort,
        } => {
            let edits = crate::session_request::Edits {
                agent,
                account,
                model,
                effort,
            };
            let started = crate::session_request::answer(state, &asked, approve, edits).await?;
            Ok(started.map(|session| Event::SessionOpened {
                id: request_id,
                session: state.wire_session(session),
            }))
        }

        RequestBody::UpdateSettings {
            auto_pull_repositories,
            auto_delete_unresumable_archives,
            auto_approve_session_requests,
            auto_approve_resume_requests,
            auto_approve_account_switches,
            auto_approve_handoffs,
            ask_questions_as_choices,
            interface_language,
            auto_select_account,
            auto_switch_on_usage_limit,
            sidebar_order_by_status,
            auto_start_quota_windows,
            default_clone_dir,
            git_workflow,
            claude_compatibility,
        } => {
            update_settings(
                state,
                SettingsChanges {
                    auto_pull_repositories,
                    auto_delete_unresumable_archives,
                    auto_approve_session_requests,
                    auto_approve_resume_requests,
                    auto_approve_account_switches,
                    auto_approve_handoffs,
                    ask_questions_as_choices,
                    interface_language,
                    auto_select_account,
                    auto_switch_on_usage_limit,
                    sidebar_order_by_status,
                    auto_start_quota_windows,
                    default_clone_dir,
                    git_workflow: git_workflow.unwrap_or_default(),
                    claude_compatibility: claude_compatibility.unwrap_or_default(),
                },
            )?;
            Ok(None)
        }

        RequestBody::RefreshGitStatus { console } => {
            refresh_git_status(state, &console)?;
            Ok(None)
        }

        RequestBody::PullProjectBranch { project } => {
            let project = state
                .store
                .get_project(&project)?
                .ok_or_else(|| CodedError::unknown_project(&project))?;
            git_status::pull_project_now(state, project);
            Ok(None)
        }

        RequestBody::CreateShellTerminal {
            project,
            worktree,
            path,
        } => {
            let shell_terminal = crate::shell_terminal::create(
                state,
                project,
                worktree,
                path,
                crate::browse::git::GitEnv::from_shell,
            )
            .await?;
            Ok(Some(Event::ShellTerminalCreated {
                id: request_id,
                shell_terminal,
            }))
        }

        RequestBody::RenameShellTerminal {
            shell_terminal,
            title,
        } => {
            crate::shell_terminal::rename(state, &shell_terminal, required_text("title", &title)?)?;
            Ok(None)
        }

        RequestBody::CloseShellTerminal { shell_terminal } => {
            crate::shell_terminal::close(state, &shell_terminal).await?;
            Ok(None)
        }

        RequestBody::OpenProjectTerminal { project } => {
            let project = state
                .store
                .get_project(&project)?
                .ok_or_else(|| CodedError::unknown_project(&project))?;
            let path = project.path;
            tokio::task::spawn_blocking(move || open_terminal::open_directory(Path::new(&path)))
                .await??;
            Ok(None)
        }

        RequestBody::OpenProjectDirectory { project } => {
            let project = state
                .store
                .get_project(&project)?
                .ok_or_else(|| CodedError::unknown_project(&project))?;
            let path = project.path;
            tokio::task::spawn_blocking(move || open_directory::open(Path::new(&path))).await??;
            Ok(None)
        }

        RequestBody::RevealProjectPath {
            project,
            path,
            worktree,
        }
        | RequestBody::OpenProjectPath {
            project,
            path,
            worktree,
        } => {
            let state = Arc::clone(state);
            tokio::task::spawn_blocking(move || {
                let path = crate::browse::resolve_project_path(
                    &state,
                    &project,
                    worktree.as_deref(),
                    &path,
                    &crate::browse::git::GitEnv::from_shell,
                )?;
                open_directory::open_path(&path, reveal)
            })
            .await??;
            Ok(None)
        }

        RequestBody::ProbePathLinks { project, paths } => {
            let state = Arc::clone(state);
            let _permit = crate::path_links::PROBE_PERMITS.acquire().await?;
            let links = tokio::task::spawn_blocking(move || {
                crate::path_links::probe(&state, project.as_deref(), paths)
            })
            .await??;
            Ok(Some(Event::PathLinks {
                id: request_id,
                links,
            }))
        }

        RequestBody::OpenPathLink { project, path } => {
            let state = Arc::clone(state);
            tokio::task::spawn_blocking(move || {
                crate::path_links::open(&state, project.as_deref(), &path)
            })
            .await??;
            Ok(None)
        }

        RequestBody::PushProjectBranch { project } => {
            let project = state
                .store
                .get_project(&project)?
                .ok_or_else(|| CodedError::unknown_project(&project))?;
            let state = Arc::clone(state);
            tokio::task::spawn_blocking(move || git_push::push_current_branch(&state, &project))
                .await??;
            Ok(None)
        }

        RequestBody::SwitchProjectBranch { project, branch } => {
            let project = state
                .store
                .get_project(&project)?
                .ok_or_else(|| CodedError::unknown_project(&project))?;
            let state = Arc::clone(state);
            tokio::task::spawn_blocking(move || {
                git_switch::switch_branch(&state, &project, &branch)
            })
            .await??;
            Ok(None)
        }

        RequestBody::RestoreProjectChanges { project } => {
            let project = state
                .store
                .get_project(&project)?
                .ok_or_else(|| CodedError::unknown_project(&project))?;
            let state = Arc::clone(state);
            tokio::task::spawn_blocking(move || git_restore::restore_project(&state, &project))
                .await??;
            Ok(None)
        }

        RequestBody::CreateAccount {
            agent,
            name,
            config_dir,
        } => {
            create_account(state, agent, name, config_dir)?;
            Ok(None)
        }

        RequestBody::UpdateAccount {
            account,
            name,
            config_dir,
        } => {
            // A sharing account's directory is brought back to its own before it is let go, once
            // the new directory is known to be acceptable.
            let _sharing = match &config_dir {
                Some(dir) => {
                    let agent = state
                        .store
                        .get_account(&account)?
                        .ok_or_else(|| CodedError::unknown_account(&account))?
                        .agent;
                    let dir = normalize_account_dir(agent, dir)?;
                    check_account_dir_unique(state, agent, &dir, Some(&account))?;
                    if let Some(name) = &name {
                        let name = normalize_account_name(name)?;
                        check_account_name_unique(state, agent, &name, Some(&account))?;
                    }
                    crate::shared_config::before_repoint(state, &account, &dir).await?
                }
                None => None,
            };
            update_account(state, &account, name, config_dir)?;
            Ok(None)
        }

        RequestBody::SetCodexApprovalReviewer { reviewer } => {
            CODEX_REVIEWER.set(state, reviewer)?;
            Ok(None)
        }

        RequestBody::SetClaudeApprovalMode { mode } => {
            CLAUDE_MODE.set(state, mode)?;
            Ok(None)
        }

        RequestBody::SetAccountDisabled {
            agent,
            account,
            disabled,
        } => {
            set_account_disabled(state, agent, account.as_deref(), disabled)?;
            Ok(None)
        }

        RequestBody::SetAccountSharing { account, share } => {
            crate::shared_config::set(state, account, share).await?;
            Ok(None)
        }

        RequestBody::SetDefaultAccountName { agent, name } => {
            set_default_account_name(state, agent, &name)?;
            Ok(None)
        }

        RequestBody::DeleteAccount {
            account,
            trash_directory,
        } => {
            // Checked before anything is changed, sharing turned off included, so a refusal
            // leaves everything as it was.
            // Once the pre-check has passed, and until the removal ends however it does, no sign-in
            // starts for the account: it would write into the directory after its sign-in was ended.
            let _removing = if trash_directory {
                crate::account_trash::directory_to_trash(state, &account).await?;
                state
                    .store
                    .get_account(&account)?
                    .map(|account| state.begin_account_removal(&account))
            } else {
                None
            };
            // A sharing account's directory is brought back to its own before it is let go, so it
            // is never a directory of links into the default one that goes to the Trash.
            let _sharing = crate::shared_config::before_delete(state, &account).await?;
            if trash_directory {
                // The account is locked now, so it cannot be repointed and no session of it starts
                // from here on: what was judged before the lock is judged again.
                // The directory goes first, so a failure keeps the account for a retry.
                crate::account_trash::move_directory(state, &account).await?;
            }
            delete_account(state, &account)?;
            Ok(None)
        }

        RequestBody::InspectAccountTrash { account } => Ok(Some(Event::AccountTrashInspected {
            id: request_id,
            outcome: crate::account_trash::inspect(state, &account).await?,
        })),

        RequestBody::AddAgent { agent } => {
            add_agent(state, agent)?;
            Ok(None)
        }

        RequestBody::ReorderAgents { agents } => {
            reorder_agents(state, &agents)?;
            Ok(None)
        }

        RequestBody::ReorderAccounts { agent, accounts } => {
            if state.store.reorder_accounts(agent, &accounts)? {
                broadcast_settings(state)?;
            }
            Ok(None)
        }

        RequestBody::RemoveAgent { agent } => {
            remove_agent(state, agent)?;
            Ok(None)
        }

        RequestBody::Shutdown => {
            state.request_shutdown();
            Ok(None)
        }
    }
}

/// A console to create, as the request carries it: account references by id, models and efforts
/// as typed.
pub struct NewConsole {
    pub name: String,
    pub default_agent: Agent,
    pub project_session_agent: Option<Agent>,
    pub claude_account_id: Option<String>,
    pub codex_account_id: Option<String>,
    pub grok_account_id: Option<String>,
    pub claude_console_session_model: Option<String>,
    pub claude_console_session_effort: Option<String>,
    pub codex_console_session_model: Option<String>,
    pub codex_console_session_effort: Option<String>,
    pub grok_console_session_model: Option<String>,
    pub grok_console_session_effort: Option<String>,
    pub claude_project_session_model: Option<String>,
    pub claude_project_session_effort: Option<String>,
    pub codex_project_session_model: Option<String>,
    pub codex_project_session_effort: Option<String>,
    pub grok_project_session_model: Option<String>,
    pub grok_project_session_effort: Option<String>,
    pub icon: Option<String>,
    pub console_session_prompt: Option<String>,
}

/// Every refusal `create_console` can make of `new`, with nothing written. The one place that
/// list lives: a caller that has to know a console will not be refused before it creates anything
/// asks here rather than repeating the checks. Returns the avatar as it is stored.
pub fn check_new_console(state: &Arc<AppState>, new: &NewConsole) -> Result<Option<String>> {
    required_text("name", &new.name)?;
    let icon = normalize_icon(new.icon.as_deref())?;
    checked_account_reference(state, Agent::Claude, new.claude_account_id.clone())?;
    checked_account_reference(state, Agent::Codex, new.codex_account_id.clone())?;
    checked_account_reference(state, Agent::Grok, new.grok_account_id.clone())?;
    Ok(icon)
}

/// Refuses a console session model or effort in `new` that the agent does not offer (see
/// `launch_settings`). Separate from [`check_new_console`], which has no wait in it, and left out
/// for a console a configuration document creates, whose values were chosen on another machine.
pub async fn check_new_console_launch(state: &Arc<AppState>, new: &NewConsole) -> Result<()> {
    let accounts = [
        &new.claude_account_id,
        &new.codex_account_id,
        &new.grok_account_id,
    ];
    let pairs = [
        // Console session pairs, then project session pairs, each by agent.
        [
            (
                &new.claude_console_session_model,
                &new.claude_console_session_effort,
            ),
            (
                &new.codex_console_session_model,
                &new.codex_console_session_effort,
            ),
            (
                &new.grok_console_session_model,
                &new.grok_console_session_effort,
            ),
        ],
        [
            (
                &new.claude_project_session_model,
                &new.claude_project_session_effort,
            ),
            (
                &new.codex_project_session_model,
                &new.codex_project_session_effort,
            ),
            (
                &new.grok_project_session_model,
                &new.grok_project_session_effort,
            ),
        ],
    ];
    for set in pairs {
        for ((agent, account), (model, effort)) in Agent::ALL.into_iter().zip(accounts).zip(set) {
            LaunchCheck::given(agent, account.clone(), model.as_deref(), effort.as_deref())
                .run(state)
                .await?;
        }
    }
    Ok(())
}

/// Creates and broadcasts a console from a configuration document, whose model and effort values
/// are stored as given rather than checked against what the agent offers. Every other field is
/// checked first, so a refusal leaves no working directory behind.
pub fn create_console(state: &Arc<AppState>, new: NewConsole) -> Result<Console> {
    let icon = check_new_console(state, &new)?;
    store_new_console(state, new, icon)
}

/// Stores and broadcasts a console [`check_new_console`] accepted, `icon` being what it returned.
fn store_new_console(
    state: &Arc<AppState>,
    new: NewConsole,
    icon: Option<String>,
) -> Result<Console> {
    let id = Uuid::new_v4().to_string();
    let workdir = state.console_workdir(&id);
    std::fs::create_dir_all(&workdir)?;
    let console = Console {
        id,
        name: new.name.trim().to_string(),
        workdir: workdir.to_string_lossy().into_owned(),
        default_agent: new.default_agent,
        project_session_agent: new.project_session_agent,
        claude_account_id: new.claude_account_id,
        codex_account_id: new.codex_account_id,
        grok_account_id: new.grok_account_id,
        claude_console_session_model: normalize_launch_setting(
            new.claude_console_session_model.as_deref(),
        ),
        claude_console_session_effort: normalize_launch_setting(
            new.claude_console_session_effort.as_deref(),
        ),
        codex_console_session_model: normalize_launch_setting(
            new.codex_console_session_model.as_deref(),
        ),
        codex_console_session_effort: normalize_launch_setting(
            new.codex_console_session_effort.as_deref(),
        ),
        grok_console_session_model: normalize_launch_setting(
            new.grok_console_session_model.as_deref(),
        ),
        grok_console_session_effort: normalize_launch_setting(
            new.grok_console_session_effort.as_deref(),
        ),
        claude_project_session_model: normalize_launch_setting(
            new.claude_project_session_model.as_deref(),
        ),
        claude_project_session_effort: normalize_launch_setting(
            new.claude_project_session_effort.as_deref(),
        ),
        codex_project_session_model: normalize_launch_setting(
            new.codex_project_session_model.as_deref(),
        ),
        codex_project_session_effort: normalize_launch_setting(
            new.codex_project_session_effort.as_deref(),
        ),
        grok_project_session_model: normalize_launch_setting(
            new.grok_project_session_model.as_deref(),
        ),
        grok_project_session_effort: normalize_launch_setting(
            new.grok_project_session_effort.as_deref(),
        ),
        icon,
        console_session_prompt: trimmed_or_none(new.console_session_prompt.as_deref()),
        created_at: now_millis(),
    };
    // Written before the record is stored: nothing else can touch a console no one has been told
    // of yet, and a failed write then leaves no console behind.
    mcp::console_instructions::write_console_session_instructions_from_settings(
        &state.store,
        &console,
    )?;
    state.store.insert_console(&console)?;
    state.broadcast(Event::ConsoleUpserted {
        console: console.clone(),
    });
    Ok(console)
}

/// What `update_console` changes; a field left at its default is left alone.
#[derive(Default)]
pub struct ConsoleChanges {
    pub name: Option<String>,
    pub default_agent: Option<Agent>,
    /// `Some(None)` makes project sessions follow `default_agent` again.
    pub project_session_agent: Present<Agent>,
    /// Each agent's account by id; `Some(None)` is back to that agent's default account.
    pub accounts: HashMap<Agent, Option<String>>,
    pub launch: HashMap<Agent, LaunchChange>,
    /// Each agent's project session model and effort.
    pub project_launch: HashMap<Agent, LaunchChange>,
    /// `Some(None)` clears the avatar.
    pub icon: Present<String>,
    /// `Some(None)` clears the user's addition to the console session's instructions.
    pub console_session_prompt: Present<String>,
}

/// Applies `changes` to the console `id`, broadcasts it and returns it as stored. Every field is
/// checked first, with nothing written yet. An absent reference is not checked at all: saving a
/// console with a field untouched succeeds whatever has happened to the account it names since, and
/// a model or effort that is not being changed is not checked against the agent's list either.
///
/// Asking the agent for its models can take a while, so it is done before the record is read for
/// the write: the change is applied to the console as it is once the answers are in, and the
/// checks are made again only if an edit meanwhile moved the account one of them was made under.
pub async fn update_console(
    state: &Arc<AppState>,
    id: &str,
    changes: ConsoleChanges,
) -> Result<Console> {
    let name = changes
        .name
        .as_deref()
        .map(|name| required_text("name", name))
        .transpose()?;
    let icon = changes
        .icon
        .as_ref()
        .map(|icon| normalize_icon(icon.as_deref()))
        .transpose()?;
    let console_session_prompt = changes
        .console_session_prompt
        .as_ref()
        .map(|prompt| trimmed_or_none(prompt.as_deref()));
    let mut accounts = HashMap::new();
    for (agent, reference) in &changes.accounts {
        accounts.insert(
            *agent,
            checked_account_reference(state, *agent, reference.clone())?,
        );
    }
    let mut verified = Vec::new();
    loop {
        let mut console = state
            .store
            .get_console(id)?
            .ok_or_else(|| CodedError::unknown_console(id))?;
        if let Some(name) = &name {
            console.name = name.clone();
        }
        if let Some(default_agent) = changes.default_agent {
            console.default_agent = default_agent;
        }
        if let Some(project_session_agent) = changes.project_session_agent {
            console.project_session_agent = project_session_agent;
        }
        // Only sessions opened afterwards take a new reference; each existing one keeps the
        // account and the directory it was started with (see `Session::account_id`).
        for (agent, reference) in &accounts {
            *console.account_mut(*agent) = reference.clone();
        }
        // Read when a console session is opened or resumed; one already running keeps the
        // values it was started with.
        let mut pending = Vec::new();
        for (agent, change) in &changes.launch {
            let account_id = console.account(*agent).clone();
            let (model, effort) = console.launch_mut(*agent);
            let check = launch_settings::apply_change(*agent, account_id, model, effort, change);
            if !check.is_empty() && !verified.contains(&check) {
                pending.push(check);
            }
        }
        for (agent, change) in &changes.project_launch {
            let account_id = console.account(*agent).clone();
            let (model, effort) = console.project_launch_mut(*agent);
            let check = launch_settings::apply_change(*agent, account_id, model, effort, change);
            if !check.is_empty() && !verified.contains(&check) {
                pending.push(check);
            }
        }
        if !pending.is_empty() {
            for check in pending {
                check.run(state).await?;
                verified.push(check);
            }
            continue;
        }
        if let Some(icon) = &icon {
            console.icon = icon.clone();
        }
        if let Some(prompt) = &console_session_prompt {
            console.console_session_prompt = prompt.clone();
        }
        state.store.update_console(&console)?;
        // Read back rather than sent as `console`: the record is what clients are told it is.
        let stored = state
            .store
            .get_console(id)?
            .ok_or_else(|| CodedError::unknown_console(id))?;
        state.broadcast(Event::ConsoleUpserted {
            console: stored.clone(),
        });
        // After the record is stored, so the file never holds text that was not saved. Every
        // agent's new console sessions read the change, and a resumed one does on Claude Code and
        // Codex, never on Grok. A failure here is logged, not returned, since the change is saved
        // and told to every client: the next launch's refresh writes the file.
        if let Err(err) =
            mcp::console_instructions::refresh_console_session_instructions(&state.store, id)
        {
            tracing::warn!(console = %id, %err, "refreshing the console's instruction file failed");
        }
        return Ok(stored);
    }
}

/// A request that lacks a field its kind needs. `field` is the wire name, passed as is.
fn field_required(field: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::FIELD_REQUIRED,
        format!("`{field}` is required"),
        &[("field", field)],
    )
}

/// A name or title as stored: trimmed, and refused as a missing `field` if that leaves nothing.
fn required_text(field: &str, text: &str) -> Result<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(field_required(field));
    }
    Ok(trimmed.to_string())
}

const PROJECT_NAME_LIMIT: usize = 100;
const PROJECT_DESCRIPTION_LIMIT: usize = 2_000;

fn check_text_length(field: &str, text: &str, limit: usize) -> Result<()> {
    if text.chars().count() > limit {
        return Err(CodedError::raised(
            error_code::FIELD_TOO_LONG,
            format!("`{field}` must be at most {limit} characters"),
            &[("field", field), ("limit", &limit.to_string())],
        ));
    }
    Ok(())
}

/// What the user typed for a project's directory, as the absolute, lexically normalised path that is
/// stored. A relative path is refused: it would mean a different directory whenever the daemon's own
/// working directory differed, and nothing that compares project paths — the trusted directories
/// among them — could read it.
pub(crate) fn absolute_path(text: &str) -> Result<PathBuf> {
    let path = hostfs::expand(text);
    if !path.is_absolute() {
        return Err(CodedError::raised(
            error_code::PATH_NOT_ABSOLUTE,
            format!("`{text}` is not an absolute path; give the full path or start it with `~/`"),
            &[("path", text)],
        ));
    }
    Ok(hostfs::lexically_normalise(&path))
}

/// A project's tags as stored: each trimmed, the empty ones dropped, and a later tag dropped when
/// an earlier one matches it ignoring case (the first spelling wins). The order is the user's.
fn normalise_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tags.into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty() && seen.insert(tag.to_lowercase()))
        .collect()
}

/// One association request. A struct rather than a parameter list because the callers differ in
/// more than one field — the user's control-socket request, the console session's tool call, and a
/// configuration import — and most of the fields are optional strings that would otherwise be
/// positional.
pub struct AddProjectRequest {
    pub console_id: String,
    pub source: ProjectSource,
    pub path: Option<String>,
    pub remote_url: Option<String>,
    pub name: Option<String>,
    pub default_agent: Option<Agent>,
    pub settings: NewProjectSettings,
    /// Whether an absent `default_agent` is detected from each directory; false stores none.
    pub detect_default_agent: bool,
    pub tags: Option<Vec<String>>,
    /// Stored trimmed; blank is none. Like `name`, it applies only when a single project is added.
    pub description: Option<String>,
    /// Whether the settings' models and efforts are checked against what the agent offers. Not for
    /// an imported document, whose values were chosen on the machine it came from, and which must
    /// not lose a project because this one's agent no longer lists a model.
    pub check_launch: bool,
    /// Imported legacy text must survive configuration roundtrips, including overlong values.
    pub check_text_limits: bool,
}

/// What a project is associated with beyond where it is, as the request carries it: an account
/// reference is checked, and a model or effort trimmed with blank read as unset, before anything
/// is stored. Every field absent is the default, and what the console session's tool call sends.
#[derive(Default)]
pub struct NewProjectSettings {
    pub pinned: bool,
    pub claude_account_id: ProjectAccount,
    pub codex_account_id: ProjectAccount,
    pub grok_account_id: ProjectAccount,
    pub claude_session_model: Option<String>,
    pub claude_session_effort: Option<String>,
    pub codex_session_model: Option<String>,
    pub codex_session_effort: Option<String>,
    pub grok_session_model: Option<String>,
    pub grok_session_effort: Option<String>,
    pub git_workflow: GitWorkflowOverride,
}

impl NewProjectSettings {
    /// The account setting for `agent`, for a caller that fills them in agent by agent.
    pub fn account_mut(&mut self, agent: Agent) -> &mut ProjectAccount {
        match agent {
            Agent::Claude => &mut self.claude_account_id,
            Agent::Codex => &mut self.codex_account_id,
            Agent::Grok => &mut self.grok_account_id,
        }
    }

    /// The model and effort for `agent`.
    pub fn launch(&self, agent: Agent) -> (&Option<String>, &Option<String>) {
        match agent {
            Agent::Claude => (&self.claude_session_model, &self.claude_session_effort),
            Agent::Codex => (&self.codex_session_model, &self.codex_session_effort),
            Agent::Grok => (&self.grok_session_model, &self.grok_session_effort),
        }
    }

    pub fn launch_mut(&mut self, agent: Agent) -> (&mut Option<String>, &mut Option<String>) {
        match agent {
            Agent::Claude => (
                &mut self.claude_session_model,
                &mut self.claude_session_effort,
            ),
            Agent::Codex => (
                &mut self.codex_session_model,
                &mut self.codex_session_effort,
            ),
            Agent::Grok => (&mut self.grok_session_model, &mut self.grok_session_effort),
        }
    }
}

/// Checks the models and efforts a new project is given against what each agent offers, unless
/// `check` is false. Runs once the directory or repository is known to be usable and before
/// anything is cloned or stored, since asking an agent can take a while and a refusal must leave
/// nothing behind.
async fn check_new_project_launch(
    state: &Arc<AppState>,
    console: &Console,
    settings: &NewProjectSettings,
    accounts: [&ProjectAccount; 3],
    check: bool,
) -> Result<()> {
    if !check {
        return Ok(());
    }
    for (agent, account) in Agent::ALL.into_iter().zip(accounts) {
        let (model, effort) = settings.launch(agent);
        LaunchCheck::given(
            agent,
            console.resolve_project_account(agent, account),
            model.as_deref(),
            effort.as_deref(),
        )
        .behind(console.project_launch(agent).0)
        .run(state)
        .await?;
    }
    Ok(())
}

pub async fn add_project(
    state: &Arc<AppState>,
    request: AddProjectRequest,
) -> Result<Vec<Project>> {
    let AddProjectRequest {
        console_id,
        source,
        path,
        remote_url,
        name,
        default_agent,
        settings,
        detect_default_agent,
        tags,
        description,
        check_launch,
        check_text_limits,
    } = request;
    let console = state
        .store
        .get_console(&console_id)?
        .ok_or_else(|| CodedError::unknown_console(&console_id))?;
    // Blank is no name: it is ignored for a parent directory, and a single project takes its
    // directory's own.
    let name = name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());
    let description = trimmed_or_none(description.as_deref());
    let check_text = || -> Result<()> {
        if check_text_limits {
            if let Some(name) = &name {
                check_text_length("name", name, PROJECT_NAME_LIMIT)?;
            }
            if let Some(description) = &description {
                check_text_length("description", description, PROJECT_DESCRIPTION_LIMIT)?;
            }
        }
        Ok(())
    };
    // A clone must not leave a directory behind after a text validation failure. Parent requests
    // first discover how many projects there are, because several ignore the supplied text.
    if source != ProjectSource::Parent {
        check_text()?;
    }
    // Checked before anything is cloned or stored, so a refusal leaves nothing behind.
    let claude_account_id =
        checked_project_account(state, Agent::Claude, settings.claude_account_id.clone())?;
    let codex_account_id =
        checked_project_account(state, Agent::Codex, settings.codex_account_id.clone())?;
    let grok_account_id =
        checked_project_account(state, Agent::Grok, settings.grok_account_id.clone())?;
    let accounts = [&claude_account_id, &codex_account_id, &grok_account_id];

    let directories: Vec<PathBuf> = match source {
        ProjectSource::Local => {
            let path = absolute_path(&path.ok_or_else(|| field_required("path"))?)?;
            hostfs::require_directory(&path)?;
            check_new_project_launch(state, &console, &settings, accounts, check_launch).await?;
            vec![path]
        }
        ProjectSource::Parent => {
            let parent = absolute_path(&path.ok_or_else(|| field_required("path"))?)?;
            let repos = hostfs::discover_repos(&parent)?;
            if repos.is_empty() {
                return Err(CodedError::raised(
                    error_code::NO_REPOSITORIES_FOUND,
                    format!(
                        "no git repositories were found directly under `{}`",
                        parent.display()
                    ),
                    &[("path", &parent.to_string_lossy())],
                ));
            }
            if repos.len() == 1 {
                check_text()?;
            }
            check_new_project_launch(state, &console, &settings, accounts, check_launch).await?;
            repos
        }
        ProjectSource::Git => {
            let url = remote_url
                .clone()
                .ok_or_else(|| field_required("remote_url"))?;
            let parent = match path {
                Some(path) => absolute_path(&path)?,
                None => PathBuf::from(state.store.default_clone_dir()?),
            };
            check_new_project_launch(state, &console, &settings, accounts, check_launch).await?;
            let cloned =
                tokio::task::spawn_blocking(move || hostfs::clone_repo(&url, &parent)).await??;
            vec![cloned]
        }
    };

    // A caller-supplied name only applies when a single project is being associated; a parent
    // directory yields many, and each takes its own directory's name.
    let explicit_name = if directories.len() == 1 { name } else { None };

    let tags = normalise_tags(tags.unwrap_or_default());
    let description = if directories.len() == 1 {
        description
    } else {
        None
    };

    let mut associated = associated_directories(state, &console_id).await?;
    let remote_urls = directory_remote_urls(
        &associated,
        source,
        &directories,
        git_remote::read_remote_url,
    )
    .await?;

    let mut added = Vec::new();
    for directory in directories {
        let path_text = directory.to_string_lossy().into_owned();
        if associated.contains(&directory) {
            continue;
        }
        // A clone keeps the URL it was given; the other sources take the one read above.
        let project_remote_url = match source {
            ProjectSource::Git => remote_url.clone(),
            ProjectSource::Local | ProjectSource::Parent => remote_urls.get(&directory).cloned(),
        };
        let project_name = explicit_name.clone().unwrap_or_else(|| {
            directory
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path_text.clone())
        });
        let project = Project {
            id: Uuid::new_v4().to_string(),
            console_id: console_id.clone(),
            host_id: LOCAL_HOST_ID.to_string(),
            name: project_name,
            path: path_text,
            default_agent: default_agent.or_else(|| {
                detect_default_agent
                    .then(|| detect_project_agent(state, &directory))
                    .flatten()
            }),
            source,
            remote_url: project_remote_url,
            claude_account_id: claude_account_id.clone(),
            codex_account_id: codex_account_id.clone(),
            grok_account_id: grok_account_id.clone(),
            claude_session_model: normalize_launch_setting(
                settings.claude_session_model.as_deref(),
            ),
            claude_session_effort: normalize_launch_setting(
                settings.claude_session_effort.as_deref(),
            ),
            codex_session_model: normalize_launch_setting(settings.codex_session_model.as_deref()),
            codex_session_effort: normalize_launch_setting(
                settings.codex_session_effort.as_deref(),
            ),
            grok_session_model: normalize_launch_setting(settings.grok_session_model.as_deref()),
            grok_session_effort: normalize_launch_setting(settings.grok_session_effort.as_deref()),
            trust_consent: false,
            pinned: settings.pinned,
            tags: tags.clone(),
            description: description.clone(),
            git_workflow: settings.git_workflow,
        };
        state.store.insert_project(&project)?;
        associated.insert(&directory);
        crate::git_watch::add(state, &project);
        state.broadcast(Event::ProjectUpserted {
            project: project.clone(),
        });
        added.push(project);
    }
    if added.is_empty() {
        return Err(CodedError::raised(
            error_code::ALL_PROJECTS_ALREADY_ADDED,
            "every directory found is already associated with this console",
            &[],
        ));
    }
    Ok(added)
}

/// What `update_project` changes; a field left at its default is left alone.
#[derive(Default)]
pub struct ProjectChanges {
    pub name: Option<String>,
    /// `Some(None)` is back to following the console's project session agent (else its
    /// `default_agent`).
    pub default_agent: Present<Agent>,
    pub accounts: HashMap<Agent, ProjectAccount>,
    pub launch: HashMap<Agent, LaunchChange>,
    pub pinned: Option<bool>,
    /// Replaces the tags wholesale; refused beside `add_tags` or `remove_tags`.
    pub tags: Option<Vec<String>>,
    pub add_tags: Vec<String>,
    /// Matched ignoring case.
    pub remove_tags: Vec<String>,
    /// `Some(None)` clears it; blank text clears it too.
    pub description: Present<String>,
    pub git_workflow: GitWorkflowOverrideChanges,
}

/// Applies `changes` to the project `id`, broadcasts it and returns it as stored. Every reference
/// and setting is checked first, with nothing written yet; an absent one is not checked at all,
/// as for a console. The agent's models are asked for before the record is read for the write, so an
/// edit made in the meantime is not lost.
pub async fn update_project(
    state: &Arc<AppState>,
    id: &str,
    changes: ProjectChanges,
) -> Result<Project> {
    if changes.tags.is_some() && !(changes.add_tags.is_empty() && changes.remove_tags.is_empty()) {
        let other = if changes.add_tags.is_empty() {
            "remove_tags"
        } else {
            "add_tags"
        };
        return Err(CodedError::raised(
            error_code::CONFLICTING_FIELDS,
            format!("`tags` and `{other}` cannot both be given"),
            &[("first", "tags"), ("second", other)],
        ));
    }
    let name = changes
        .name
        .as_deref()
        .map(|name| required_text("name", name))
        .transpose()?;
    let description = changes
        .description
        .as_ref()
        .map(|description| trimmed_or_none(description.as_deref()));
    let mut accounts = HashMap::new();
    for (agent, account) in &changes.accounts {
        accounts.insert(
            *agent,
            checked_project_account(state, *agent, account.clone())?,
        );
    }
    let mut verified = Vec::new();
    loop {
        let mut project = state
            .store
            .get_project(id)?
            .ok_or_else(|| CodedError::unknown_project(id))?;
        let console = state
            .store
            .get_console(&project.console_id)?
            .ok_or_else(|| CodedError::unknown_console(&project.console_id))?;
        // Compare against the latest record on every pass: unchanged legacy text must not block
        // unrelated settings, including after a launch check has yielded.
        if let Some(name) = &name {
            if name != project.name.trim() {
                check_text_length("name", name, PROJECT_NAME_LIMIT)?;
                project.name = name.clone();
            }
        }
        if let Some(description) = &description {
            if description != &trimmed_or_none(project.description.as_deref()) {
                if let Some(description) = description {
                    check_text_length("description", description, PROJECT_DESCRIPTION_LIMIT)?;
                }
                project.description = description.clone();
            }
        }
        // Absent means "leave it alone", an explicit null means "go back to inheriting the
        // console's project session agent" — the two have to stay distinguishable, or a project's
        // own default could be set but never cleared.
        if let Some(default_agent) = changes.default_agent {
            project.default_agent = default_agent;
        }
        // Only sessions opened afterwards take a new reference, model or effort; each existing
        // one keeps the account and directory it was started with (see `Session::account_id`)
        // and the process it is running.
        for (agent, reference) in &accounts {
            *project.account_mut(*agent) = reference.clone();
        }
        let mut pending = Vec::new();
        for (agent, change) in &changes.launch {
            let account_id = console.resolve_project_account(*agent, project.account(*agent));
            let (model, effort) = project.launch_mut(*agent);
            let check = launch_settings::apply_change(*agent, account_id, model, effort, change)
                .behind(console.project_launch(*agent).0);
            if !check.is_empty() && !verified.contains(&check) {
                pending.push(check);
            }
        }
        if !pending.is_empty() {
            for check in pending {
                check.run(state).await?;
                verified.push(check);
            }
            continue;
        }
        if let Some(pinned) = changes.pinned {
            project.pinned = pinned;
        }
        if let Some(tags) = &changes.tags {
            project.tags = normalise_tags(tags.clone());
        } else if !changes.add_tags.is_empty() || !changes.remove_tags.is_empty() {
            let removed: HashSet<String> = changes
                .remove_tags
                .iter()
                .map(|tag| tag.trim().to_lowercase())
                .collect();
            let mut tags = std::mem::take(&mut project.tags);
            tags.retain(|tag| !removed.contains(&tag.to_lowercase()));
            tags.extend(changes.add_tags.iter().cloned());
            project.tags = normalise_tags(tags);
        }
        project.git_workflow = changes.git_workflow.applied_to(project.git_workflow);
        state.store.update_project(&project)?;
        // Read back rather than sent as `project`: the stored record carries what this write
        // does not touch (the remote URL, the trust consent), which another path may have just
        // changed.
        let stored = state
            .store
            .get_project(id)?
            .ok_or_else(|| CodedError::unknown_project(id))?;
        state.broadcast(Event::ProjectUpserted {
            project: stored.clone(),
        });
        return Ok(stored);
    }
}

/// The directories this console already has a project at, told apart by what they are on the file
/// system rather than by their paths' spelling: a project added through a link, or in another
/// case on a case-insensitive file system, is the same project as one added by the real path.
/// The stored paths are read under the store's lock and looked at on the file system after it is
/// released, on a blocking thread, so a directory on a stalled mount never holds the store.
async fn associated_directories(
    state: &AppState,
    console_id: &str,
) -> Result<hostfs::DirectoryIdentities> {
    let paths = state.store.project_paths(console_id)?;
    Ok(identities_within(
        state,
        console_id,
        paths,
        ASSOCIATED_STAT_DEADLINE,
        |paths| hostfs::DirectoryIdentities::from_paths(paths.iter().map(String::as_str)),
    )
    .await?)
}

/// How long looking at the stored project paths on the file system may take. Past it a project on
/// a mount that does not answer would hold every association to the console, so the set falls
/// back to the paths' text.
const ASSOCIATED_STAT_DEADLINE: Duration = Duration::from_secs(10);

/// The set `build` makes from `paths` on a blocking thread, or, once `deadline` passes, one made
/// from their text alone. The thread is left to finish on its own, and is recorded on the state
/// until it does: while it runs, later calls for the console take the text alone at once instead
/// of starting another thread that would be stuck the same way.
async fn identities_within(
    state: &AppState,
    console_id: &str,
    paths: Vec<String>,
    deadline: Duration,
    build: impl FnOnce(&[String]) -> hostfs::DirectoryIdentities + Send + 'static,
) -> Result<hostfs::DirectoryIdentities, tokio::task::JoinError> {
    let from_text =
        |paths: &[String]| hostfs::DirectoryIdentities::from_text(paths.iter().map(String::as_str));
    if state.stalled_directory_passes.is_stuck(console_id) {
        return Ok(from_text(&paths));
    }
    let text_only = paths.clone();
    let alive = Arc::new(());
    let weak = Arc::downgrade(&alive);
    let built = tokio::task::spawn_blocking(move || {
        let identities = build(&paths);
        drop(alive);
        identities
    });
    match tokio::time::timeout(deadline, built).await {
        Ok(joined) => joined,
        Err(_) => {
            tracing::warn!(console = %console_id, "looking at the project directories timed out, comparing their paths as text");
            state
                .stalled_directory_passes
                .record(console_id.to_string(), weak);
            Ok(from_text(&text_only))
        }
    }
}

/// The remote URL `read` finds for each of `directories` that has one, for the sources whose
/// projects take it from their directory; a clone has the URL it was given and reads nothing.
///
/// Read only for the directories that are not already associated, so a parent directory whose
/// repositories mostly are does not pay a `git` read for each one it then skips; the check in
/// `add_project`'s loop stays the authority, this one only saves the reads. They run a few at a
/// time on blocking threads (see `git_remote::read_bounded`), so a directory on a stalled mount
/// holds up its own read and no other, and a parent directory holding hundreds of repositories
/// does not take over the blocking pool. A directory whose read did not come back, because its
/// thread panicked, is simply absent: a project stored without a URL, not a failed association.
async fn directory_remote_urls(
    associated: &hostfs::DirectoryIdentities,
    source: ProjectSource,
    directories: &[PathBuf],
    read: impl Fn(&Path) -> Option<String> + Send + Sync + 'static,
) -> Result<HashMap<PathBuf, String>> {
    if source == ProjectSource::Git {
        return Ok(HashMap::new());
    }
    let unassociated: Vec<PathBuf> = directories
        .iter()
        .filter(|directory| !associated.contains(directory))
        .cloned()
        .collect();
    let urls = git_remote::read_bounded(unassociated, move |directory| {
        let url = read(&directory);
        (directory, url)
    })
    .await;
    Ok(urls
        .into_iter()
        .filter_map(|(directory, url)| Some((directory, url?)))
        .collect())
}

/// The agent to store for a project associated with no default agent chosen: the one its
/// directory is set up for (see `hostfs::detect_agent`), unless that agent has been determined
/// unavailable. Decided once, at association, and stored like any chosen default; while
/// availability is still undetermined the detected agent is kept.
fn detect_project_agent(state: &Arc<AppState>, directory: &Path) -> Option<Agent> {
    hostfs::detect_agent(directory).filter(|agent| agent_is_available(state, *agent))
}

/// Whether `agent` is not known to be missing; while availability is still undetermined it is kept.
fn agent_is_available(state: &Arc<AppState>, agent: Agent) -> bool {
    state.agent_availability_of(agent).availability != Availability::Unavailable
}

/// What a new session should be bound to: always `None` for a console session, which is never
/// bound; for a project session, whatever `requested` names, which must be a console session of
/// `console_id`, or a project session of the new session's own project that is unbound or a lead
/// session, or the request is refused with `unknown_session` — a binding changes afterwards only
/// at the session's or its owner's own request, so naming one that does not exist, or belongs to
/// another console, must not be let through to be discovered later. A project session with the
/// role of a bound one is refused as an owner for the same reason, and this is where the depth
/// limit is enforced, not only by which tools are offered: a session bound to a lead session would
/// otherwise make a third level. A session still starting is refused too: its launch may fail, and
/// a failed launch deletes the session whatever is bound to it.
fn resolve_bound_to(
    state: &Arc<AppState>,
    console_id: &str,
    project_id: Option<&str>,
    role: Role,
    requested: Option<String>,
) -> Result<Option<String>> {
    if role == Role::Console {
        return Ok(None);
    }
    let Some(target_id) = requested else {
        return Ok(None);
    };
    let target = state
        .store
        .get_session(&target_id)?
        .filter(|target| {
            target.console_id == console_id
                && target.status != SessionStatus::Starting
                && match target.role {
                    Role::Console => true,
                    Role::Project => {
                        !target.is_worker()
                            && target.project_id.is_some()
                            && target.project_id.as_deref() == project_id
                    }
                }
        })
        .ok_or_else(|| CodedError::unknown_session(&target_id))?;
    Ok(Some(target.id))
}

/// One session to open. A struct rather than a parameter list because the two callers differ in
/// more than one field — the user opening a session by hand, and the console session dispatching
/// one — and the fields that differ are all optional strings that would otherwise be positional.
#[derive(Clone)]
pub struct OpenRequest {
    pub console_id: String,
    pub project_id: Option<String>,
    pub agent: Option<Agent>,
    /// The account chosen for this session: `None` leaves it to the project's reference for the
    /// session's agent, else the console's; `Some(None)` chooses the default account outright. See
    /// [`session_account`].
    pub account: Option<Option<String>>,
    /// A model or effort override, preserving absent versus explicitly unset.
    pub model: Option<Option<String>>,
    pub effort: Option<Option<String>>,
    pub task: Option<String>,
    pub title: Option<String>,
    pub origin: Origin,
    /// The session this (project) session should report to — a console session, or an unbound
    /// project session of the same project — or `None` for one outside the orchestration. Ignored
    /// when a console session itself is being opened, which is never bound.
    pub bound_to: Option<String>,
    /// The agent's own id of a conversation started outside Octoboard, which this (project)
    /// session continues instead of starting one. What is refused is in [`checked_resume_id`] and
    /// [`locate_conversation_to_resume`].
    pub resume_agent_session_id: Option<String>,
    /// The id to record the session under, for a caller that needs it before the session exists
    /// (a handoff names its package by it); `None` makes a new one.
    pub session_id: Option<String>,
    /// The appearance of the terminal this launch will be shown in. Not recorded on the session.
    /// See `protocol::TerminalAppearance`.
    pub terminal_appearance: Option<TerminalAppearance>,
}

/// The conversation id `requested` to resume, trimmed, or `None` for none (blank counting as
/// none), refused before anything is launched or written. A console session takes none, and a
/// session reporting to another takes none: a resumed session is a plain project session. The id
/// must be a UUID, which every agent's conversation ids are (Grok reads anything else as a
/// title), and no session may record it already, in any project, because two processes must
/// never share one transcript.
fn checked_resume_id(
    state: &Arc<AppState>,
    role: Role,
    project_id: Option<&str>,
    bound_to: Option<&str>,
    requested: Option<String>,
) -> Result<Option<String>> {
    let Some(id) = requested
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
    else {
        return Ok(None);
    };
    if role == Role::Console {
        return Err(CodedError::raised(
            error_code::RESUME_FOR_CONSOLE_SESSION,
            "a console session cannot resume a conversation",
            &[],
        ));
    }
    if bound_to.is_some() {
        return Err(CodedError::raised(
            error_code::CONFLICTING_FIELDS,
            "`resume_agent_session_id` and `bound_to` cannot both be given",
            &[("first", "resume_agent_session_id"), ("second", "bound_to")],
        ));
    }
    let id = parse_conversation_id(&id)?;
    refuse_if_conversation_recorded(state, &id, project_id, None)?;
    Ok(Some(id))
}

/// `id` as the agents write a conversation id, lowercase and hyphenated, which is how a record is
/// looked for and stored however the user typed it; refused with `resume_id_invalid` when it is no
/// UUID.
pub(crate) fn parse_conversation_id(id: &str) -> Result<String> {
    match Uuid::parse_str(id) {
        Ok(parsed) => Ok(parsed.hyphenated().to_string()),
        Err(_) => Err(CodedError::raised(
            error_code::RESUME_ID_INVALID,
            format!("`{id}` is not a conversation id: it must be a UUID"),
            &[("id", id)],
        )),
    }
}

/// Refuses `id` when a session other than `except` records it as its agent's conversation id, in
/// any project and in any state: two processes must never share one transcript. A session of
/// `project_id` is named so the user can resume it instead; any other, a console session
/// included, is said to be outside the project, with its id for the UI to name.
fn refuse_if_conversation_recorded(
    state: &Arc<AppState>,
    id: &str,
    project_id: Option<&str>,
    except: Option<&str>,
) -> Result<()> {
    let Some(holder) = state.store.session_by_agent_session_id(id, except)? else {
        return Ok(());
    };
    Err(if holder.project_id.as_deref() == project_id {
        CodedError::raised(
            error_code::CONVERSATION_IN_SESSION,
            format!(
                "the session \"{}\" already holds this conversation: resume or reopen it instead",
                holder.title
            ),
            &[("id", id), ("session", &holder.id)],
        )
    } else {
        CodedError::raised(
            error_code::CONVERSATION_OUTSIDE_PROJECT,
            "this conversation already belongs to a session outside this project",
            &[("id", id), ("session", &holder.id)],
        )
    })
}

/// The directory of an account: the pinned one, else the default account's, resolved from
/// `shell_env`, the snapshot the caller took when it had a default side.
fn account_dir(
    agent: Agent,
    pinned: Option<&str>,
    shell_env: Option<&HashMap<String, String>>,
) -> PathBuf {
    match (pinned, shell_env) {
        (Some(dir), _) => PathBuf::from(dir),
        (None, Some(shell_env)) => adapter::default_account_dir(agent, shell_env),
        (None, None) => unreachable!("a default account takes the snapshot"),
    }
}

/// Refuses a conversation to resume that has no record in the account directory the session will
/// launch with: the pinned one, else the default account's, resolved from the shell snapshot.
/// Answers that snapshot (none for a pinned account) for the launch to use, so the check and the
/// launch cannot disagree about where the default account is. Blocking.
fn locate_conversation_to_resume(
    agent: Agent,
    config_dir: Option<&str>,
    id: &str,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>>,
) -> Result<Option<HashMap<String, String>>> {
    let shell_env = match config_dir {
        Some(_) => None,
        None => Some(snapshot().context("snapshotting the user's shell environment")?),
    };
    let dir = account_dir(agent, config_dir, shell_env.as_ref());
    relocate::locate(agent, &dir, id).map_err(|err| match err.downcast::<CodedError>() {
        Ok(coded) if coded.code == error_code::CONVERSATION_NOT_FOUND => CodedError::raised(
            error_code::CONVERSATION_TO_RESUME_NOT_FOUND,
            format!(
                "no conversation `{id}` of {} was found in `{}`",
                agent.label(),
                dir.display()
            ),
            &[
                ("agent", agent.label()),
                ("path", &dir.to_string_lossy()),
                ("id", id),
            ],
        ),
        Ok(coded) => anyhow::Error::new(coded),
        Err(err) => err,
    })?;
    Ok(shell_env)
}

/// Where the conversation `agent_session_id` is held, for the project `project` asking, as the
/// new-session dialog's Resume tab and the `request_resume_session` tool both ask it: a session
/// that records it first, whatever its agent or project, else the on-disk record in the default
/// account and every account of each agent that is available and added. Refused with
/// `unknown_project` for a `project` that does not exist and `resume_id_invalid` for an id that is
/// no UUID. Reads the disk only.
pub(crate) async fn locate_conversation(
    state: &Arc<AppState>,
    project: &str,
    agent_session_id: &str,
) -> Result<ConversationLocation> {
    if state.store.get_project(project)?.is_none() {
        return Err(CodedError::unknown_project(project));
    }
    let id = parse_conversation_id(agent_session_id)?;
    if let Some(holder) = state.store.session_by_agent_session_id(&id, None)? {
        return Ok(held_conversation(&holder, project));
    }
    // An undecided list offers every agent, as `refuse_unless_agent_added` does.
    let added = state.store.added_agents()?;
    let agents: Vec<Agent> = Agent::ALL
        .into_iter()
        .filter(|agent| agent_is_available(state, *agent))
        .filter(|agent| added.as_ref().is_none_or(|list| list.contains(agent)))
        .collect();
    let accounts = state.store.list_accounts()?;
    Ok(tokio::task::spawn_blocking(move || {
        find_conversation_to_resume(&agents, &accounts, &id, env_shell::launch_snapshot)
    })
    .await??
    .unwrap_or(ConversationLocation::NotFound))
}

/// Where a conversation is held that `holder` records, for the project `project` asking: an
/// archived session of it can be reopened, any other session of it is running already, and one
/// outside it (another project's, or a console session) belongs elsewhere whatever its state.
fn held_conversation(holder: &Session, project: &str) -> ConversationLocation {
    if holder.project_id.as_deref() != Some(project) {
        ConversationLocation::OutsideProject {
            session: holder.id.clone(),
        }
    } else if holder.status == SessionStatus::Archived {
        ConversationLocation::ArchivedSession {
            session: holder.id.clone(),
        }
    } else {
        ConversationLocation::LiveSession {
            session: holder.id.clone(),
        }
    }
}

/// Where the record of the conversation `id` is among the accounts a session could be opened with:
/// for each of `agents`, its default account (resolved from the shell snapshot, taken only if
/// there is one to look in) and each of its `accounts`, in the directory
/// [`locate_conversation_to_resume`] would look in. When more than one holds it, the account whose
/// record was written to last, since that is the conversation that went on, as a
/// [`ConversationLocation::Record`] (`None` for the default account, as in a session's own
/// account). A record's age is the modification time of what [`relocate::locate`] names, which for
/// Grok Build is the session's directory, so only approximately when the last write was inside it.
/// A snapshot that fails leaves the default accounts out, so as not to hide a stored account's
/// record, and is the error only when nothing was found anywhere. Blocking.
fn find_conversation_to_resume(
    agents: &[Agent],
    accounts: &[Account],
    id: &str,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>>,
) -> Result<Option<ConversationLocation>> {
    let mut snapshot = Some(snapshot);
    let mut shell_env: Option<HashMap<String, String>> = None;
    let mut snapshot_error = None;
    let mut newest: Option<(SystemTime, ConversationLocation)> = None;
    for &agent in agents {
        let default = (None, None);
        let pinned = accounts
            .iter()
            .filter(|account| account.agent == agent)
            .map(|account| (Some(account.id.clone()), Some(account.config_dir.as_str())));
        for (account, config_dir) in std::iter::once(default).chain(pinned) {
            if config_dir.is_none() {
                if let Some(take) = snapshot.take() {
                    match take() {
                        Ok(env) => shell_env = Some(env),
                        Err(err) => {
                            snapshot_error =
                                Some(err.context("snapshotting the user's shell environment"))
                        }
                    }
                }
                if shell_env.is_none() {
                    continue;
                }
            }
            let dir = account_dir(agent, config_dir, shell_env.as_ref());
            let record = match relocate::locate(agent, &dir, id) {
                Ok(record) => record,
                Err(err) => match err.downcast::<CodedError>() {
                    Ok(coded) if coded.code == error_code::CONVERSATION_NOT_FOUND => continue,
                    Ok(coded) => return Err(anyhow::Error::new(coded)),
                    Err(err) => return Err(err),
                },
            };
            let written = relocate::modified(&dir.join(record));
            if newest.as_ref().is_none_or(|(latest, _)| written > *latest) {
                newest = Some((written, ConversationLocation::Record { agent, account }));
            }
        }
    }
    match (newest, snapshot_error) {
        (Some((_, location)), _) => Ok(Some(location)),
        (None, Some(err)) => Err(err),
        (None, None) => Ok(None),
    }
}

/// A session that is recorded and published as `starting`, whose agent process is still to be
/// launched. The launch claim is held from here until [`finish_open`] has settled it, so nothing
/// else launches, switches or deletes the session meanwhile.
struct StartingSession {
    claim: crate::state::LaunchClaim,
    session: Session,
    cwd: PathBuf,
    task: Option<String>,
    /// The outside conversation the session continues, if it was opened with one.
    resume_agent_session_id: Option<String>,
    /// The login-shell snapshot that conversation's record was found with, which the launch must
    /// use too (see [`term::LaunchRequest::preresolved_shell_env`]).
    resume_shell_env: Option<HashMap<String, String>>,
    /// See [`OpenRequest::terminal_appearance`].
    terminal_appearance: Option<TerminalAppearance>,
}

/// Opens a session and returns it once it has launched: running, or with the launch's refusal
/// as the error and nothing left behind. For a caller that has nothing to show before the agent
/// is up — an agent's `new_session` tool, or the approval of a session request — which
/// would otherwise be told of a session that may yet vanish. The user's own request is
/// [`open_session_detached`].
pub async fn open_session(state: &Arc<AppState>, request: OpenRequest) -> Result<Session> {
    open_session_with(state, request, term::launch).await
}

/// [`open_session`], launching through `launch`: `term::launch` outside the tests, which cannot
/// start an agent.
pub(crate) async fn open_session_with(
    state: &Arc<AppState>,
    request: OpenRequest,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<Session> {
    let starting = begin_open(state, request).await?;
    finish_open(state, starting, launch)
        .await
        .map_err(OpenError::into_error)
}

/// What is left to do for a session [`open_session_detached`] recorded: launching its agent. It
/// starts once the reply that tells the client of the session has been queued, so that a launch
/// that fails at once cannot have the session deleted on the client before the client has been
/// told it exists. Run by [`Followup::start`]; dropping one nobody got to — a connection that went
/// away — runs it too, so the session never stays `starting` for want of it. The launch is spawned
/// on the runtime the follow-up was made on, so a drop outside any runtime does not panic.
pub struct Followup(Option<Box<dyn FnOnce() + Send>>);

impl Followup {
    pub fn start(mut self) {
        self.run();
    }

    fn run(&mut self) {
        if let Some(start) = self.0.take() {
            start();
        }
    }
}

impl Drop for Followup {
    fn drop(&mut self) {
        self.run();
    }
}

/// Opens a session and returns it as soon as it is recorded and published as `starting`, so a
/// client can show it at once: launching snapshots the user's shell environment and forks the
/// agent, which can take seconds on a busy machine. The launch is the returned [`Followup`]'s, to
/// run once the session has been handed to the client; the session's `session_upserted` then says
/// it is up, and one that could not be launched is deleted again and reported with a
/// `launch_failed` `session_notice`.
pub async fn open_session_detached(
    state: &Arc<AppState>,
    request: OpenRequest,
) -> Result<(Session, Followup)> {
    open_detached_with(state, request, term::launch).await
}

/// [`open_session_detached`], launching through `launch`: `term::launch` outside the tests, which
/// cannot start an agent.
async fn open_detached_with(
    state: &Arc<AppState>,
    request: OpenRequest,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<(Session, Followup)> {
    let starting = begin_open(state, request).await?;
    let opened = starting.session.clone();
    let state = state.clone();
    let runtime = tokio::runtime::Handle::current();
    let followup = Followup(Some(Box::new(move || {
        runtime.spawn(async move {
            let session = starting.session.clone();
            match finish_open(&state, starting, launch).await {
                Ok(_) => {}
                Err(OpenError::Launch(err)) => report_failed_launch(&state, &session, &err),
                // The process is up and the session stays; only settling it after the launch failed.
                Err(OpenError::Settling(err)) => {
                    tracing::warn!(session = %session.id, %err, "settling a launched session failed");
                }
            }
        });
    })));
    Ok((opened, followup))
}

/// Tells every client that the launch of `session` failed with `err`. The session is normally
/// already deleted; if it could not be deleted it is left interrupted, and the notice is sent all
/// the same. The notice carries the failure's own code as `reason_code` and its params beside
/// `title` and `reason` (the English account), so a client words the reason itself.
pub(crate) fn report_failed_launch(state: &Arc<AppState>, session: &Session, err: &anyhow::Error) {
    let (code, coded, reason) = error_parts(err);
    let mut pairs: Vec<(&str, &str)> = coded
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    // After the failure's own, which a later pair of the same name replaces.
    pairs.extend([
        ("title", session.title.as_str()),
        ("reason", reason.as_str()),
        ("reason_code", code.as_str()),
    ]);
    state.broadcast(
        Notice::new(
            notice_code::LAUNCH_FAILED,
            format!("could not start {}: {reason}", session.title),
            &pairs,
        )
        .about(&session.id),
    );
}

/// Why [`finish_open`] did not leave a running session.
enum OpenError {
    /// The agent could not be launched, and the session is deleted again.
    Launch(anyhow::Error),
    /// The agent is up, but recording that failed.
    Settling(anyhow::Error),
}

impl OpenError {
    fn into_error(self) -> anyhow::Error {
        match self {
            Self::Launch(err) | Self::Settling(err) => err,
        }
    }
}

/// What opening a session settles from its request before anything is awaited, launched or
/// recorded, and refuses for what cannot be opened.
struct ResolvedOpen {
    role: Role,
    cwd: PathBuf,
    project: Option<Project>,
    console: Console,
    default_title: String,
    resume_agent_session_id: Option<String>,
    bound_to: Option<String>,
    agent: Agent,
    account_id: Option<String>,
    config_dir: Option<String>,
}

/// Resolves what [`open_session`] opens from `request`, refusing an unknown console, project or
/// owner, an agent that is not available or not added, an account of another agent, a disabled
/// account, and an account whose plan quota is used up.
fn resolve_open(state: &Arc<AppState>, request: &OpenRequest) -> Result<ResolvedOpen> {
    resolve_open_with(state, request, true)
}

/// [`resolve_open`], refusing an account whose quota is used up only when `enforce_quota`.
fn resolve_open_with(
    state: &Arc<AppState>,
    request: &OpenRequest,
    enforce_quota: bool,
) -> Result<ResolvedOpen> {
    let console = state
        .store
        .get_console(&request.console_id)?
        .ok_or_else(|| CodedError::unknown_console(&request.console_id))?;

    let (role, cwd, project, default_title) = match &request.project_id {
        Some(project_id) => {
            let project = state
                .store
                .get_project(project_id)?
                .ok_or_else(|| CodedError::unknown_project(project_id))?;
            let cwd = PathBuf::from(&project.path);
            let title = project.name.clone();
            (Role::Project, cwd, Some(project), title)
        }
        None => {
            let workdir = PathBuf::from(&console.workdir);
            // The store reads the current console name when inserting the session.
            (Role::Console, workdir, None, String::new())
        }
    };

    // Before `bound_to` is resolved, which would drop a console session's request for one.
    let resume_agent_session_id = checked_resume_id(
        state,
        role,
        request.project_id.as_deref(),
        request.bound_to.as_deref(),
        request.resume_agent_session_id.clone(),
    )?;
    let bound_to = resolve_bound_to(
        state,
        &request.console_id,
        request.project_id.as_deref(),
        role,
        request.bound_to.clone(),
    )?;

    let preferred = session_agent(request.agent, project.as_ref(), &console);
    let automatic = request.account.is_none()
        && request.resume_agent_session_id.is_none()
        && state.store.auto_select_account()?;
    let picked = if automatic {
        automatic_open_choice(
            state,
            preferred,
            &console,
            project.as_ref(),
            None,
            request.agent.is_none(),
        )?
    } else {
        None
    };
    let agent = picked.as_ref().map_or(preferred, |(agent, _)| *agent);
    refuse_if_agent_unavailable(state, agent)?;
    // After the availability check, as in the UI, where "not installed" wins over "not added".
    refuse_unless_agent_added(state, agent)?;

    let (account_id, config_dir) = session_account(
        state,
        agent,
        &console,
        project.as_ref(),
        picked
            .map(|(_, account)| account)
            .or_else(|| request.account.clone()),
    )?;
    refuse_if_account_disabled(state, agent, account_id.as_deref())?;
    if enforce_quota {
        refuse_if_account_quota_exhausted(
            state,
            agent,
            account_id.as_deref(),
            config_dir.as_deref(),
        )?;
        refuse_if_account_not_signed_in(
            state,
            agent,
            account_id.as_deref(),
            config_dir.as_deref(),
        )?;
    }
    // Asked again by the launch under its claim; asked here so that nothing is recorded.
    crate::shared_config::refuse_if_changing(state, account_id.as_deref())?;
    Ok(ResolvedOpen {
        role,
        cwd,
        project,
        console,
        default_title,
        resume_agent_session_id,
        bound_to,
        agent,
        account_id,
        config_dir,
    })
}

/// Everything of opening a session that precedes launching its agent: the refusals, the record
/// (`starting`) and its publication.
async fn begin_open(state: &Arc<AppState>, request: OpenRequest) -> Result<StartingSession> {
    let ResolvedOpen {
        role,
        cwd,
        project,
        console,
        default_title,
        resume_agent_session_id,
        bound_to,
        agent,
        account_id,
        config_dir,
    } = resolve_open(state, &request)?;
    let OpenRequest {
        console_id,
        project_id,
        model,
        effort,
        task,
        title,
        origin,
        session_id,
        terminal_appearance,
        ..
    } = request;
    let resume_shell_env = match resume_agent_session_id.clone() {
        Some(id) => {
            let dir = config_dir.clone();
            tokio::task::spawn_blocking(move || {
                locate_conversation_to_resume(
                    agent,
                    dir.as_deref(),
                    &id,
                    env_shell::launch_snapshot,
                )
            })
            .await??
        }
        None => None,
    };
    let (model, effort) = check_launch_settings(
        state,
        agent,
        account_id.clone(),
        project.as_ref(),
        &console,
        model,
        effort,
        SettingsChecked::All,
    )
    .await?;
    let title = title
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty());

    let session = Session {
        id: session_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        agent,
        // Recorded with `has_conversation`, so an interrupt before the first prompt does not drop
        // the id on a relaunch (see [`resumable_agent_session_id`]).
        agent_session_id: resume_agent_session_id.clone(),
        console_id,
        project_id,
        host_id: LOCAL_HOST_ID.to_string(),
        role,
        origin,
        title: title.clone().unwrap_or_else(|| default_title.clone()),
        // Until the process is up; `finish_open` moves it to what the launch implies.
        status: SessionStatus::Starting,
        terminal_title: None,
        record_titles: Default::default(),
        background_tasks: Vec::new(),
        switching_account: false,
        default_effort: None,
        orchestration_role: None,
        context: None,
        observed_model: None,
        observed_effort: None,
        has_conversation: resume_agent_session_id.is_some(),
        // Resolved now and kept: a resume must find the transcript where the first launch put it,
        // whatever the console's reference says by then.
        account_id,
        config_dir,
        model,
        effort,
        bound_to,
        // A session is opened bound or unbound; only binding it afterwards makes it a lead session.
        lead: false,
        // Assigned by `insert_console_session` below for a console session; a project session
        // carries no colour or ordinal of its own.
        colour: None,
        ordinal: None,
        pinned: false,
        locked: false,
        started_at: now_millis(),
        ended_at: None,
    };
    let session = if role == Role::Console {
        state.store.insert_console_session(session, title)?
    } else {
        // `bound_to` was resolved before the launch check above awaited, so the owner may since
        // have been deleted, or have bound itself to a project session or been handed to one, and
        // so can no longer own sessions: a binding that lands in between must not leave a third
        // level. The store checks the owner in the insert itself, and once this row exists the
        // owner can neither be deleted nor change either way. Nothing has been launched yet.
        match &session.bound_to {
            Some(owner) => {
                if !state.store.insert_session_under_owner(&session, owner)? {
                    return Err(match state.store.get_session(owner)? {
                        None => CodedError::unknown_session(owner),
                        Some(_) => anyhow::anyhow!(
                            "`{owner}` can no longer own sessions: it was bound to a project \
                             session while this one was being opened"
                        ),
                    });
                }
            }
            None => state.store.insert_session(&session)?,
        }
        // The duplicate check ran before the awaits above, so another request may have recorded
        // the same conversation since. Nothing has been launched yet.
        if let Some(id) = &resume_agent_session_id {
            if let Err(err) = refuse_if_conversation_recorded(
                state,
                id,
                session.project_id.as_deref(),
                Some(&session.id),
            ) {
                state.store.delete_session(&session.id)?;
                return Err(err);
            }
        }
        session
    };
    let claim = state.begin_launch(&session.id)?;
    state.publish_session(&session);
    Ok(StartingSession {
        claim,
        session,
        cwd,
        task,
        resume_agent_session_id,
        resume_shell_env,
        terminal_appearance,
    })
}

/// Launches the agent of a session [`begin_open`] recorded, through `launch`, and publishes it
/// running. Without a process the session is deleted again: nothing ran, so there is nothing to
/// resume, and a session in the tree that never existed would only mislead.
async fn finish_open(
    state: &Arc<AppState>,
    starting: StartingSession,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<Session, OpenError> {
    let StartingSession {
        claim,
        session,
        cwd,
        task,
        resume_agent_session_id,
        resume_shell_env,
        terminal_appearance,
    } = starting;
    // Read inside the launch's own result: a failure here is one before anything ran, which is
    // the same as a launch that failed, and must delete the session the same way.
    let started = async {
        let recorded = state.session_record(&session.id)?;
        // Archived in the moment before the launch began: the user has already said they do not
        // want it, and there is nothing to end.
        if recorded.status != SessionStatus::Starting {
            return Ok(Some(recorded));
        }
        start_process(
            state,
            &claim,
            &session,
            &cwd,
            task.as_deref(),
            resume_agent_session_id.as_deref(),
            terminal_appearance,
            // The snapshot the conversation to resume was found with is the one the launch must use.
            move |request| {
                launch(term::LaunchRequest {
                    preresolved_shell_env: resume_shell_env,
                    ..request
                })
            },
        )
        .await
        .map(|()| None)
    }
    .await;
    match started {
        Ok(Some(archived)) => Ok(archived),
        Ok(None) => settle_started(state, &session, task.is_some()).map_err(OpenError::Settling),
        Err(err) => {
            state.revoke_mcp_tokens(&session.id);
            state.discard_outbox(&session.id);
            // The claim is held through the deletion, so nothing acts on the session half-gone.
            if let Err(discard_err) = state.discard_session(&session.id) {
                tracing::warn!(session = %session.id, err = %discard_err, "deleting a session whose launch failed failed");
                // The row stays, so it must not stay `starting`: nothing would ever move it, and
                // `starting` sessions refuse deletion and resume.
                mark_interrupted(state, &session.id);
            }
            drop(claim);
            Err(OpenError::Launch(err))
        }
    }
}

/// Moves a session that has no process out of `starting`, to the status a click resumes from.
fn mark_interrupted(state: &Arc<AppState>, id: &str) {
    if let Err(err) = state.interrupt_session_record(id) {
        tracing::warn!(session = %id, %err, "marking a session that failed to launch interrupted failed");
    }
}

/// Moves a session whose process is up from `starting` to what the launch implies, and returns its
/// record.
fn settle_started(state: &Arc<AppState>, session: &Session, has_task: bool) -> Result<Session> {
    // A session opened without a task is sitting at its prompt, not working. This also matters for
    // Codex specifically: its `SessionStart` hook does not fire until the first prompt submission,
    // so an optimistic `working` here would stay until the user typed. Conditional on it still
    // starting: the agent's own first hook may have moved it on already, and an archive that landed
    // while the process was being forked stands.
    let up = if has_task {
        SessionStatus::Working
    } else {
        SessionStatus::Idle
    };
    if !state.apply_reported_status_if(&session.id, SessionStatus::Starting, up)? {
        let record = state.session_record(&session.id)?;
        if record.status == SessionStatus::Archived {
            // Archived while the process was being forked, when there was none to end.
            if let Some(live) = state.live_session(&session.id) {
                stop_process(state, live);
            }
        }
        return Ok(record);
    }
    // Re-read rather than return the record we built: launching may have learned the agent's own
    // session id, and the stale copy has none.
    state.session_record(&session.id)
}

/// Which of the values given to [`check_launch_settings`] are held against what the agent offers.
#[derive(Clone, Copy)]
enum SettingsChecked<'a> {
    /// Every value, as for a session being opened or moved to another account.
    All,
    /// Only the values that differ from the session's own overrides `model` and `effort`: a value
    /// it already runs with is not refused because the agent has stopped offering it since.
    Changed {
        model: &'a Option<Option<String>>,
        effort: &'a Option<Option<String>>,
    },
}

/// Normalises a session's model and effort override and refuses one `agent` does not offer under
/// `account_id`, as the launch of a session does, among the values `checked` names.
#[allow(clippy::too_many_arguments)]
async fn check_launch_settings(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<String>,
    project: Option<&Project>,
    console: &Console,
    model: Option<Option<String>>,
    effort: Option<Option<String>>,
    checked: SettingsChecked<'_>,
) -> Result<(Option<Option<String>>, Option<Option<String>>)> {
    let (model, effort) = (
        normalize_session_launch_setting(model),
        normalize_session_launch_setting(effort),
    );
    // A session that chose no model takes the project's, else the console's project session one,
    // or for a console session the console's console session one.
    let effective_model = match &model {
        Some(model) => model.clone(),
        None => match project {
            Some(project) => project_launch_settings(project, console, agent).0,
            None => console.launch(agent).0.clone(),
        },
    };
    let given = |value: &Option<Option<String>>, had: Option<&Option<Option<String>>>| {
        value.clone().filter(|_| had != Some(value)).flatten()
    };
    let (had_model, had_effort) = match checked {
        SettingsChecked::All => (None, None),
        SettingsChecked::Changed { model, effort } => (Some(model), Some(effort)),
    };
    LaunchCheck {
        agent,
        account_id,
        model: given(&model, had_model),
        effort: given(&effort, had_effort),
        effective_model,
    }
    .run(state)
    .await?;
    Ok((model, effort))
}

/// The project a session belongs to, if any, and its console, which its launch settings fall
/// back on.
fn session_scope(state: &AppState, session: &Session) -> Result<(Option<Project>, Console)> {
    let project = match &session.project_id {
        Some(project_id) => Some(
            state
                .store
                .get_project(project_id)?
                .ok_or_else(|| CodedError::unknown_project(project_id))?,
        ),
        None => None,
    };
    let console = state
        .store
        .get_console(&session.console_id)?
        .ok_or_else(|| CodedError::unknown_console(&session.console_id))?;
    Ok((project, console))
}

/// Refuses a session that [`open_session`] would refuse for what `request` asks, a console session
/// included, before anything is launched or recorded: an unknown console, project or owner, an
/// agent that is not available or not added, an account of another agent, a disabled account, an
/// account whose plan quota is used up or that the last quota reading found not logged in, a model
/// or effort the agent does not offer. For a session whose opening waits on the user's answer, so
/// that a request which could never be carried out is refused to the asker rather than put to the
/// user. Returns the agent the session would be of.
pub async fn check_open(state: &Arc<AppState>, request: &OpenRequest) -> Result<Agent> {
    Ok(check_open_with(state, request, true).await?.0)
}

/// [`check_open`] for a session whose opening waits on the user's choice of account, which is not
/// refused for an account whose quota is used up or that is not logged in (the user can pick
/// another; the refusal comes at approval). Returns the account the session would open on.
pub async fn check_open_asking(
    state: &Arc<AppState>,
    request: &OpenRequest,
) -> Result<Option<String>> {
    Ok(check_open_with(state, request, false).await?.1)
}

/// The checks of [`check_open`], refusing an account whose quota is used up or that is not logged
/// in only when `enforce_quota`; returns the agent and the account the session would open on.
async fn check_open_with(
    state: &Arc<AppState>,
    request: &OpenRequest,
    enforce_quota: bool,
) -> Result<(Agent, Option<String>)> {
    let ResolvedOpen {
        project,
        console,
        agent,
        account_id,
        ..
    } = resolve_open_with(state, request, enforce_quota)?;
    check_launch_settings(
        state,
        agent,
        account_id.clone(),
        project.as_ref(),
        &console,
        request.model.clone(),
        request.effort.clone(),
        SettingsChecked::All,
    )
    .await?;
    Ok((agent, account_id))
}

/// The model and reasoning effort a user changed for a switch or a reopen, each absent to leave the
/// session's own override as it is, `Some(None)` for the agent's own default.
#[derive(Default)]
pub(crate) struct LaunchEdit {
    pub model: Option<Option<String>>,
    pub effort: Option<Option<String>>,
}

/// The model and effort overrides `session` would have with `edit` applied, each value left
/// unchanged keeping what the session has, refused for a value the agent does not offer under
/// `account_id`; `None` when there is nothing to change.
async fn checked_launch_overrides(
    state: &Arc<AppState>,
    session: &Session,
    account_id: Option<String>,
    edit: LaunchEdit,
) -> Result<Option<(Option<Option<String>>, Option<Option<String>>)>> {
    if edit.model.is_none() && edit.effort.is_none() {
        return Ok(None);
    }
    let (project, console) = session_scope(state, session)?;
    let checked = check_launch_settings(
        state,
        session.agent,
        account_id,
        project.as_ref(),
        &console,
        edit.model.or_else(|| session.model.clone()),
        edit.effort.or_else(|| session.effort.clone()),
        SettingsChecked::All,
    )
    .await?;
    Ok(Some(checked))
}

/// Why `owner` cannot own `session`, in prose for the model, or `None` when it can. These are the
/// rules every binding made after a session was opened keeps, whether the session binds itself
/// ([`bind_session`]) or its owner hands it to another ([`transfer_session`]): an owner is a
/// running console session of the session's console, or a running project session of its project
/// that can own sessions (unbound, or a lead session); and a session bound to a project session
/// owns none, so one that has sessions of its own, archived ones included since its owner can
/// reopen them, can only be bound to a console session. Orchestration stays at most two levels
/// deep, and no binding can close a cycle.
pub(crate) fn owner_refusal(
    state: &Arc<AppState>,
    session: &Session,
    owner: &Session,
) -> Result<Option<String>> {
    let named = |session: &Session| {
        format!(
            "`{}` (\"{}\")",
            session.id,
            crate::sharing::one_line(&session.title)
        )
    };
    if owner.id == session.id {
        return Ok(Some(format!("{} cannot be its own owner", named(session))));
    }
    if owner.console_id != session.console_id {
        return Ok(Some(format!(
            "{} belongs to another console, and an owner must be in the console of the session \
             it owns",
            named(owner)
        )));
    }
    if owner.role == Role::Project {
        if owner.project_id != session.project_id {
            return Ok(Some(format!(
                "{} is a session of another project, and a project session owns only sessions of \
                 its own project",
                named(owner)
            )));
        }
        if owner.is_worker() {
            return Ok(Some(format!(
                "{} reports to a session and cannot own sessions of its own: orchestration is at \
                 most two levels deep",
                named(owner)
            )));
        }
        let owned = bound_sessions(state, session)?;
        if !owned.is_empty() {
            return Ok(Some(format!(
                "{} has sessions of its own ({}), and a session bound to a project session cannot \
                 own any: only a console session can own it",
                named(session),
                owned
                    .iter()
                    .map(|owned| format!("`{}`", owned.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }
    // A session still starting is not running yet either: its launch may fail, and a failed
    // launch deletes the session whatever is bound to it.
    if owner.status.is_dormant()
        || owner.status == SessionStatus::Starting
        || !state.has_process(&owner.id)
    {
        return Ok(Some(format!("{} is not running", named(owner))));
    }
    Ok(None)
}

/// Takes the exclusive claim on `session_id` (see [`AppState::begin_switch`]) for as long as a
/// change of the owner it is bound to takes. What a handoff or an account switch holds goes stale
/// if it changes meanwhile: the owner read at the start (`bound_to` in the `OpenRequest` for a
/// bound role, `lead_of` for a lead session, [`owner_of`] for the switch's relaunch). Taken before
/// the session is read, so what is checked after it holds until the write; refused
/// ([`error_code::SESSION_CHANGING`]) when a launch, a resume, a handoff, a handover of branches, a
/// switch or another binding change of it is under way.
fn claim_for_binding(state: &Arc<AppState>, session_id: &str) -> Result<crate::state::LaunchClaim> {
    state.begin_switch(session_id).map_err(|_| {
        CodedError::raised(
            error_code::SESSION_CHANGING,
            format!(
                "`{session_id}` is being started, resumed, handed off, switched to another \
                 account, having its branches handed over or having its owner changed, so its \
                 owner cannot change now: look again once that is done"
            ),
            &[("session", session_id)],
        )
    })
}

/// Refuses ([`error_code::SESSION_OWNER_CHANGING`]) when `owner_id`, the owner a binding change
/// gains or loses a session for, has its claim held — a handoff of it would otherwise move and
/// archive it with a session bound to it meanwhile. Only looked at, not claimed: an owner is not
/// rewritten, and claims on owners would refuse concurrent binds to the same one. So the check is
/// not made under a claim, which is acceptable: a write that lands before the handoff's
/// `hand_over_binding` is moved by it in its transaction. Skipped when the owner is the session
/// itself, which [`owner_refusal`] refuses by its own reason.
fn refuse_if_owner_claimed(state: &Arc<AppState>, session_id: &str, owner_id: &str) -> Result<()> {
    if owner_id != session_id && state.is_launching(owner_id) {
        return Err(CodedError::raised(
            error_code::SESSION_OWNER_CHANGING,
            format!(
                "`{owner_id}` is being started, resumed, handed off, switched to another account \
                 or having its owner changed, so sessions cannot be bound to or moved from it \
                 now: look again once that is done"
            ),
            &[("session", session_id), ("owner", owner_id)],
        ));
    }
    Ok(())
}

/// Binds the unbound project session `session_id` to `owner_id`, at its own request, and tells
/// every client. Bound to a console session it becomes a lead session, keeping the sessions it
/// started and the tools it drives them with; bound to a project session it becomes an ordinary
/// bound session, which only one with no sessions of its own may (see [`owner_refusal`]).
///
/// Refused, with nothing changed, for a session or an owner under a claim (see
/// [`claim_for_binding`], [`refuse_if_owner_claimed`]), a console session, a session that is
/// already bound, and an owner [`owner_refusal`] refuses. Returns the session as it is now bound,
/// and the owner as it was checked.
pub fn bind_session(
    state: &Arc<AppState>,
    session_id: &str,
    owner_id: &str,
) -> Result<(Session, Session)> {
    let _claim = claim_for_binding(state, session_id)?;
    let session = state.session_record(session_id)?;
    if session.role == Role::Console {
        anyhow::bail!("a console session cannot be bound to another session");
    }
    if let Some(owner) = &session.bound_to {
        anyhow::bail!(
            "this session is already bound to `{owner}`: only an unbound session binds itself to \
             an owner"
        );
    }
    refuse_if_owner_claimed(state, session_id, owner_id)?;
    let owner = state.session_record(owner_id)?;
    if let Some(reason) = owner_refusal(state, &session, &owner)? {
        anyhow::bail!(reason);
    }
    let lead = owner.role == Role::Console;
    let bound = state
        .store
        .set_binding(session_id, None, owner_id, lead, !lead, false)?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "the binding changed while it was being made — this session was bound, it gained \
                 sessions of its own, or `{owner_id}` can no longer own sessions; look again \
                 before retrying"
            )
        })?;
    state.publish_session(&bound);
    withdraw_pages_if_bound(state, &session, &bound);
    Ok((bound, owner))
}

/// Withdraws the report-panel pages of a session that a binding change has just given the role of
/// a bound one: it has no tool left to list or delete them, and the panel would go on offering
/// their form. The binding is written by now, so a failure is logged and leaves the pages for
/// [`submit_page`] to refuse.
fn withdraw_pages_if_bound(state: &Arc<AppState>, before: &Session, after: &Session) {
    if !before.is_worker() && after.is_worker() {
        if let Err(err) = mcp::withdraw_pages(state, &after.id) {
            tracing::warn!(
                session = %after.id, %err, "withdrawing the pages of a bound session failed"
            );
        }
    }
}

/// Hands `session_id`, bound to `from_id`, to the owner `to_id`, at `from_id`'s request, and tells
/// every client. What the session is bound to changes and nothing else: the sessions bound to it
/// stay bound to it. A lead session handed to another console session stays one; handed to a
/// project session it no longer can own sessions, which only one with none of its own may be (see
/// [`owner_refusal`]).
///
/// Refused, with nothing changed, for the session or either owner under a claim (see
/// [`claim_for_binding`], [`refuse_if_owner_claimed`]), a session that is not bound to `from_id`,
/// handing a session to the owner it already has, and an owner [`owner_refusal`] refuses. Returns
/// the session as it is now bound, its new owner as it was checked, and whether the session has a
/// process to be told in; one that has none is left owed the notice for its next launch, decided
/// while the claim is held and recorded by the binding write itself, as [`release_session`] does.
pub fn transfer_session(
    state: &Arc<AppState>,
    session_id: &str,
    from_id: &str,
    to_id: &str,
) -> Result<(Session, Session, bool)> {
    let _claim = claim_for_binding(state, session_id)?;
    let session = state.session_record(session_id)?;
    if session.bound_to.as_deref() != Some(from_id) {
        anyhow::bail!("`{session_id}` is not bound to `{from_id}`, so it is not its to hand over");
    }
    if to_id == from_id {
        anyhow::bail!("`{session_id}` is already bound to `{to_id}`");
    }
    refuse_if_owner_claimed(state, session_id, from_id)?;
    refuse_if_owner_claimed(state, session_id, to_id)?;
    let owner = state.session_record(to_id)?;
    if let Some(reason) = owner_refusal(state, &session, &owner)? {
        anyhow::bail!(reason);
    }
    let lead = owner.role == Role::Console && session.lead;
    let to_project = owner.role == Role::Project;
    // Decided here, under the claim, and recorded by the binding write itself when there is no
    // process to tell.
    let running = mcp::is_running(state, &session);
    let bound = state
        .store
        .set_binding(session_id, Some(from_id), to_id, lead, to_project, !running)?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "the binding changed while it was being handed over — `{session_id}` was bound \
                 elsewhere, it gained sessions of its own, or `{to_id}` can no longer own \
                 sessions; look again before retrying"
            )
        })?;
    state.publish_session(&bound);
    withdraw_pages_if_bound(state, &session, &bound);
    Ok((bound, owner, running))
}

/// What [`release_session`] did.
#[derive(Debug)]
pub struct Released {
    /// The session as it is now.
    pub session: Session,
    /// Whether it has a process to be told in.
    pub running: bool,
    /// The owner it was released from.
    pub former_owner: String,
}

/// Unbinds `session_id` from its owner, at the owner's request (`owner_id` names it) or the
/// user's (`None`), and tells every client. The session becomes an ordinary unbound project
/// session: `bound_to` and `lead` are cleared, and what it owns stays bound to it, so a lead
/// session keeps its sessions. Its branches stay with it. Any running or dormant state will do.
///
/// Refused, with nothing changed, for the session or its owner under a claim (see
/// [`claim_for_binding`], [`refuse_if_owner_claimed`]), a console session
/// ([`error_code::CONSOLE_SESSION_NEVER_BOUND`]), a session that is not bound
/// ([`error_code::SESSION_NOT_BOUND`]), and, with `owner_id`, one not bound to it
/// ([`error_code::SESSION_NOT_BOUND_TO_OWNER`]); [`error_code::SESSION_BINDING_CHANGED`] when the
/// binding moved between the check and the write. Returns the session as it is now, whether it has
/// a process to be told in, and the owner it was released from, read under the claim; one that has
/// no process is left owed the notice for its next launch, decided while the claim is held and
/// recorded by the binding write itself, with the owner's parting `note` (given only by an owner,
/// already trimmed and not blank). Telling a running one, and the owner, is the caller's.
pub fn release_session(
    state: &Arc<AppState>,
    session_id: &str,
    owner_id: Option<&str>,
    note: Option<&str>,
) -> Result<Released> {
    let _claim = claim_for_binding(state, session_id)?;
    let session = state.session_record(session_id)?;
    if session.role == Role::Console {
        return Err(CodedError::raised(
            error_code::CONSOLE_SESSION_NEVER_BOUND,
            format!(
                "`{session_id}` is a console session, which is never bound, so it has no owner \
                 to be released from"
            ),
            &[("session", session_id)],
        ));
    }
    let Some(current) = session.bound_to.as_deref() else {
        return Err(CodedError::raised(
            error_code::SESSION_NOT_BOUND,
            format!("`{session_id}` is not bound to any owner, so there is nothing to release"),
            &[("session", session_id)],
        ));
    };
    if let Some(owner) = owner_id.filter(|owner| *owner != current) {
        return Err(CodedError::raised(
            error_code::SESSION_NOT_BOUND_TO_OWNER,
            format!("`{session_id}` is not bound to `{owner}`, so it is not its to release"),
            &[("session", session_id), ("owner", owner)],
        ));
    }
    refuse_if_owner_claimed(state, session_id, current)?;
    let running = mcp::is_running(state, &session);
    let released = state
        .store
        .clear_binding(session_id, current, !running, note)?
        .ok_or_else(|| {
            CodedError::raised(
                error_code::SESSION_BINDING_CHANGED,
                format!(
                    "the binding changed while it was being released — `{session_id}` was bound \
                     elsewhere; look again before retrying"
                ),
                &[("session", session_id)],
            )
        })?;
    state.publish_session(&released);
    Ok(Released {
        session: released,
        running,
        former_owner: current.to_string(),
    })
}

/// Who `session` reports to, for the launch that tells it so: `None` for a session that launches
/// with an unbound project session's role description and tools, which a lead session does on
/// every launch, as at its first. An owner that is no longer on record reads as a console session:
/// the text is the generic one, and a report to it fails on its own.
fn owner_of(state: &Arc<AppState>, session: &Session) -> Result<Option<mcp::Owner>> {
    let Some(owner_id) = session.bound_to.as_ref().filter(|_| session.is_worker()) else {
        return Ok(None);
    };
    Ok(Some(match state.store.get_session(owner_id)? {
        Some(owner) if owner.role == Role::Project => mcp::Owner::Project {
            session_id: owner.id,
        },
        _ => mcp::Owner::Console,
    }))
}

/// The agent's id of the conversation a relaunch of this session resumes, or none when it has not
/// got one yet. Only a session that has had a turn has something to resume: without one the agent
/// stored no conversation, and resuming by id fails with "No conversation found" — so a launch
/// without an id opens a fresh conversation instead, in the same project and under the same
/// session. A switch relocates exactly the conversation a resume would ask for.
fn resumable_agent_session_id(session: &Session) -> Option<String> {
    if session.has_conversation {
        session.agent_session_id.clone()
    } else {
        None
    }
}

/// Relaunches a dormant session. `instruction` is given to it here rather than by the caller: as
/// the launch prompt for a Claude Code session that resumes a conversation, otherwise queued to be
/// written once it is running — whichever hook releases it can fire the moment the agent starts, so
/// it has to be in the queue before the launch and out again if the launch never happens.
///
/// A session bound to an archived owner (a console session, or a project session that started it)
/// brings its archived owners back first, level by level from the topmost down — a session bound
/// to an archived lead session brings back the lead session's archived console session, then the
/// lead session — so what it reports reaches a process. The walk stops at the first owner that is
/// not archived; one that is only interrupted is not relaunched. If an owner cannot be relaunched
/// the whole reopen fails with its reason: the owners already brought back stay reopened, and the
/// rest, the session included, stay as they were. Reopening an owner reopens nothing bound to it,
/// and no other session bound to the same owners is reopened: the group comes back one session at
/// a time, from the one the user asked for.
///
/// Before anything is reopened, the session and each owner about to be launched are checked for a
/// conversation the agent can still resume ([`check_resumable`]). The session itself is refused
/// with `conversation_unresumable` when its conversation is proven gone, with nothing changed; an
/// owner whose conversation is proven gone is relaunched into a fresh conversation instead. A
/// Codex rollout that sits in `archived_sessions/` is brought back first ([`unarchive_rollout`]),
/// the session's own before any owner is launched. An owner's leftover records of the conversation
/// it gives up go to the Trash. The login-shell snapshot is taken once and serves the checks, the
/// unarchive and every launch.
pub async fn resume_session(
    state: &Arc<AppState>,
    id: &str,
    instruction: Option<&str>,
    terminal_appearance: Option<TerminalAppearance>,
) -> Result<()> {
    resume_session_with(
        state,
        id,
        instruction,
        terminal_appearance,
        env_shell::launch_snapshot,
    )
    .await
}

/// [`resume_session`] with the login-shell snapshot supplied by the caller. It is taken once and
/// serves the resumability checks, any `codex unarchive` and every relaunch, so all of them look at
/// the same accounts; tests pass a stub so no real login shell runs.
async fn resume_session_with(
    state: &Arc<AppState>,
    id: &str,
    instruction: Option<&str>,
    terminal_appearance: Option<TerminalAppearance>,
    shell_snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
) -> Result<()> {
    resume_session_with_launch(
        state,
        id,
        instruction,
        terminal_appearance,
        LaunchEdit::default(),
        shell_snapshot,
        term::launch,
    )
    .await
}

/// [`resume_session`] that also changes the model and reasoning effort the session is reopened
/// with, as an owner reopening it over MCP asks. They are checked against what the session's agent
/// offers under its own account before anything is launched, recorded as the session's own just
/// before its relaunch, and put back when the relaunch fails. Any archived owner reopened with it
/// keeps its settings.
///
/// The login-shell snapshot and the process start `launch` are supplied by the caller:
/// `env_shell::launch_snapshot` and `term::launch` outside the tests, which cannot start an
/// agent.
pub(crate) async fn resume_session_edited_with(
    state: &Arc<AppState>,
    id: &str,
    instruction: Option<&str>,
    terminal_appearance: Option<TerminalAppearance>,
    launch_edit: LaunchEdit,
    shell_snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl Fn(term::LaunchRequest) -> Result<term::Launch> + Clone + Send + 'static,
) -> Result<()> {
    resume_session_with_launch(
        state,
        id,
        instruction,
        terminal_appearance,
        launch_edit,
        shell_snapshot,
        launch,
    )
    .await
}

/// [`resume_session_with`] with the model and effort to reopen the session with, and the process
/// start `launch`, called once for each session reopened.
async fn resume_session_with_launch(
    state: &Arc<AppState>,
    id: &str,
    instruction: Option<&str>,
    terminal_appearance: Option<TerminalAppearance>,
    launch_edit: LaunchEdit,
    shell_snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl Fn(term::LaunchRequest) -> Result<term::Launch> + Clone + Send + 'static,
) -> Result<()> {
    // Claimed before anything else, so two overlapping relaunches cannot both get through, and
    // before the record is read, so a delete cannot slip in between reading and launching.
    let claim = state.begin_launch(id)?;
    // Taken once: the checks, the unarchive and every launch of this reopen see the one
    // environment, and a default account is resolved from the same shell the agent will run in.
    let shell = tokio::task::spawn_blocking(shell_snapshot).await??;
    let owners = archived_owners(state, id)?;
    // Every session this reopen would launch is checked before the first is, so a target whose
    // record is gone leaves the whole group as it was instead of reopening the owners above it.
    let mut checked_owners = Vec::new();
    for owner in &owners {
        checked_owners.push(check_resumable(state, owner, &shell).await?);
    }
    let target = check_resumable(state, id, &shell).await?;
    if target.judgement.resumability == agent_records::Resumability::Unresumable {
        return Err(conversation_unresumable(&target.session));
    }
    // Refused for a value the agent does not offer before anything is launched or recorded.
    let overrides = checked_launch_overrides(
        state,
        &target.session,
        target.session.account_id.clone(),
        launch_edit,
    )
    .await?;
    // Before any owner is launched, so a failed unarchive leaves the whole group as it was.
    unarchive_rollout(&target, &shell).await?;
    for checked in checked_owners {
        let owner = checked.session.id.clone();
        // The owners were listed before the awaits of the earlier relaunches, so a reopen of a
        // sibling bound session may have brought this one back since: skip it unless it still
        // needs reopening. Two reopens can both read an owner as archived; the launch claim lets
        // one through and refuses the other, and the user's next click goes through.
        if !needs_reopening(state, &owner)? {
            continue;
        }
        let owner_claim = state.begin_launch(&owner)?;
        // The verdict is acted on destructively, so it must still be about the conversation the
        // owner has now: a hook may have recorded a new one since it was judged.
        let current = state.session_record(&owner)?;
        let checked = if current.agent_session_id != checked.session.agent_session_id
            || current.has_conversation != checked.session.has_conversation
        {
            check_resumable(state, &owner, &shell).await?
        } else {
            checked
        };
        if checked.judgement.resumability == agent_records::Resumability::Unresumable {
            forget_gone_conversation(state, &checked.session, &shell).await?;
        } else {
            unarchive_rollout(&checked, &shell).await?;
        }
        relaunch_session_with(
            state,
            &owner_claim,
            &owner,
            None,
            true,
            Some(shell.clone()),
            None,
            launch.clone(),
        )
        .await?;
    }
    if let Some((model, effort)) = &overrides {
        state.set_session_launch_overrides(id, model, effort)?;
    }
    let relaunched = relaunch_session_with(
        state,
        &claim,
        id,
        instruction,
        true,
        Some(shell),
        terminal_appearance,
        launch,
    )
    .await;
    if relaunched.is_err() && overrides.is_some() {
        // A reopen that fails leaves the session as it was, the model and effort included.
        let before = &target.session;
        if let Err(revert) = state.set_session_launch_overrides(id, &before.model, &before.effort) {
            tracing::warn!(
                session = %id,
                %revert,
                "putting the session's model and effort back failed"
            );
        }
    }
    relaunched
}

/// Refuses, before a request to resume `id` is put to the user, what [`resume_session`] would
/// refuse for a conversation its agent no longer has (`conversation_unresumable`), with nothing
/// changed. Returns the archived owners a resume brings back with it, topmost first.
pub(crate) async fn check_can_resume(state: &Arc<AppState>, id: &str) -> Result<Vec<String>> {
    check_can_resume_with(state, id, env_shell::launch_snapshot).await
}

/// [`check_can_resume`] with the login-shell snapshot supplied by the caller, which tests stub.
pub(crate) async fn check_can_resume_with(
    state: &Arc<AppState>,
    id: &str,
    shell_snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
) -> Result<Vec<String>> {
    let shell = tokio::task::spawn_blocking(shell_snapshot).await??;
    let owners = archived_owners(state, id)?;
    let target = check_resumable(state, id, &shell).await?;
    if target.judgement.resumability == agent_records::Resumability::Unresumable {
        return Err(conversation_unresumable(&target.session));
    }
    Ok(owners)
}

/// A session and what [`check_resumable`] found of its conversation.
struct Checked {
    session: Session,
    judgement: agent_records::Judgement,
}

/// Looks for the conversation record of a dormant session in its own account, resolving a default
/// account through `shell`, the snapshot the launch will use. Only a proven absence is
/// [`agent_records::Resumability::Unresumable`]; anything unknown launches as it always has, and a
/// session that is not dormant is not looked at.
async fn check_resumable(
    state: &Arc<AppState>,
    id: &str,
    shell: &HashMap<String, String>,
) -> Result<Checked> {
    let session = state.session_record(id)?;
    if !session.status.is_dormant() {
        return Ok(Checked {
            session,
            judgement: agent_records::Judgement {
                resumability: agent_records::Resumability::Resumable,
                needs_unarchive: false,
            },
        });
    }
    let shell = shell.clone();
    tokio::task::spawn_blocking(move || {
        let judgement = agent_records::judge(&agent_records::Locator::with_shell(shell), &session);
        Checked { session, judgement }
    })
    .await
    .map_err(Into::into)
}

/// Gives up the conversation of `session`, whose record the agent no longer has, so its next
/// launch opens a fresh one: what is still left of it on disk goes to the Trash first, while the
/// session still names it.
async fn forget_gone_conversation(
    state: &Arc<AppState>,
    session: &Session,
    shell: &HashMap<String, String>,
) -> Result<()> {
    trash_agent_records_with(
        state,
        vec![session.clone()],
        agent_records::Locator::with_shell(shell.clone()),
        state.host_tools.get().trash,
    )
    .await;
    state.forget_conversation(&session.id)
}

/// The refusal of a reopen whose session the agent can no longer resume: launching it would only
/// make the agent exit, and a reopen would move an archived session out of the archive for it.
fn conversation_unresumable(session: &Session) -> anyhow::Error {
    CodedError::raised(
        error_code::CONVERSATION_UNRESUMABLE,
        format!(
            "{} no longer has the conversation of this session, so it cannot be resumed",
            session.agent.label()
        ),
        &[("agent", session.agent.label()), ("session", &session.id)],
    )
}

/// Runs `codex unarchive` for a checked Codex session whose rollout only sits in the account's
/// `archived_sessions/`, which a resume refuses. A failure leaves the session untouched: this
/// runs before its status is written.
async fn unarchive_rollout(checked: &Checked, shell: &HashMap<String, String>) -> Result<()> {
    if !checked.judgement.needs_unarchive {
        return Ok(());
    }
    let (session, shell) = (checked.session.clone(), shell.clone());
    tokio::task::spawn_blocking(move || {
        let agent_session_id = session.agent_session_id.as_deref().unwrap_or_default();
        adapter::codex::unarchive(
            &shell,
            session.config_dir.as_deref().map(Path::new),
            agent_session_id,
        )
    })
    .await?
}

/// Whether an owner listed by [`archived_owners`] is still to be relaunched by a reopen: it is
/// archived, or a launch of it is in flight (its claim is held). That launch writes the owner's
/// status before its process is up and puts it back to archived if it fails, so it is left to the
/// launch claim, which refuses this reopen rather than letting it start sessions under an owner
/// that may not stay up. An owner that is neither (brought back since, with its process up, or only
/// interrupted) is skipped, and one that is gone is left to fail on its claim.
fn needs_reopening(state: &Arc<AppState>, owner: &str) -> Result<bool> {
    Ok(match state.store.get_session(owner)? {
        Some(record) => record.status == SessionStatus::Archived || state.is_launching(owner),
        None => true,
    })
}

/// The archived owners a dormant session brings back before itself, topmost first: its owner when
/// that is archived, then that owner's owner when that is archived too, stopping at the first that
/// is not (or is not on record). Empty for a session that is not dormant.
pub(crate) fn archived_owners(state: &Arc<AppState>, id: &str) -> Result<Vec<String>> {
    let session = state.session_record(id)?;
    let mut owners = Vec::new();
    let mut next = session.bound_to.filter(|_| session.status.is_dormant());
    while let Some(owner_id) = next {
        let Some(owner) = state
            .store
            .get_session(&owner_id)?
            .filter(|owner| owner.status == SessionStatus::Archived)
        else {
            break;
        };
        // Bindings are at most two levels deep and never form a cycle; the check only keeps
        // corrupt data from looping here.
        if owners.contains(&owner.id) || owner.id == id {
            break;
        }
        next = owner.bound_to.clone();
        owners.push(owner.id);
    }
    owners.reverse();
    Ok(owners)
}

/// The relaunch a resume and a switch of the session's account both end in: everything Octoboard
/// injects is reassembled and every launch refusal applies, whichever of the two asked. `claim` is
/// the caller's, taken before it read anything and dropped by it when it is done; `reopen` says
/// whether an archived session may be relaunched (a resume reopens one, a switch must not);
/// `shell_env` is a snapshot the caller already resolved something from (see
/// [`term::LaunchRequest::preresolved_shell_env`]). `launch` is `term::launch` outside the tests,
/// which cannot start an agent.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn relaunch_session_with(
    state: &Arc<AppState>,
    claim: &crate::state::LaunchClaim,
    id: &str,
    instruction: Option<&str>,
    reopen: bool,
    shell_env: Option<HashMap<String, String>>,
    terminal_appearance: Option<TerminalAppearance>,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<()> {
    let mut session = state.session_record(id)?;
    if !session.status.is_dormant() {
        // The ordinary cause is a second click landing after the first relaunch already moved the
        // session, so it carries the same code as a launch that is already under way.
        return Err(CodedError::raised(
            error_code::SESSION_ALREADY_RUNNING,
            "this session is not interrupted or archived",
            &[("session", id)],
        ));
    }

    if !reopen && session.status == SessionStatus::Archived {
        return Err(session_archived(id));
    }

    // A change of owner the session was not running to be told of is told now, ahead of the
    // instruction, from the binding as it stands: Claude Code and Grok replay what they were first
    // told of their owner on every resume. Read and built with the refusals, before anything is
    // written, and cleared only for the count seen here, so a change marked since stays owed.
    let owed = state.store.owner_notice_pending(id)?;
    // What was kept for the session while it was interrupted, read with the refusals as well,
    // less the waits for the user that are over by now.
    let held = reporting::current_kept_messages(state, id)?;
    let combined = if owed > 0 {
        let notice = mcp::owner_notice(state, &session)?;
        Some(match instruction {
            Some(instruction) => format!("{notice}\n\n{instruction}"),
            None => notice,
        })
    } else {
        instruction.map(str::to_string)
    };
    let instruction = combined.as_deref();

    let previous_status = session.status;
    let previous_ended_at = session.ended_at;
    let cwd = session_cwd(state, &session)?;
    let resume_id = resumable_agent_session_id(&session);
    // A relaunch carries no task of its own, so the agent comes up at its prompt rather than
    // working (a Claude Code instruction given as the launch prompt is reported by its hook). On
    // Codex that is also the only correct value available: its `SessionStart` hook does not fire
    // until the first prompt submission, so an optimistic `working` would never be corrected.
    session.status = SessionStatus::Idle;
    session.ended_at = None;
    if state
        .store
        .update_session_lifecycle_if(id, previous_status, SessionStatus::Idle, None)?
        .is_none()
    {
        return Err(session_archived(id));
    }

    // Claude Code asks "Resume this conversation?" at a launch that resumes with no prompt on the
    // command line, and a message pasted into that dialog is discarded while its Enter picks a
    // row. With the instruction as the launch prompt the dialog is not shown, so it goes there;
    // every other launch has it queued instead.
    let (prompt, queued) = match instruction {
        Some(instruction) if session.agent == Agent::Claude && resume_id.is_some() => {
            (term::launch_prompt(instruction), None)
        }
        queued => (None, queued),
    };
    // The information is queued in the order it arrived. For a Claude Code resume the instruction
    // goes as the launch prompt, so the information is written after it; for the other agents it
    // is queued behind the information. Each row stays in the store until its message has been
    // written into the session (`AppState::flush_outbox`): a launch that fails, a process that
    // ends or a daemon that exits before then discards the queue and leaves the row for the next
    // resume.
    for (row, text) in &held {
        state.queue_kept_message(&session.id, text, *row);
    }
    // Queued with every refusal above already past, so a call that never launched leaves nothing
    // behind for a later launch to deliver.
    if let Some(instruction) = queued {
        state.queue_message(&session.id, instruction);
    }

    match start_process(
        state,
        claim,
        &session,
        &cwd,
        prompt.as_deref(),
        resume_id.as_deref(),
        terminal_appearance,
        // The snapshot the caller resolved something from is the one the launch must use.
        move |request| {
            launch(term::LaunchRequest {
                preresolved_shell_env: shell_env,
                ..request
            })
        },
    )
    .await
    {
        Ok(()) => {
            // What was queued for the relaunched session — the instruction above, say — is
            // released by its first hook (for Claude Code, once bracketed paste is on as well);
            // for Codex, which runs none until its first prompt, also by Octoboard's successful
            // press of its trust screen, or once its trust watch has ended with none sighted
            // (`trust::supervise`). Never before, since the agent may be sitting on its trust or
            // approval screen, where the paste's trailing Enter would answer it.
            let recorded = match state.session_record(&session.id) {
                Ok(recorded) => recorded,
                Err(err) => {
                    // The caller will not watch the process come up.
                    state.release_launch_watch(&session.id);
                    return Err(err);
                }
            };
            if recorded.status == SessionStatus::Archived {
                // The archive may have landed while the launch was off the runtime, before
                // there was a registered process for it to end.
                if let Some(live) = state.live_session(&session.id) {
                    stop_process(state, live);
                }
                state.revoke_mcp_tokens(&session.id);
                state.discard_outbox(&session.id);
                return Err(session_archived(&session.id));
            }
            // The notice went out with the launch: a failure to clear it only means it is told
            // once more at the next one.
            if owed > 0 {
                if let Err(err) = state.store.clear_owner_notice(&session.id, owed) {
                    tracing::warn!(session = %session.id, %err, "clearing a delivered owner notice failed");
                }
            }
            state.publish_session(&recorded);
            Ok(())
        }
        Err(err) => {
            // Back to where it was, archived included: a reopen that failed to launch must not
            // quietly move a session out of its project's Archive group. The recorded end time is
            // written back rather than left alone, since the relaunch above cleared it and
            // persisted that before launching; the archive is ordered and dated by it (see
            // "The archive view" in `docs/product/archive-view.md`), so a session this attempt did not
            // change would otherwise jump to the top with an incorrect archive date.
            // Restore only this attempt's optimistic status: a newer archive, including its
            // own end time, must survive a launch failure.
            if let Some(restored) = state.store.update_session_lifecycle_if(
                id,
                SessionStatus::Idle,
                previous_status,
                previous_ended_at,
            )? {
                state.publish_session(&restored);
            }
            state.revoke_mcp_tokens(&session.id);
            // Nothing will ever release what was queued for this relaunch, and leaving it would
            // deliver a stale instruction to whatever launch comes next.
            state.discard_outbox(&session.id);
            Err(err)
        }
    }
}

/// The model and reasoning effort `console` launches a console session of `agent`
/// with: that agent's own pair, since each has its own model names.
fn console_session_launch_settings(
    console: &Console,
    agent: Agent,
) -> (Option<String>, Option<String>) {
    match agent {
        Agent::Claude => (
            console.claude_console_session_model.clone(),
            console.claude_console_session_effort.clone(),
        ),
        Agent::Codex => (
            console.codex_console_session_model.clone(),
            console.codex_console_session_effort.clone(),
        ),
        Agent::Grok => (
            console.grok_console_session_model.clone(),
            console.grok_console_session_effort.clone(),
        ),
    }
}

/// The model and reasoning effort a session of `project` launches `agent` with when it chose
/// neither: for each value independently, the project's own, else its console's project session
/// value, else `None`, the agent's own default. Each agent has its own pair, since each has its own
/// model names.
pub(crate) fn project_launch_settings(
    project: &Project,
    console: &Console,
    agent: Agent,
) -> (Option<String>, Option<String>) {
    let (model, effort) = project.launch(agent);
    let (console_model, console_effort) = console.project_launch(agent);
    (
        model.clone().or_else(|| console_model.clone()),
        effort.clone().or_else(|| console_effort.clone()),
    )
}

/// The model and reasoning effort `session` is launched with, for its own agent: a console session
/// reads its console's console session pair, a project session its project's with its console's
/// project session pair behind it (see [`project_launch_settings`]), unless that field has a
/// session override. Inherited values are read at every launch, while an override (including
/// explicitly leaving the choice to the agent) survives resumes and account switches.
pub(crate) fn launch_settings(
    state: &Arc<AppState>,
    session: &Session,
) -> Result<(Option<String>, Option<String>)> {
    let inherited = match &session.project_id {
        Some(project_id) => {
            let project = state
                .store
                .get_project(project_id)?
                .ok_or_else(|| CodedError::unknown_project(project_id))?;
            let console = state
                .store
                .get_console(&session.console_id)?
                .ok_or_else(|| CodedError::unknown_console(&session.console_id))?;
            project_launch_settings(&project, &console, session.agent)
        }
        None => {
            let console = state
                .store
                .get_console(&session.console_id)?
                .ok_or_else(|| CodedError::unknown_console(&session.console_id))?;
            console_session_launch_settings(&console, session.agent)
        }
    };
    Ok((
        session.model.clone().unwrap_or(inherited.0),
        session.effort.clone().unwrap_or(inherited.1),
    ))
}

/// The git workflow settings in force for a project session's project as this launch is assembled,
/// which shape the rule its role text carries; none for a console session, which is given no rule.
fn launch_git_workflow_settings(
    state: &Arc<AppState>,
    session: &Session,
) -> Result<Option<GitWorkflowSettings>> {
    let Some(project_id) = session
        .project_id
        .as_ref()
        .filter(|_| session.role == Role::Project)
    else {
        return Ok(None);
    };
    let project = state
        .store
        .get_project(project_id)?
        .ok_or_else(|| CodedError::unknown_project(project_id))?;
    Ok(Some(crate::git_workflow::effective(
        &state.store,
        &project,
    )?))
}

/// Launches the agent for an already-stored session, through `launch` (`term::launch` outside the
/// tests, which cannot start an agent), and starts watching it.
#[allow(clippy::too_many_arguments)]
async fn start_process(
    state: &Arc<AppState>,
    claim: &crate::state::LaunchClaim,
    session: &Session,
    cwd: &Path,
    task: Option<&str>,
    resume_agent_session_id: Option<&str>,
    terminal_appearance: Option<TerminalAppearance>,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<()> {
    // Refresh the instruction file on every console launch, resume and account switch. A missing
    // directory is left to the launch, whose refusal names it, rather than failing here on the write.
    if session.role == Role::Console && cwd.is_dir() {
        mcp::console_instructions::refresh_console_session_instructions(
            &state.store,
            &session.console_id,
        )?;
    }
    let (model, effort) = launch_settings(state, session)?;
    // A sharing account's links are repaired before its agent starts, and the agent is told
    // where the shared directory is.
    let sharing = crate::shared_config::before_launch(state, session).await?;
    for notice in sharing.notices {
        state.broadcast(notice.about(&session.id));
    }
    // Reserved until the process is registered below, and given up if it never is.
    let pending_launch = state.process_ledger.begin_launch(&session.id);
    let request = term::LaunchRequest {
        session_id: session.id.clone(),
        agent: session.agent,
        role: session.role,
        owner: owner_of(state, session)?,
        git_workflow: launch_git_workflow_settings(state, session)?,
        ask_questions_as_choices: state.store.ask_questions_as_choices()?,
        user_language: state.store.interface_language()?,
        cwd: cwd.to_path_buf(),
        task: task.map(str::to_string),
        resume_agent_session_id: resume_agent_session_id.map(str::to_string),
        config_dir: session.config_dir.as_ref().map(PathBuf::from),
        approval_reviewer: CODEX_REVIEWER.for_session(state, session)?,
        approval_mode: CLAUDE_MODE.for_session(state, session)?,
        claude_compatibility: state.store.claude_compatibility_of(session.agent)?,
        shared_default_dir: sharing.shared_default_dir,
        model,
        effort,
        terminal_appearance,
        preresolved_shell_env: None,
        daemon_port: state.port,
        self_exe: state.self_exe.clone(),
        // Issued per launch: the previous process is gone, and a token outliving it would let a
        // stale child act on this session.
        mcp_token: state.issue_mcp_token(&session.id),
        process_tag: pending_launch.tag().clone(),
    };

    // Launching snapshots the user's shell environment and forks a process, so it goes off the
    // runtime rather than holding up the socket it was asked on.
    let launch = tokio::task::spawn_blocking(move || launch(request)).await??;
    // Octoboard's own account switch, or the restoring of a session after one, watches the
    // process come up and reports the outcome itself.
    if claim.watches_launch() {
        launch.session.watch_under_claim();
    }

    // The live map now says the session is running; `claim` stays with the caller, which drops it
    // when it is done with the launch.
    state.register_live(launch.session.clone());
    // The output of an earlier run is stale from here on: were this process to end without saving
    // (a daemon killed outright), it must not stand in for what this one printed. It goes after
    // `register_live`, so a client attaching meanwhile finds either the process or the saved
    // output, never neither (and a launch that fails leaves it to show). It also goes before
    // `watch_exit`, so the exit save of this process is never the one removed; `stop_all_sessions`
    // saves too, but only after `terminate` has returned, which waits on the process, long past
    // this point.
    state.forget_saved_output(&session.id);
    state.watch_exit(launch.session.clone());
    trust::supervise(state, &launch.session);
    agent_dialogs::watch(state, &launch.session);
    if let Some(agent_session_id) = launch.agent_session_id {
        if let Err(err) = state.set_agent_session_id(&session.id, &agent_session_id) {
            // Registered already, and the caller will not watch it come up.
            state.release_launch_watch(&session.id);
            return Err(err);
        }
    }
    if let Some(notice) = launch.notice {
        state.broadcast(notice.about(&session.id));
    }
    Ok(())
}

/// Ends the session's process and archives it. What archiving means beyond ending the process
/// belongs here and not in [`stop_process`], which a switch of the session's account shares.
///
/// Archiving a session that has sessions bound to it — a console session, or a project session
/// that started some — is a group operation along the binding, at both levels: a console
/// session's bound sessions include its lead sessions, and theirs come along. It is decided by
/// whether a process is running and not by status: refused, with nothing changed, while any
/// session in the group has one (working, awaiting instructions or waiting for the user alike, at
/// either level; the refusal counts and names them), and otherwise archiving every session in the
/// group that is not archived yet, before the owner itself. Nothing bound to it is left outside the
/// archive. The order is deliberate: the deepest level first and the owner last, so a failure part
/// way leaves only a state the user could have produced by hand, never an archived owner with a
/// session bound to it still outside the archive. A session with nothing bound to it — one with
/// the role of a bound session, or an owner that started none — never reaches another session: the
/// owner's tool, a `done` report with no open items, a `delete_project` that stops sessions.
/// The user's control-socket request instead explicitly ends running descendants too.
///
/// Returns the sessions it archived, as they were before, for the removal of the branches bound
/// to them (`branch_origin::cleanup`), which is the caller's: a project's deletion stops its
/// sessions this way and removes none.
pub fn archive_session(state: &Arc<AppState>, id: &str) -> Result<Vec<Session>> {
    archive_session_group(state, id, false)
}

/// A user-confirmed archive may end every process in the group; agent tools retain the
/// unfinished-work safeguard. Validate all locks before changing any session. Returns the sessions
/// archived, as [`archive_session`] does.
fn archive_session_group(
    state: &Arc<AppState>,
    id: &str,
    stop_descendants: bool,
) -> Result<Vec<Session>> {
    let _removal = state.claim_session_removal();
    let session = state.session_record(id)?;
    let descendants = team(state, &session)?;
    ensure_unlocked(std::iter::once(session.clone()).chain(descendants.clone()))?;
    let mut archived = Vec::new();
    if stop_descendants {
        for bound in descendants {
            if bound.status != SessionStatus::Archived {
                archive_record(state, &bound.id)?;
                archived.push(bound);
            }
        }
        archive_record(state, id)?;
        archived.push(session);
        return Ok(archived);
    }
    // "Not running" is taken to mean interrupted by exclusion: a status that is neither archived
    // nor backed by a process would be archived here without anyone having decided so. One gap is
    // left open: `begin_open` inserts the session record, `bound_to` included, before it takes
    // the launch claim, so in between a bound session reads as neither archived nor
    // process-backed. Nothing awaits in that gap, so no archive can land in it, and it is not
    // closed.
    let (running, dormant): (Vec<Session>, Vec<Session>) = descendants
        .into_iter()
        .filter(|bound| bound.status != SessionStatus::Archived)
        .partition(|bound| state.has_process(&bound.id));
    if !running.is_empty() {
        return Err(CodedError::raised(
            error_code::SESSION_HAS_RUNNING_SESSIONS,
            format!(
                "this session cannot be archived while sessions under it have a process \
                 running: {}; archive them or wait for them to finish first",
                session_titles(&running)
            ),
            &[
                ("count", &running.len().to_string()),
                ("sessions", &session_titles(&running)),
                (
                    "session_ids",
                    &serde_json::to_string(
                        &running
                            .iter()
                            .map(|session| &session.id)
                            .collect::<Vec<_>>(),
                    )?,
                ),
            ],
        ));
    }
    for bound in dormant {
        archive_record(state, &bound.id)?;
        archived.push(bound);
    }
    archive_record(state, id)?;
    archived.push(session);
    Ok(archived)
}

/// Removes the branches and worktrees bound to `sessions` once their processes have ended, off
/// the request's path, forcing what `forced` names; every client is told of what is still there
/// and was not kept on purpose (`session_branches_not_removed`, see `cleanup::to_report`). The
/// archive or deletion it follows stands either way.
fn remove_branches_once_stopped(
    state: &Arc<AppState>,
    sessions: Vec<Session>,
    forced: Vec<SessionBranchTarget>,
    keep: Vec<SessionBranchTarget>,
) {
    if sessions.is_empty() {
        return;
    }
    let state = Arc::clone(state);
    tokio::spawn(async move {
        let left =
            branch_origin::cleanup::remove_once_stopped(state.clone(), sessions, forced.clone())
                .await;
        report_not_removed(
            &state,
            &branch_origin::cleanup::to_report(left, &forced, &keep),
        );
    });
}

/// Tells every client of the branches and worktrees an archive, a deletion or a project's removal
/// was to take and that are still there (`session_branches_not_removed`); nothing when there are
/// none.
fn report_not_removed(state: &AppState, missed: &[SessionBranchItem]) {
    if missed.is_empty() {
        return;
    }
    let failure = CodedError::raised(
        error_code::SESSION_BRANCHES_NOT_REMOVED,
        format!(
            "these were to be removed and are still there: {}",
            branch_origin::cleanup::lines(missed).join("; ")
        ),
        &[
            ("count", &missed.len().to_string()),
            ("items", &branch_origin::cleanup::for_client(missed)),
        ],
    );
    state.broadcast(Event::from_error(None, &failure));
}

pub(crate) fn ensure_unlocked(sessions: impl IntoIterator<Item = Session>) -> Result<()> {
    if let Some(session) = sessions.into_iter().find(|session| session.locked) {
        return Err(CodedError::raised(
            error_code::SESSION_LOCKED,
            "this operation includes a locked session; unlock it first",
            &[("session", &session.id)],
        ));
    }
    Ok(())
}

/// The sessions of `sessions` bound to `owner`, in any status. A session with the role of a bound
/// one owns none, so it has none. Walks that build an owner's team or group go through it, so
/// they agree on what is bound to what.
fn bound_in<'a>(sessions: &'a [Session], owner: &Session) -> impl Iterator<Item = &'a Session> {
    let owns = !owner.is_worker();
    let owner_id = owner.id.clone();
    sessions
        .iter()
        .filter(move |session| owns && session.bound_to.as_deref() == Some(owner_id.as_str()))
}

/// `root` and every session below it along the bindings, in any status: the sessions bound to it,
/// each followed by its own. Bindings are two levels deep at most (a console session, its lead
/// sessions, what is bound to those), so the walk goes no further.
pub(crate) fn group_in<'a>(sessions: &'a [Session], root: &'a Session) -> Vec<&'a Session> {
    let mut group = vec![root];
    for bound in bound_in(sessions, root) {
        group.push(bound);
        group.extend(bound_in(sessions, bound));
    }
    group
}

/// How many sessions are bound to each owner of `sessions`, in any status, by the owner's id,
/// counted as [`bound_in`] does: a session with the role of a bound one owns none, whatever rows
/// say they are bound to it.
pub(crate) fn owned_counts(sessions: &[Session]) -> HashMap<String, usize> {
    let owns_none: HashSet<&str> = sessions
        .iter()
        .filter(|session| session.is_worker())
        .map(|session| session.id.as_str())
        .collect();
    let mut counts = HashMap::new();
    for owner in sessions
        .iter()
        .filter_map(|session| session.bound_to.as_deref())
    {
        if !owns_none.contains(owner) {
            *counts.entry(owner.to_string()).or_default() += 1;
        }
    }
    counts
}

/// [`owned_counts`] for the one session `owner`, without reading the table of sessions: its
/// entry, or none when it owns none. Counted as [`bound_in`] does, so a session with the role of a
/// bound one owns none.
pub(crate) fn owned_count_of(
    state: &Arc<AppState>,
    owner: &Session,
) -> Result<HashMap<String, usize>> {
    let mut counts = HashMap::new();
    if !owner.is_worker() {
        let owned = state.store.count_bound_to(&owner.id)?;
        if owned > 0 {
            counts.insert(owner.id.clone(), owned);
        }
    }
    Ok(counts)
}

/// The sessions bound to `owner`, in any status.
pub(crate) fn bound_sessions(state: &Arc<AppState>, owner: &Session) -> Result<Vec<Session>> {
    let sessions = state.store.list_sessions()?;
    Ok(bound_in(&sessions, owner).cloned().collect())
}

/// Every session below `owner` along the bindings, in any status, the deepest level first: the
/// sessions bound to its lead sessions, then the sessions bound to it.
pub(crate) fn team(state: &Arc<AppState>, owner: &Session) -> Result<Vec<Session>> {
    let sessions = state.store.list_sessions()?;
    let mut team = Vec::new();
    for bound in bound_in(&sessions, owner) {
        team.extend(bound_in(&sessions, bound).cloned());
    }
    team.extend(bound_in(&sessions, owner).cloned());
    Ok(team)
}

/// Archives the running sessions of the project `in_project` matches, the way the user's own
/// archive request does.
fn stop_project_sessions(
    state: &Arc<AppState>,
    in_project: impl Fn(&Session) -> bool,
) -> Result<()> {
    // A process still exiting when its rows are gone is tolerated by `watch_exit` and the hook paths.
    // The deepest level goes first: archiving an owner is refused while a session
    // bound to it still has a process, and all of them are in this project. So the
    // sessions bound to a lead session go before it, and it, bound to a console
    // session, before the unbound ones. A console session belongs to no project and
    // is not in this loop; a lead session's binding to one is left as any bound
    // session's is, and goes with its record.
    let mut in_this_project = Vec::new();
    for live in state.live_sessions() {
        if let Some(session) = state.store.get_session(&live.id)? {
            if in_project(&session) {
                in_this_project.push(session);
            }
        }
    }
    let levels = in_this_project
        .iter()
        .map(|session| Ok((session.id.clone(), binding_depth(state, session)?)))
        .collect::<Result<HashMap<String, usize>>>()?;
    in_this_project.sort_by_key(|session| std::cmp::Reverse(levels[&session.id]));
    for session in in_this_project {
        archive_session(state, &session.id)?;
    }
    Ok(())
}

/// How many owners up `session`'s bindings go: 0 for an unbound session, 1 for one bound to an
/// owner that is itself unbound, 2 for one bound to a lead session.
fn binding_depth(state: &Arc<AppState>, session: &Session) -> Result<usize> {
    let Some(owner) = &session.bound_to else {
        return Ok(0);
    };
    let owner_bound = state
        .store
        .get_session(owner)?
        .is_some_and(|owner| owner.bound_to.is_some());
    Ok(if owner_bound { 2 } else { 1 })
}

/// English fallback titles; clients use the structured IDs to word and render the list.
fn session_titles(sessions: &[Session]) -> String {
    sessions
        .iter()
        .map(|session| session.title.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Each session as an id and its title, for a refusal that names sessions to the model.
pub(crate) fn session_labels<'a>(sessions: impl IntoIterator<Item = &'a Session>) -> String {
    sessions
        .into_iter()
        .map(|session| format!("`{}` (\"{}\")", session.id, session.title))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Marks one session archived and ends its process, if it has one.
fn archive_record(state: &Arc<AppState>, id: &str) -> Result<()> {
    ensure_unlocked([state.session_record(id)?])?;
    state.archive_session_record(id)?;
    state.forget_left_for_limit(id);
    if let Some(live) = state.live_session(id) {
        // Ending the process waits out the graceful period, which the caller should not.
        stop_process(state, live);
    }
    // Messages kept for it to take at a resume are not for it once archived: a later reopen must
    // not be handed them.
    if let Err(err) = state.store.clear_kept_messages(id) {
        tracing::warn!(
            session = %id, %err,
            "dropping the messages kept for an archived session failed"
        );
    }
    Ok(())
}

/// Ends one session's process, waiting out the graceful period the agents' shutdown hooks depend
/// on, on a blocking thread. This is the whole of "ending a process" and nothing more: it decides
/// no status, archives nothing and reaches no other session. What archiving adds around it — the
/// status, and whatever later follows an archived session to the sessions bound to it — is
/// `archive_session`'s, which is why a switch of the session's account stops the process through
/// this and not through archiving. Answers whether it found the process running and stopped it
/// (see [`LiveSession::end`]), which the exit watcher reads to know the end was asked for.
///
/// The process ledger reads the table first, so that what the agent started a moment ago is
/// attributed to its launch while its chain through the agent still shows.
pub(crate) fn stop_process(
    state: &Arc<AppState>,
    live: Arc<LiveSession>,
) -> tokio::task::JoinHandle<bool> {
    let ledger = Arc::clone(&state.process_ledger);
    tokio::task::spawn_blocking(move || {
        ledger.scan_before_ending();
        live.end()
    })
}

/// How long a relaunched session has to stay up before a switch counts it as having come up. An
/// agent that cannot find the conversation it was asked to resume says so and exits within a
/// moment, each in its own words, which the switch does not read: its process ending is the signal,
/// as it is everywhere else the daemon decides a session is interrupted.
///
/// One failing relaunch could outlive the window: Grok Build restores a session it cannot find
/// locally from a remote registry, a network round trip. That is harmless here, because the
/// switch's own copy means Grok finds the session locally; and if a relaunch did fail after the
/// window, the session would be interrupted on the new account, where the copy is, and so
/// resumable.
///
/// An account that is not signed in is not caught here, for any of the three agents: each comes up
/// on its own sign-in instead of exiting — Grok Build included, which waits on a browser
/// device-code approval — so a switch to such an account counts as having come up.
const SWITCH_SETTLE: Duration = Duration::from_secs(4);

/// How long a switch waits for the process it ended to be seen gone and recorded as interrupted,
/// which the exit watcher does on its own schedule. Far above that schedule plus the graceful
/// period: reaching it means the process would not die, which is not a state to carry on from
/// (see [`error_code::SESSION_DID_NOT_STOP`]).
pub(crate) const SWITCH_STOP_TIMEOUT: Duration = Duration::from_secs(30);

/// What a switch worked out before it ended anything.
struct SwitchPlan {
    /// The snapshot the default account was resolved from, handed on to the relaunch so the copy
    /// and the launch cannot disagree about where the default account is.
    shell_env: Option<HashMap<String, String>>,
    /// The conversation to copy: from the source account's directory to the target's, the path
    /// relative to both of the record, and the agent session id it belongs to. What the agent
    /// keeps beside it is listed only once the process is down, as it may still be creating
    /// some. None for a session with no conversation on the agent's side.
    relocation: Option<(PathBuf, PathBuf, PathBuf, String)>,
}

/// Everything a switch can refuse before it ends the session's process: the launch rule on the
/// target directory, and the conversation record being where it is expected. The default account
/// pins nothing, so each side of it resolves from one shell snapshot taken here, once, and kept
/// for the relaunch; so does a Codex switch, whose environment says where a home keeps its
/// database. `source` and `target` are the pinned directories, `None` for the default account.
/// Blocking.
fn prepare_switch(
    agent: Agent,
    resume_id: Option<&str>,
    source: Option<&str>,
    target: Option<&str>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>>,
) -> Result<SwitchPlan> {
    // Before anything is copied: the copy would create the directory's `sessions/` and so make a
    // Grok home that was never initialized pass the same check at launch.
    adapter::refuse_unlaunchable_account(agent, target.map(Path::new))?;
    // Codex's environment also says where a home keeps its database, which the copy must find.
    let shell_env = if source.is_none() || target.is_none() || agent == Agent::Codex {
        Some(snapshot().context("snapshotting the user's shell environment")?)
    } else {
        None
    };
    let Some(resume_id) = resume_id else {
        return Ok(SwitchPlan {
            shell_env,
            relocation: None,
        });
    };
    let resolve = |pinned: Option<&str>| account_dir(agent, pinned, shell_env.as_ref());
    let (from, to) = (resolve(source), resolve(target));
    let record = relocate::locate(agent, &from, resume_id)?;
    Ok(SwitchPlan {
        shell_env,
        relocation: Some((from, to, record, resume_id.to_owned())),
    })
}

/// Moves a session to another account of its own agent: ends its process the way archiving does,
/// copies its conversation into the target account's directory, records the account and the
/// directory on the session, and relaunches it as a resume does — with a prompt to carry on where
/// the switch cut a turn off or a usage limit had stopped one (see `crate::switch_continue`). The
/// user or an owner asks for it, and so does Octoboard itself for a session whose turn stopped on
/// its account's usage limit, while `auto_switch_on_usage_limit` is on (see `crate::auto_switch`).
///
/// `account` is the target by id, `None` for the default account. Refused, with nothing recorded
/// and (where it can be told beforehand) the process left running: a launch, resume or switch of
/// the session under way, an archived session, the account it is on already, an account that is
/// not one of the session's agent, a disabled account, an account whose plan quota is used up, an
/// account the last quota reading found not logged in, a Grok target that is not an initialized
/// home, a conversation record that is not where it is expected, and a source or target whose
/// sharing is being changed (`account_sharing_in_progress`). Once the process has been ended, a
/// relocation that does not complete or a relaunch that is refused or does not come up leaves the
/// session interrupted on the account it had, with the conversation still in both directories.
///
/// Two windows break "always back on the account it had", neither harmful. A daemon that dies
/// between recording the new account and the relaunch leaves the session on the new account, which
/// already holds the copy, so the conversation resumes there. And a switch to the *default* Grok
/// account is not checked for being an initialized home (`refuse_unlaunchable_account` is a no-op
/// for an account that pins nothing, by design: that directory is the user's own setup), so an
/// uninitialized `~/.grok` gets the copy, and the relaunch either comes up on Grok's own sign-in,
/// which counts as a switch made like any target that is not signed in, or dies, which is reported
/// as a failed switch and put back.
///
/// It archives nothing: the process is ended by [`stop_process`], not by [`archive_session`], and
/// the status the session passes through is the interrupted one the exit watcher gives any
/// process that ends without the session having been archived.
pub async fn switch_session_account(
    state: &Arc<AppState>,
    id: &str,
    account: Option<Option<String>>,
    terminal_appearance: Option<TerminalAppearance>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
) -> Result<()> {
    switch_session_account_with(
        state,
        id,
        account,
        None,
        terminal_appearance,
        LaunchEdit::default(),
        snapshot,
        term::launch,
    )
    .await
    .map(drop)
}

/// [`switch_session_account`] with the asker: `asked_by` is the asking owner's id, `None` for the
/// user. The session must still be bound to the owner once the switch holds its claim (the owner
/// checked before it), else the switch is refused with nothing changed. `launch_edit` is what the
/// user changed of the session's model and effort, checked with the rest and recorded just before
/// the relaunch that reads it, and put back with the account when the switch fails. `launch` is
/// `term::launch` outside the tests, which cannot start an agent. Answers whether the relaunched
/// session was sent [`switch_continue::CONTINUE_PROMPT`] to carry on with the turn the switch cut
/// off or a usage limit stopped (see `crate::switch_continue`).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn switch_session_account_with(
    state: &Arc<AppState>,
    id: &str,
    account: Option<Option<String>>,
    asked_by: Option<&str>,
    terminal_appearance: Option<TerminalAppearance>,
    launch_edit: LaunchEdit,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<bool> {
    switch_session_account_if(
        state,
        id,
        account,
        asked_by,
        SwitchOnlyIf::default(),
        terminal_appearance,
        launch_edit,
        snapshot,
        launch,
    )
    .await
    .map_err(|failed| failed.error)
}

/// What a switch decided on a record read before its claim still needs to find once it holds the
/// claim. Nothing is required by default.
#[derive(Clone, Copy, Default)]
pub(crate) struct SwitchOnlyIf<'a> {
    /// The failed turn and process whose verified quota recovery authorizes this restart.
    pub recovery: Option<&'a crate::auto_switch::PendingRecovery>,
    /// The account the session must still be on (`Some(None)` the default account).
    pub on_account: Option<Option<&'a str>>,
    /// The session must not be shown as waiting for the user, by whatever route: moving it would
    /// end the prompt it waits at. Relaxed when `usage_limited` is set: the usage-limit dialog of
    /// an agent's screen is the point of that switch and does not count as a wait.
    pub not_waiting: bool,
    /// The session must have a process at its prompt or in a turn (shown `working` or `idle`),
    /// so one the user stopped, or whose process exited since the record was read, is not
    /// brought back.
    pub running: bool,
    /// The session's last turn must have stopped on the plan's usage limit, as its conversation
    /// record says (`switch_continue::stopped_on_usage_limit`), read once more under the claim
    /// before the process is ended. A caller that may need to wait for the record to say so does
    /// it ahead of the claim ([`wait_for_usage_limit`]). Also marks the switch as one made for the
    /// limit: once the process has come up, the account left is noted for the session
    /// (`AppState::note_left_for_limit`), under the claim.
    pub usage_limited: bool,
    /// Octoboard's own switch: the process it launches is watched under the claim, so the exit
    /// watcher does not tell the owner that it ended on its own (`LaunchClaim::watches_launch`).
    pub watch_launch: bool,
    /// Text put ahead of the prompt to carry on, in the same prompt, so that the session is told
    /// of the switch without a turn of its own. Not sent when the switch does not carry on.
    pub note: Option<&'a str>,
}

/// Why [`switch_session_account_if`] failed: the error as every other caller sees it, untouched,
/// and whether the switch had ended the session's process and seen it down by then, which leaves
/// the session interrupted and not as it was. A process that did not stop in time, or one that had
/// exited by itself before the stop, is not "ended" by the switch.
#[derive(Debug)]
pub(crate) struct SwitchFailed {
    pub error: anyhow::Error,
    pub ended_process: bool,
}

impl<E: Into<anyhow::Error>> From<E> for SwitchFailed {
    fn from(error: E) -> Self {
        Self {
            error: error.into(),
            ended_process: false,
        }
    }
}

/// The switch was refused because what it was decided on no longer holds; nothing was changed.
#[derive(Debug)]
pub(crate) struct NoLongerDue(pub &'static str);

impl std::fmt::Display for NoLongerDue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for NoLongerDue {}

/// [`switch_session_account_with`] that also holds the session to `only_if`, checked under the
/// claim against the record read there: a session that no longer meets it is refused with
/// [`NoLongerDue`], nothing changed. A failure says, beside the error, whether this switch ended
/// the session's process and `wait_until_down` saw it down ([`SwitchFailed`]), the caller's cue
/// that the session is left interrupted; a failure before that, a process that did not stop in time
/// (`session_did_not_stop`) included, or after a process that had ended by itself, is not.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn switch_session_account_if(
    state: &Arc<AppState>,
    id: &str,
    account: Option<Option<String>>,
    asked_by: Option<&str>,
    only_if: SwitchOnlyIf<'_>,
    terminal_appearance: Option<TerminalAppearance>,
    launch_edit: LaunchEdit,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<bool, SwitchFailed> {
    let account = account.ok_or_else(|| field_required("account"))?;
    // Held until this function returns, after the come-up verdict and any revert, so nothing else
    // can launch, resume, switch or delete the session while its process is down, while the
    // relaunched one is being watched, or while the account is put back; refused here when one
    // of those is under way.
    if only_if.recovery.is_none() {
        state.quota_recovery.cancel(id);
    }
    let claim = state.begin_account_switch(id, only_if.watch_launch)?;
    if only_if.recovery.is_some_and(|wait| !wait.still_due(state)) {
        return Err(NoLongerDue("the quota recovery no longer belongs to this turn").into());
    }
    let before = state.session_record(id)?;
    refuse_unless_still_askers(&before, asked_by)?;
    refuse_if_archived(&before)?;
    if only_if
        .on_account
        .is_some_and(|expected| expected != before.account_id.as_deref())
    {
        return Err(NoLongerDue("the session is on another account now").into());
    }
    // A turn the usage limit stopped may show the agent's limit dialog, which the switch ends.
    let shown = if only_if.usage_limited {
        state.status_for_switch(&before)
    } else {
        state.shown_status(&before)
    };
    if only_if.not_waiting && shown == SessionStatus::WaitingUser {
        return Err(NoLongerDue("the session waits for the user now").into());
    }
    if only_if.running
        && !(matches!(shown, SessionStatus::Working | SessionStatus::Idle)
            && state.live_session(id).is_some())
    {
        return Err(NoLongerDue("the session has no running process now").into());
    }
    let target_account = checked_account_reference(state, before.agent, account)?;
    let same_account = before.account_id == target_account;
    if !only_if
        .recovery
        .is_some_and(crate::auto_switch::PendingRecovery::permits_current_account)
    {
        refuse_if_on_account(&before, &target_account)?;
    }
    let target_dir = state.store.account_dir(&target_account)?;
    refuse_if_account_disabled(state, before.agent, target_account.as_deref())?;
    refuse_if_account_quota_exhausted(
        state,
        before.agent,
        target_account.as_deref(),
        target_dir.as_deref(),
    )?;
    refuse_if_account_not_signed_in(
        state,
        before.agent,
        target_account.as_deref(),
        target_dir.as_deref(),
    )?;
    // Asked under the claim, which a change of the source's sharing counts as a session starting.
    crate::shared_config::refuse_if_changing(state, before.account_id.as_deref())?;
    crate::shared_config::refuse_if_changing(state, target_account.as_deref())?;
    let overrides =
        checked_launch_overrides(state, &before, target_account.clone(), launch_edit).await?;

    let plan = {
        let (agent, resume_id) = (before.agent, resumable_agent_session_id(&before));
        let (source, target) = (before.config_dir.clone(), target_dir.clone());
        tokio::task::spawn_blocking(move || {
            prepare_switch(
                agent,
                resume_id.as_deref(),
                source.as_deref(),
                target.as_deref(),
                snapshot,
            )
        })
        .await??
    };

    if only_if.usage_limited {
        let stopped = match &plan.relocation {
            Some((from, _, record, _)) => {
                let (agent, record) = (before.agent, from.join(record));
                tokio::task::spawn_blocking(move || {
                    switch_continue::stopped_on_usage_limit(agent, &record)
                })
                .await?
            }
            None => false,
        };
        if !stopped {
            return Err(
                NoLongerDue("the session's last turn did not stop on a usage limit").into(),
            );
        }
    }

    if only_if.recovery.is_some_and(|wait| !wait.still_due(state)) {
        return Err(
            NoLongerDue("the quota recovery was cancelled while preparing the restart").into(),
        );
    }
    if !same_account {
        if let Some(wait) = only_if.recovery {
            if wait.own_account_recovered(state)? {
                return Err(
                    NoLongerDue("the held account recovered while preparing the switch").into(),
                );
            }
        }
    }
    let (held_for_relaunch, ended_a_process) = stop_carrying_processes(state, id).await?;
    wait_until_down(state, id).await?;
    // From here the process is down. A failure that follows leaves the session interrupted when
    // this switch ended it, and is marked so; one whose process had ended before is not this
    // switch's to report.
    let after_stop = async {
        // Archiving it meanwhile is the user's later word on the matter.
        if state.session_record(id)?.status == SessionStatus::Archived {
            return Err(session_archived(id));
        }
        // The process is down, so what the account it ran on holds now is final: a Codex thread may
        // have started a segment since the plan, and that head is the one carried. Only a turn this
        // switch ended, or that was waiting out a usage limit in the process it ended, is carried
        // on: a session whose process had ended before was left there by something else.
        let (relocation, carry_on) = match plan.relocation {
            Some((from, to, record, agent_session_id)) => {
                let agent = before.agent;
                tokio::task::spawn_blocking(move || -> Result<_> {
                    let record = match agent {
                        Agent::Codex => relocate::locate(agent, &from, &agent_session_id)?,
                        _ => record,
                    };
                    let carry_on = ended_a_process
                        && switch_continue::should_continue(agent, &from.join(&record));
                    Ok((Some((from, to, record, agent_session_id)), carry_on))
                })
                .await??
            }
            None => (None, false),
        };

        // Held from before the copy until the session is recorded on the target, and given up
        // before the relaunch, which takes it itself: no change of sharing moves what the copy
        // writes.
        let copying = crate::shared_config::before_switch_copy(
            state,
            before.agent,
            target_account.as_deref(),
        )
        .await?;
        if let Some((from, to, record, agent_session_id)) =
            relocation.filter(|(from, to, _, _)| from != to)
        {
            let agent = before.agent;
            // The environment the relaunch will see, which with the target's `config.toml` says
            // where a Codex home keeps its database.
            let launch_env = plan.shell_env.clone().unwrap_or_default();
            tokio::task::spawn_blocking(move || {
                let companions = relocate::companions(agent, &from, &record, &agent_session_id);
                relocate::copy(&from, &to, &record, &companions)?;
                relocate::repoint_thread_row(agent, &to, &record, &agent_session_id, &launch_env)
            })
            .await??;
        }
        state.set_session_account(id, target_account.as_deref(), target_dir.as_deref())?;
        drop(copying);
        if let Some((model, effort)) = &overrides {
            if let Err(err) = state.set_session_launch_overrides(id, model, effort) {
                put_back_on_account(state, &before);
                return Err(err);
            }
        }

        let prompt_to_carry_on = match only_if.note {
            Some(note) => format!("{note}\n\n{}", switch_continue::CONTINUE_PROMPT),
            None => switch_continue::CONTINUE_PROMPT.to_string(),
        };
        if let Err(err) = relaunch_session_with(
            state,
            &claim,
            id,
            carry_on.then_some(prompt_to_carry_on.as_str()),
            false,
            plan.shell_env,
            terminal_appearance,
            launch,
        )
        .await
        {
            put_back_on_account(state, &before);
            return Err(err);
        }
        if !came_up_under_claim(state, id).await {
            put_back_once_down(state, &before).await;
            return Err(CodedError::raised(
                error_code::SWITCH_DID_NOT_COME_UP,
                "the session ended as soon as it was relaunched, so it stays on the account it had",
                &[("session", id)],
            ));
        }
        if only_if.usage_limited && !same_account {
            // Noted under the claim, so that a turn waiting for it to end never picks this account.
            state.note_left_for_limit(id, before.account_id.as_deref());
        }
        Ok(carry_on)
    };
    let switched = after_stop.await;
    give_up_what_a_restart_held(
        state,
        id,
        held_for_relaunch,
        switched.is_err() && ended_a_process,
    )
    .await;
    switched.map_err(|error| SwitchFailed {
        error,
        ended_process: ended_a_process,
    })
}

/// Ends the process of `id`, a session about to be restarted by a switch, a change of its model or
/// a change of its account's sharing, the way archiving ends it, with what the process started
/// carried over to the relaunch rather than stopped with it: held while the session has no process,
/// and, where the caller watches the relaunch coming up (a switch and a model change), while it is,
/// and given up by [`give_up_what_a_restart_held`].
/// Returns what is held, and whether this stopped a running process: one that had exited by itself
/// already, and is only waiting to be deregistered, is not the restart's doing.
pub(crate) async fn stop_carrying_processes(
    state: &Arc<AppState>,
    id: &str,
) -> Result<(HeldForLaunch, bool)> {
    let held = state.process_ledger.hold_for_launch(id);
    let ended_a_process = match state.live_session(id) {
        Some(live) => {
            state.process_ledger.hand_over(&live.launch_id, id);
            stop_process(state, live).await?
        }
        None => false,
    };
    Ok((held, ended_a_process))
}

/// Gives up what a restart still holds for the relaunch, once the relaunch is over: taken in by
/// it, or, when it `failed` after the restart had ended the session's process, kept (see
/// [`keep_what_a_failed_restart_carried`]). On a blocking task: keeping what is held writes the
/// ledger file.
pub(crate) async fn give_up_what_a_restart_held(
    state: &Arc<AppState>,
    id: &str,
    held: HeldForLaunch,
    failed: bool,
) {
    let (state_for_hold, session_id) = (state.clone(), id.to_string());
    let given_up = tokio::task::spawn_blocking(move || {
        if failed {
            keep_what_a_failed_restart_carried(&state_for_hold, &session_id, held);
        } else {
            drop(held);
        }
    })
    .await;
    if let Err(err) = given_up {
        tracing::warn!(session = %id, %err, "giving up what a restart held for the relaunch did not finish");
    }
}

/// Gives up what a restart (a switch, a model change or a change of an account's sharing) that
/// failed after it had ended the session's process still holds for the relaunch: kept for the
/// user, among the processes of the session's project, as a tree the user keeps is, when the
/// session is left interrupted in a project; stopped, when it is a console session, which keeps
/// nothing, or was archived meanwhile, which stops what it started, or when the daemon is shutting
/// down. A process that had exited by itself is not the restart's doing, so what it left is
/// stopped as for any agent that exits on its own: the caller does not come here. **Blocks.**
fn keep_what_a_failed_restart_carried(state: &Arc<AppState>, id: &str, held: HeldForLaunch) {
    match state.session_record(id) {
        Ok(session) if session.status != SessionStatus::Archived => {
            if let Some(project) = session.project_id {
                held.keep_what_is_held(project);
            }
        }
        Ok(_) => {}
        Err(err) => {
            tracing::warn!(session = %id, %err, "reading a session whose restart failed; what it carried is stopped");
        }
    }
}

/// How long a switch for a usage limit waits for the limit to show in the conversation record. An
/// agent may report a failed turn (its `StopFailure` hook) before it has written the entry that
/// says why, so a record that does not say so yet is read again for this long before the turn is
/// judged not to have stopped on the limit. The real ordering is not measured.
#[cfg(not(test))]
const USAGE_LIMIT_RECORD_WAIT: Duration = Duration::from_secs(4);
#[cfg(test)]
const USAGE_LIMIT_RECORD_WAIT: Duration = Duration::from_millis(1500);

/// Whether finding `session`'s conversation record needs the shell snapshot: the default account
/// has no directory of its own, which is resolved from the environment.
pub(crate) fn record_needs_shell_env(session: &Session) -> bool {
    session.config_dir.is_none()
}

/// Whether the conversation record of `session`, found the way [`prepare_switch`] finds it, says
/// the last turn stopped on the usage limit, read again for up to [`USAGE_LIMIT_RECORD_WAIT`] until
/// it does. Meant to run ahead of the switch's claim, so that the wait holds nothing: `shell_env`
/// is the snapshot when [`record_needs_shell_env`]. Refused as `prepare_switch` is when the record
/// is not where it is expected; a session with no conversation has not stopped on a limit.
pub(crate) async fn wait_for_usage_limit(
    session: &Session,
    shell_env: Option<HashMap<String, String>>,
) -> Result<bool> {
    let Some(resume_id) = resumable_agent_session_id(session) else {
        return Ok(false);
    };
    let (agent, source) = (session.agent, session.config_dir.clone());
    let record = tokio::task::spawn_blocking(move || -> Result<PathBuf> {
        if source.is_none() && shell_env.is_none() {
            anyhow::bail!("the default account's directory needs the shell environment");
        }
        let from = account_dir(agent, source.as_deref(), shell_env.as_ref());
        Ok(from.join(relocate::locate(agent, &from, &resume_id)?))
    })
    .await??;
    let deadline = Instant::now() + USAGE_LIMIT_RECORD_WAIT;
    loop {
        let path = record.clone();
        let agent = session.agent;
        let stopped = tokio::task::spawn_blocking(move || {
            switch_continue::stopped_on_usage_limit(agent, &path)
        })
        .await?;
        if stopped || Instant::now() >= deadline {
            return Ok(stopped);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Brings a session a failed account switch left interrupted back up on the account it had
/// (`before` is its record from before the switch), as a resume does and without a prompt: it was
/// at its prompt before the switch, and its turn is not carried on, since the account's usage
/// limit would stop it again. Octoboard's own switch makes this, so that its failure leaves the
/// session as it was (see `crate::auto_switch`). Refused, with nothing launched, when the session
/// is running or being started by then (somebody brought it back meanwhile) or archived; fails
/// when the process cannot be started or does not stay up. `target` is the account the failed
/// switch was to move it to (`None` the default account): a session found on neither that nor
/// `before`'s account was moved by somebody else meanwhile, and is left as they left it.
pub(crate) async fn restore_after_failed_switch(
    state: &Arc<AppState>,
    before: &Session,
    target: Option<&str>,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<()> {
    let id = before.id.as_str();
    // Under an account switch's claim, so that the process it starts is watched as the switch's
    // was. Another claim on the session (a handover of branches, a binding change) is waited for
    // rather than given up on: the session is down and nobody else will bring it back.
    let deadline = Instant::now() + RESTORE_CLAIM_WAIT;
    let claim = loop {
        match state.begin_account_switch(id, true) {
            Ok(claim) => break claim,
            Err(err) if is_already_starting(&err) && Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(err) => return Err(err),
        }
    };
    let now = state.session_record(id)?;
    if now.status == SessionStatus::Archived {
        return Err(session_archived(id));
    }
    if !now.status.is_dormant() {
        return Err(CodedError::raised(
            error_code::SESSION_ALREADY_RUNNING,
            "this session is not interrupted or archived",
            &[("session", id)],
        ));
    }
    if now.account_id != before.account_id {
        if now.account_id.as_deref() != target {
            return Ok(());
        }
        // A revert that failed on the way is tried again.
        put_back_on_account(state, before);
    }
    relaunch_session_with(state, &claim, id, None, false, None, None, launch).await?;
    if !came_up_under_claim(state, id).await {
        return Err(CodedError::raised(
            error_code::SWITCH_DID_NOT_COME_UP,
            "the session ended as soon as it was relaunched on the account it had",
            &[("session", id)],
        ));
    }
    Ok(())
}

/// How long a restore waits for another claim on the session to end before it gives up.
const RESTORE_CLAIM_WAIT: Duration = Duration::from_secs(15);

/// Whether `err` says another launch or claim holds the session.
fn is_already_starting(err: &anyhow::Error) -> bool {
    err.downcast_ref::<CodedError>()
        .is_some_and(|coded| coded.code == error_code::SESSION_ALREADY_STARTING)
}

/// Whether the process an account switch (or a restore after one) relaunched is still running after
/// [`SWITCH_SETTLE`]. One that is has been seen to come up, so its end from then on is the agent's
/// own and is told to the owner as any is.
async fn came_up_under_claim(state: &Arc<AppState>, id: &str) -> bool {
    let Some(live) = state.live_session(id) else {
        return false;
    };
    let up = came_up(&live, SWITCH_SETTLE).await;
    if up {
        live.release_watch();
    }
    up
}

/// What [`set_session_model_with`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModelChange {
    /// The session already had that model and effort as its own: nothing was written or restarted.
    Unchanged,
    /// The session had no process: the model and effort were recorded for its next resume.
    Recorded,
    /// The session's process was ended and relaunched with them, resuming its conversation.
    /// `continued` says whether it was sent [`switch_continue::CONTINUE_PROMPT`] to carry on with
    /// a turn the restart cut off or a usage limit had stopped.
    Restarted { continued: bool },
}

/// Changes the model and reasoning effort of a session that is not archived, as the user asks from
/// the window: see [`set_session_model_with`]. `model` and `effort` are as `open_session` takes
/// them, each absent to keep what the session has.
pub async fn set_session_model(
    state: &Arc<AppState>,
    id: &str,
    model: Option<Option<String>>,
    effort: Option<Option<String>>,
    terminal_appearance: Option<TerminalAppearance>,
) -> Result<ModelChange> {
    set_session_model_with(
        state,
        id,
        LaunchEdit { model, effort },
        ModelAsker::User,
        terminal_appearance,
        env_shell::launch_snapshot,
        term::launch,
    )
    .await
}

/// Who asks [`set_session_model_with`] for a change.
#[derive(Clone, Copy)]
pub(crate) enum ModelAsker<'a> {
    /// The user, from the window, who owns nothing to check and can see what the session waits on.
    User,
    /// The owner whose id this is, over MCP: the session must still be bound to it once the change
    /// holds its claim, and a session waiting for the user is refused
    /// (`session_waiting_for_user`), since the restart would end the prompt the user alone answers
    /// — except where its only wait is a usage-limit dialog, which the restart is free to end, as
    /// an automatic account switch does (`AppState::status_for_switch`).
    Owner(&'a str),
}

/// Changes the model and reasoning effort `edit` gives a session, which become its own choices,
/// checked as opening a session checks them against what its agent offers under its own account
/// (only the values that change).
/// One mechanism serves every agent, the account switch's without the move: a session with a
/// running process has the process ended the way archiving ends it (archiving nothing), the new
/// values recorded, and is relaunched on the same account resuming its conversation, as a resume
/// does. What the process started (a dev server, say) is carried over to the relaunch as for a
/// switch, and kept among the processes of the session's project when the restart fails after it
/// ended the process (see [`keep_what_a_failed_restart_carried`]). A turn the restart cut off, or
/// one a usage limit had stopped, is sent [`switch_continue::CONTINUE_PROMPT`] as the relaunch's
/// instruction, as after a switch; otherwise the session comes back awaiting instructions. A
/// session waiting on a prompt of its agent's has that prompt ended with the process, unless an
/// owner asks (see [`ModelAsker::Owner`]).
///
/// Every client is shown a running session `starting` from the moment the change holds its claim,
/// before anything is checked that takes time, until it is over (`AppState::mark_model_change`),
/// and the model and effort the agent's records showed are forgotten once the process is down,
/// for what it records after the relaunch to replace them.
///
/// Values the session already has are answered [`ModelChange::Unchanged`] with nothing done,
/// before anything else can refuse them, except while another change of its model is under way,
/// which claims the session (`session_already_starting`). An interrupted session only has the
/// values recorded, for its next resume. Refused, with nothing changed: no value given
/// (`field_required`), an archived session (`session_archived`; reopening one with another model is
/// `resume_session`'s), one being launched, resumed, switched, handed off or otherwise claimed
/// (`session_already_starting`) or shown starting, a value the agent does not offer, and a change
/// of its account's sharing under way. Refused after the process was asked to end, with nothing
/// recorded: `model_change_did_not_stop`. Once the process is down, a relaunch the launch rules
/// refuse, or one whose process ends at once (`model_change_did_not_come_up`), leaves the session
/// interrupted with the model and effort it had.
///
/// The shell snapshot the default account is resolved from comes from `snapshot`, and the process
/// is started by `launch`: `env_shell::launch_snapshot` and `term::launch` outside the tests.
pub(crate) async fn set_session_model_with(
    state: &Arc<AppState>,
    id: &str,
    edit: LaunchEdit,
    asker: ModelAsker<'_>,
    terminal_appearance: Option<TerminalAppearance>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<ModelChange> {
    if edit.model.is_none() && edit.effort.is_none() {
        return Err(field_required("model"));
    }
    let (model, effort) = (
        normalize_session_launch_setting(edit.model),
        normalize_session_launch_setting(edit.effort),
    );
    // The values the session would have, or `None` when it has them already: answered before the
    // claim, which another holder may have, since there is nothing to change, except while a
    // change of its model is under way, whose values it may yet put back.
    let changed = |session: &Session| -> Result<Option<_>> {
        if let ModelAsker::Owner(owner) = asker {
            refuse_unless_still_askers(session, Some(owner))?;
        }
        refuse_if_archived(session)?;
        let model = model.clone().or_else(|| session.model.clone());
        let effort = effort.clone().or_else(|| session.effort.clone());
        Ok(((&model, &effort) != (&session.model, &session.effort)).then_some((model, effort)))
    };
    if changed(&state.session_record(id)?)?.is_none() && !state.is_changing_model(id) {
        return Ok(ModelChange::Unchanged);
    }
    // Held until this function returns, as a switch holds its own: nothing else launches,
    // resumes, switches or deletes the session while its process is down or being watched.
    let claim = state.begin_switch(id)?;
    let before = state.session_record(id)?;
    let Some((model, effort)) = changed(&before)? else {
        return Ok(ModelChange::Unchanged);
    };
    if state.status_for_model_change(&before) == SessionStatus::Starting {
        return Err(CodedError::raised(
            error_code::SESSION_ALREADY_STARTING,
            "this session is already being started",
            &[("session", id)],
        ));
    }
    refuse_if_owner_must_wait(state, asker, &before)?;
    // Shown restarting at once, as an account switch is shown switching once it has its claim, so
    // that no window shows the old status or offers another change while the checks run.
    if !before.status.is_dormant() {
        state.mark_model_change(id);
    }
    crate::shared_config::refuse_if_changing(state, before.account_id.as_deref())?;
    let (project, console) = session_scope(state, &before)?;
    check_launch_settings(
        state,
        before.agent,
        before.account_id.clone(),
        project.as_ref(),
        &console,
        model.clone(),
        effort.clone(),
        SettingsChecked::Changed {
            model: &before.model,
            effort: &before.effort,
        },
    )
    .await?;
    let current = state.session_record(id)?;
    if current.status == SessionStatus::Archived {
        return Err(session_archived(id));
    }
    if current.status.is_dormant() {
        if !state
            .store
            .set_session_launch_overrides_unless_archived(id, &model, &effort, None)?
        {
            return Err(session_archived(id));
        }
        state.publish_session_by_id(id);
        return Ok(ModelChange::Recorded);
    }

    // The default account has no directory of its own: the snapshot resolves it for the record
    // read below and for the relaunch, which must agree.
    let shell_env = if record_needs_shell_env(&current) {
        Some(tokio::task::spawn_blocking(snapshot).await??)
    } else {
        None
    };
    let current = state.session_record(id)?;
    if current.status == SessionStatus::Archived {
        return Err(session_archived(id));
    }
    // A prompt may have come up while the snapshot was taken.
    refuse_if_owner_must_wait(state, asker, &current)?;
    // Committed to the restart from here: a quota recovery waiting to resume the session would
    // only be cut off by it.
    state.quota_recovery.cancel(id);
    state.mark_model_change(id);
    let (held_for_relaunch, ended_a_process) = stop_carrying_processes(state, id).await?;
    if let Err(err) = wait_until_down_within(state, id, state.model_change_stop_timeout()).await {
        let stopped = err
            .downcast_ref::<CodedError>()
            .is_some_and(|coded| coded.code == error_code::SESSION_DID_NOT_STOP);
        return Err(if stopped {
            CodedError::raised(
                error_code::MODEL_CHANGE_DID_NOT_STOP,
                "the session's process did not end in time, so its model was not changed",
                &[("session", id)],
            )
        } else {
            err
        });
    }
    // From here the process is down; a failure that follows is the restart's, whatever it was.
    let after_stop = async {
        // Archiving it meanwhile is the user's later word on the matter. Hooks may also have
        // learned the conversation's identity while validation or the shell snapshot was pending.
        let stopped = state.session_record(id)?;
        if stopped.status == SessionStatus::Archived {
            return Err(session_archived(id));
        }
        let reading_state = state.clone();
        let shell = shell_env.clone();
        let (carry_on, boundary) = tokio::task::spawn_blocking(move || {
            let boundary =
                crate::record_collector::observed_boundary(&reading_state, &stopped, shell.clone());
            let carry_on = ended_a_process
                && resumable_agent_session_id(&stopped).is_some_and(|agent_session_id| {
                    let dir =
                        account_dir(stopped.agent, stopped.config_dir.as_deref(), shell.as_ref());
                    relocate::locate(stopped.agent, &dir, &agent_session_id).is_ok_and(|record| {
                        switch_continue::should_continue(stopped.agent, &dir.join(record))
                    })
                });
            (carry_on, boundary)
        })
        .await?;

        let boundary = serde_json::to_string(&boundary)?;
        if !state.store.set_session_launch_overrides_unless_archived(
            id,
            &model,
            &effort,
            Some(&boundary),
        )? {
            return Err(session_archived(id));
        }
        state.publish_session_by_id(id);
        let put_back = || {
            if let Err(revert) =
                state.set_session_launch_overrides(id, &before.model, &before.effort)
            {
                tracing::warn!(
                    session = %id,
                    %revert,
                    "putting the session's model and effort back failed"
                );
            }
        };
        if let Err(err) = relaunch_session_with(
            state,
            &claim,
            id,
            carry_on.then_some(switch_continue::CONTINUE_PROMPT),
            false,
            shell_env,
            terminal_appearance,
            launch,
        )
        .await
        {
            put_back();
            return Err(err);
        }
        if !came_up_under_claim(state, id).await {
            // Once the exit watcher has recorded it interrupted, so the record put back is the one
            // every client is left with.
            if let Err(err) = wait_until_down(state, id).await {
                tracing::warn!(session = %id, %err, "the relaunched process was not recorded as ended");
            }
            put_back();
            return Err(CodedError::raised(
                error_code::MODEL_CHANGE_DID_NOT_COME_UP,
                "the session ended as soon as it was relaunched with the new model, so it keeps the \
                 model and effort it had",
                &[("session", id)],
            ));
        }
        Ok(carry_on)
    };
    let restarted = after_stop.await;
    give_up_what_a_restart_held(
        state,
        id,
        held_for_relaunch,
        restarted.is_err() && ended_a_process,
    )
    .await;
    let carry_on = restarted?;
    Ok(ModelChange::Restarted {
        continued: carry_on,
    })
}

/// Refuses a change of the model of `session` an owner asked for while the session waits for the
/// user, beyond a usage-limit dialog, which the restart is free to end (see [`ModelAsker::Owner`]).
fn refuse_if_owner_must_wait(
    state: &AppState,
    asker: ModelAsker<'_>,
    session: &Session,
) -> Result<()> {
    if matches!(asker, ModelAsker::Owner(_))
        && state.status_for_model_change(session) == SessionStatus::WaitingUser
    {
        return Err(CodedError::raised(
            error_code::SESSION_WAITING_FOR_USER,
            "this session is waiting for the user, who alone can answer what it is showing; \
             restarting it would end that prompt",
            &[("session", &session.id)],
        ));
    }
    Ok(())
}

/// Refuses, in prose, an operation an owner asked for on a session that is no longer its own. The
/// asker checked before the operation took its claim; this repeats it under the claim, with
/// `session` read there. `None` is the user, who owns nothing to check.
pub(crate) fn refuse_unless_still_askers(session: &Session, asked_by: Option<&str>) -> Result<()> {
    match asked_by {
        Some(asker) if session.bound_to.as_deref() != Some(asker) => anyhow::bail!(
            "`{}` (\"{}\") is no longer yours: it was handed to another owner meanwhile",
            session.id,
            crate::sharing::one_line(&session.title)
        ),
        _ => Ok(()),
    }
}

/// Records the account, directory, model and effort a switched session had before, `before`'s. A
/// revert that fails is logged, never allowed to hide the reason the switch failed.
fn put_back_on_account(state: &Arc<AppState>, before: &Session) {
    // The model and effort a switch recorded for its relaunch go back with the account.
    if let Ok(now) = state.session_record(&before.id) {
        if (&now.model, &now.effort) != (&before.model, &before.effort) {
            if let Err(revert) =
                state.set_session_launch_overrides(&before.id, &before.model, &before.effort)
            {
                tracing::warn!(
                    session = %before.id,
                    %revert,
                    "putting the session's model and effort back failed"
                );
            }
        }
    }
    if let Err(revert) = state.set_session_account(
        &before.id,
        before.account_id.as_deref(),
        before.config_dir.as_deref(),
    ) {
        tracing::warn!(session = %before.id, %revert, "putting the session back on its account failed");
    }
}

/// [`put_back_on_account`] for a relaunched process that has already ended, once the exit watcher
/// has recorded the session interrupted. The watcher does that on its own schedule, so reverting
/// first would let every client see the session idle on its old account with no process until the
/// watcher caught up — after the reply saying the switch failed — and a watcher that read the
/// record just before the revert would publish it with the new account. Waiting first means the
/// restored record is broadcast before the caller replies.
async fn put_back_once_down(state: &Arc<AppState>, before: &Session) {
    if let Err(err) = wait_until_down(state, &before.id).await {
        tracing::warn!(session = %before.id, %err, "the relaunched process was not recorded as ended");
    }
    put_back_on_account(state, before);
}

/// Refuses a switch of `session` to the account it is on already (`None` for the default account).
pub(crate) fn refuse_if_on_account(session: &Session, account: &Option<String>) -> Result<()> {
    if *account == session.account_id {
        return Err(CodedError::raised(
            error_code::SESSION_ALREADY_ON_ACCOUNT,
            "this session is on that account already",
            &[("session", &session.id)],
        ));
    }
    Ok(())
}

/// Refuses an archived session, which a switch is refused for, before the user is asked to approve
/// one.
pub(crate) fn refuse_if_archived(session: &Session) -> Result<()> {
    if session.status == SessionStatus::Archived {
        return Err(session_archived(&session.id));
    }
    Ok(())
}

pub(crate) fn session_archived(id: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::SESSION_ARCHIVED,
        "an archived session cannot be locked, switched or handed off; reopen it first",
        &[("session", id)],
    )
}

/// Waits for the exit watcher to have seen the session's process gone and recorded the session as
/// dormant — the state a relaunch starts from, and the one thing that must be true before the
/// watcher can no longer write over what the relaunch records.
pub(crate) async fn wait_until_down(state: &Arc<AppState>, id: &str) -> Result<()> {
    wait_until_down_within(state, id, SWITCH_STOP_TIMEOUT).await
}

/// [`wait_until_down`], giving up after `timeout`.
pub(crate) async fn wait_until_down_within(
    state: &Arc<AppState>,
    id: &str,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while state.live_session(id).is_some() || !state.session_record(id)?.status.is_dormant() {
        if Instant::now() >= deadline {
            // The process may still be running, with nothing recorded and nothing copied.
            return Err(CodedError::raised(
                error_code::SESSION_DID_NOT_STOP,
                "the session's process did not end in time, so it was not switched",
                &[("session", id)],
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(())
}

/// Whether the relaunched process is still running after `settle`.
async fn came_up(live: &LiveSession, settle: Duration) -> bool {
    let deadline = Instant::now() + settle;
    while Instant::now() < deadline {
        if live.poll_exit() {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    !live.poll_exit()
}

/// The ids of the stored sessions that match, for a deletion that drops them by cascade.
fn session_ids_where(
    state: &Arc<AppState>,
    matches: impl Fn(&Session) -> bool,
) -> Result<Vec<String>> {
    Ok(state
        .store
        .list_sessions()?
        .into_iter()
        .filter(|session| matches(session))
        .map(|session| session.id)
        .collect())
}

/// Removes Octoboard's record of one session: an archived one, and with an archived owner (a
/// console session, or a project session that started some) the archived sessions bound to it, at
/// both levels: left behind, reopening one would have no owner to come back with. Deleting an
/// archived bound session on its own leaves its owner alone.
///
/// A session that is not archived is deleted only while it is unused: no turn yet, with none
/// bound to it, and bound to no owner or opened by the user. That is decided in the same step as
/// the delete (see `Store::delete_session_if_unused`), so a turn racing the request wins and the
/// request is refused. Its process is ended after the record is gone, and not before, so a
/// refused delete never costs the session its process.
///
/// The record takes its saved terminal output with it. Nothing else of Octoboard's hangs off a
/// session: its queue, MCP token and turn bookkeeping go when its process does, and so does its
/// scratch directory (clearing the run directory at startup is only the fallback). The project is
/// never touched. The caller moves unused sessions' native records to the Trash automatically,
/// and used sessions' records when asked, through `trash_agent_records_of` (every one
/// deleted, the archived sessions bound to it included).
fn delete_session(state: &Arc<AppState>, id: &str) -> Result<Vec<Session>> {
    let _removal = state.claim_session_removal();
    let session = state.session_record(id)?;
    ensure_unlocked(std::iter::once(session.clone()).chain(team(state, &session)?))?;
    let mut gone = Vec::new();
    let deleted = if session.status == SessionStatus::Archived {
        delete_archived_group(state, &session, &mut gone)?
    } else {
        match state.delete_if_unused(id)? {
            Deletion::Deleted { process, session } => {
                if let Some(live) = process {
                    stop_process(state, live);
                }
                gone.push(*session);
                true
            }
            Deletion::Refused => false,
        }
    };
    if !deleted {
        return Err(CodedError::raised(
            error_code::SESSION_NOT_DELETABLE,
            "only an archived session, or one that has never been used, can be deleted, and not \
             while it is starting",
            &[("session", id)],
        ));
    }
    Ok(gone)
}

fn inspect_session_records(
    state: &AppState,
    ids: &[String],
    mut locator: agent_records::Locator,
) -> Result<crate::protocol::SessionRecordPresence> {
    use crate::protocol::SessionRecordPresence::{Absent, Present, Unknown};
    let accounts = state.store.list_accounts()?;
    let mut presence = Absent;
    for id in ids {
        let Some(session) = state.store.get_session(id)? else {
            presence = Unknown;
            continue;
        };
        match agent_records::presence(&state.record_hints, &mut locator, &accounts, &session) {
            Present => return Ok(Present),
            Unknown => presence = Unknown,
            Absent => {}
        }
    }
    Ok(presence)
}

fn delete_unused_session(state: &Arc<AppState>, id: &str) -> Result<Vec<Session>> {
    let _removal = state.claim_session_removal();
    ensure_unlocked([state.session_record(id)?])?;
    if let Deletion::Deleted { process, session } = state.delete_if_unused(id)? {
        if let Some(live) = process {
            stop_process(state, live);
        }
        return Ok(vec![*session]);
    }
    Err(CodedError::raised(
        error_code::SESSION_NOT_DELETABLE,
        "the session is no longer unused",
        &[("session", id)],
    ))
}

/// Moves the agent's own records of the sessions just deleted to the Trash
/// ([`trash_agent_records_with`], through the Trash proper and the default accounts).
async fn trash_agent_records_after_delete(state: &Arc<AppState>, deleted: Vec<Session>) {
    trash_agent_records_with(
        state,
        deleted,
        agent_records::Locator::default(),
        hostfs::move_to_trash,
    )
    .await;
}

/// [`trash_agent_records_of`] on a blocking task: a move across volumes copies, and the accounts'
/// records are listed. The delete has happened already, so nothing here fails the request: a
/// failure is logged.
pub(crate) async fn trash_agent_records_with(
    state: &Arc<AppState>,
    deleted: Vec<Session>,
    locator: agent_records::Locator,
    trash: impl Fn(&Path) -> Result<()> + Send + 'static,
) {
    if deleted.is_empty() {
        return;
    }
    let state = Arc::clone(state);
    let moved = tokio::task::spawn_blocking(move || {
        trash_agent_records_of(&state, &deleted, locator, trash)
    })
    .await;
    if let Err(err) = moved {
        tracing::warn!(%err, "moving the agent's session records to the Trash did not finish");
    }
}

/// Moves the agent's own records of each of `deleted` that deleting it may remove (see
/// `agent_records::removable_record_paths`) through `trash`. The sessions are gone from the store,
/// or are about to give up their conversation and still on record: either way they no longer count
/// as holding the conversation with another session. A session is left alone when another
/// remaining session refers to the same agent session id (a resumed copy, say), and a path that is
/// missing is skipped. The records' loss costs the session its resumption in the agent only, so a
/// failure to find or move one is logged, never raised. The accounts' records are listed once for
/// the whole call, through `locator`.
///
/// A process that was stopped by the deletion may still rewrite its transcript after the move,
/// leaving a small file behind in the agent's directory; that is accepted.
fn trash_agent_records_of(
    state: &AppState,
    deleted: &[Session],
    mut locator: agent_records::Locator,
    trash: impl Fn(&Path) -> Result<()>,
) {
    let (remaining, accounts) = match (state.store.list_sessions(), state.store.list_accounts()) {
        (Ok(sessions), Ok(accounts)) => (sessions, accounts),
        (Err(err), _) | (_, Err(err)) => {
            tracing::warn!(%err, "reading the sessions before trashing the agent's records failed");
            return;
        }
    };
    let mut paths: Vec<PathBuf> = Vec::new();
    for session in deleted {
        let Some(agent_id) = &session.agent_session_id else {
            continue;
        };
        let shared = remaining.iter().any(|other| {
            other.agent == session.agent
                && other.agent_session_id.as_ref() == Some(agent_id)
                && !deleted.iter().any(|gone| gone.id == other.id)
        });
        if shared {
            continue;
        }
        for path in agent_records::removable_record_paths(
            &state.record_hints,
            &mut locator,
            &accounts,
            session,
        ) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    for path in paths {
        if path.symlink_metadata().is_err() {
            continue;
        }
        if let Err(err) = trash(&path) {
            tracing::warn!(path = %path.display(), %err, "moving the agent's session record to the Trash failed");
        }
    }
}

/// Deletes `session` if it is archived and says whether it did; the archived sessions bound to it
/// go along, each with its own archived bound sessions, so an archived console session takes its
/// archived lead sessions' archived sessions too. Deleting the owner first means a refusal, which
/// only that first step can give, deletes nothing. It does not make the whole atomic: if a later
/// step is skipped (a session being launched right now) or fails, the owner is already gone. That
/// is left, because a bound session can only be mid-launch through a resume, which reopens the
/// owner first, so the owner was no longer archived and the delete was refused. One that stopped
/// being archived meanwhile is skipped. Each session deleted is added to `deleted`, also when a
/// later step fails. The automatic cleanup meets none of the skips: it checks under the removal
/// claim that no member is launching or has a process.
pub(crate) fn delete_archived_group(
    state: &Arc<AppState>,
    session: &Session,
    deleted: &mut Vec<Session>,
) -> Result<bool> {
    if !state.delete_if_archived(&session.id)? {
        return Ok(false);
    }
    deleted.push(session.clone());
    for bound in bound_sessions(state, session)? {
        // One that is not archived keeps its own bound sessions, which still have an owner.
        delete_archived_group(state, &bound, deleted)?;
    }
    Ok(true)
}

/// Deletes every archived session of `project`, or every archived session bound to the console
/// session `console_session_id`, or with neither every archived console session of `console` along
/// with the archived sessions bound to them; an archived lead session in either of the last two
/// takes its own archived sessions along. One that stopped being archived meanwhile (a resume
/// got there first) is skipped. Returns every session deleted.
fn delete_archived_sessions(
    state: &Arc<AppState>,
    console_id: &str,
    project_id: Option<&str>,
    console_session_id: Option<&str>,
) -> Result<Vec<Session>> {
    let _removal = state.claim_session_removal();
    state
        .store
        .get_console(console_id)?
        .ok_or_else(|| CodedError::unknown_console(console_id))?;
    if project_id.is_some() && console_session_id.is_some() {
        return Err(CodedError::raised(
            error_code::CONFLICTING_FIELDS,
            "`project` and `console_session` cannot both be given",
            &[("first", "project"), ("second", "console_session")],
        ));
    }
    if let Some(project_id) = project_id {
        let project = state
            .store
            .get_project(project_id)?
            .ok_or_else(|| CodedError::unknown_project(project_id))?;
        if project.console_id != console_id {
            return Err(CodedError::unknown_project(project_id));
        }
    }
    if let Some(console_session_id) = console_session_id {
        let owner = state.session_record(console_session_id)?;
        if owner.console_id != console_id || owner.role != Role::Console {
            return Err(CodedError::unknown_session(console_session_id));
        }
    }
    let mut deleted = Vec::new();
    let mut selected = Vec::new();
    for session in state.store.list_sessions()? {
        let in_scope = session.console_id == console_id
            && match (project_id, console_session_id) {
                (Some(project_id), _) => session.project_id.as_deref() == Some(project_id),
                (None, Some(owner)) => session.bound_to.as_deref() == Some(owner),
                (None, None) => session.role == Role::Console,
            };
        if in_scope && session.status == SessionStatus::Archived {
            ensure_unlocked(std::iter::once(session.clone()).chain(team(state, &session)?))?;
            selected.push(session);
        }
    }
    for session in selected {
        delete_archived_group(state, &session, &mut deleted)?;
    }
    Ok(deleted)
}

/// The user's own write into a session. Refused rather than queued while the session is waiting for
/// them: they are the one who has to answer the prompt that is blocking it, and they can be told so.
/// Off the runtime, because the write blocks on the PTY and on the wait for the agent to send it.
async fn send_message(state: &Arc<AppState>, id: &str, text: String) -> Result<()> {
    reporting::write_message_off_runtime(state, id, text, reporting::WhenBlocked::Refuse).await?;
    Ok(())
}

/// A report panel form submission. Refused unless `page_id` names its console session's newest page —
/// history pages are read-only, and this is where that is actually enforced; a panel that disables
/// its own submit button on a history page is only reflecting the rule, not the source of it.
/// Refused too when the session that pushed it has the role of a bound session and so has no tool
/// to read the submission through; a lead session, though it reports to an owner too, keeps its
/// pages.
async fn submit_page(state: &Arc<AppState>, page_id: &str, data: serde_json::Value) -> Result<()> {
    let page = state.store.get_page(page_id)?.ok_or_else(|| {
        CodedError::raised(
            error_code::UNKNOWN_PAGE,
            format!("unknown page {page_id}"),
            &[("page", page_id)],
        )
    })?;
    let newest = state.store.newest_page_id(&page.console_session_id)?;
    if newest.as_deref() != Some(page.id.as_str()) {
        return Err(CodedError::raised(
            error_code::PAGE_NOT_CURRENT,
            "this page is no longer current; its form can no longer be submitted",
            &[],
        ));
    }
    // A bound session has no tool to read what the page sent; its pages are withdrawn when it
    // becomes one, so this holds for one left behind by a failed withdrawal, until the daemon's
    // next start sweeps it.
    let pusher = state.store.get_session(&page.console_session_id)?;
    if pusher.is_some_and(|pusher| pusher.is_worker()) {
        return Err(CodedError::raised(
            error_code::UNKNOWN_PAGE,
            format!(
                "unknown page {page_id}: its session has the role of a bound session, which \
                 pushes no pages"
            ),
            &[("page", page_id)],
        ));
    }
    let console_session_id = page.console_session_id;

    let message = reporting::render_page_submission(&page.id, &data);
    let owned_state = state.clone();
    // The write blocks on the PTY. Queued rather than refused: the user is the one submitting, so
    // there is nobody to tell to answer a prompt first, and the submission must not be dropped.
    tokio::task::spawn_blocking(move || {
        reporting::write_message(
            &owned_state,
            &console_session_id,
            &message,
            reporting::WhenBlocked::Queue,
        )
    })
    .await??;
    Ok(())
}

/// Refuses to open a session whose resolved agent has been *determined* unavailable — never while
/// that determination is still pending, which the launch's own refusal of a missing binary already
/// covers; see "When the agent or the account cannot open a session" in
/// `docs/product/opening-sessions.md`. Checked here, ahead of everything that follows, so this is
/// also what a console session's own `new_session` tool is refused through — it calls this same
/// function.
fn refuse_if_agent_unavailable(state: &Arc<AppState>, agent: Agent) -> Result<()> {
    if agent_is_available(state, agent) {
        return Ok(());
    }
    Err(CodedError::raised(
        error_code::AGENT_NOT_AVAILABLE,
        format!(
            "{} is not available: its binary does not resolve on your login shell's PATH. {} \
             supports Claude Code, Codex and Grok Build; install one of them and make sure it is \
             on your PATH to open a session.",
            agent.label(),
            crate::APP_NAME
        ),
        &[("agent", agent.label())],
    ))
}

/// Refuses to put a session on an account whose plan quota is used up, going by the reading held
/// at this moment (see [`account_quotas::AccountQuotas::exhausted`]): an account not read yet, or
/// of an agent that is not probed, is not refused, and nothing waits for a fresh reading. Applies
/// to every origin, the user's own request included. `config_dir` is the account's directory as
/// [`Store::account_dir`] resolves it, so a reading taken under another directory is ignored.
fn refuse_if_account_quota_exhausted(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<&str>,
    config_dir: Option<&str>,
) -> Result<()> {
    let now = now_millis();
    let Some(exhausted) = used_up_quota(state, agent, account_id, config_dir, now) else {
        return Ok(());
    };
    let account = state.store.account_display_name(agent, account_id)?;
    let resets = exhausted.resets_at.map(|at| at.to_string());
    let minutes = exhausted.window_minutes.map(|minutes| minutes.to_string());
    let mut params = vec![
        ("agent", agent.label()),
        ("account", account.as_str()),
        ("window", exhausted.window.as_str()),
        ("window_kind", exhausted.kind.wire_name()),
    ];
    if let Some(minutes) = minutes.as_deref() {
        params.push(("window_minutes", minutes));
    }
    if exhausted.has_model_weekly {
        params.push(("model_weekly", "true"));
    }
    if let Some(resets) = resets.as_deref() {
        params.push(("resets_at", resets));
    }
    let resets_in = exhausted
        .resets_at
        .map(|at| format!(", which resets in about {}", duration_label(at - now)))
        .unwrap_or_default();
    Err(CodedError::raised(
        error_code::ACCOUNT_QUOTA_EXHAUSTED,
        format!(
            "{account} has used up its {} quota ({}){resets_in}. Choose another account, or wait \
             for the quota to reset.",
            agent.label(),
            exhausted.named_for(agent)
        ),
        &params,
    ))
}

/// Refuses an account the held quota reading found not signed in (see
/// [`account_quotas::AccountQuotas::signed_out`]); a reading that failed for another reason, or
/// none yet, never refuses. `config_dir` is the account's directory as [`Store::account_dir`]
/// resolves it, so a reading taken under another directory is ignored.
fn refuse_if_account_not_signed_in(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<&str>,
    config_dir: Option<&str>,
) -> Result<()> {
    if !state
        .account_quotas
        .signed_out(agent, account_id, config_dir)
    {
        return Ok(());
    }
    let account = state.store.account_display_name(agent, account_id)?;
    Err(CodedError::raised(
        error_code::ACCOUNT_NOT_SIGNED_IN,
        format!(
            "{account} ({}) is not logged in. Choose another account, or log in to it in Settings \
             → Agent accounts.",
            agent.label()
        ),
        &[("agent", agent.label()), ("account", account.as_str())],
    ))
}

/// The used-up window that keeps a new session off `account_id` (`None` the default account) of
/// `agent` at `now`, judged from the reading held alone; `config_dir` is the account's directory,
/// so a reading taken under another one does not count. What
/// [`refuse_if_account_quota_exhausted`] refuses and what [`account_can_open`] passes over.
fn used_up_quota(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<&str>,
    config_dir: Option<&str>,
    now: i64,
) -> Option<account_quotas::Exhausted> {
    state
        .account_quotas
        .exhausted(agent, account_id, config_dir, now)
}

/// Whether a new session of `agent` would get past the agent's own refusals as things stand
/// ([`refuse_if_agent_unavailable`], [`refuse_unless_agent_added`]). For a caller choosing a
/// default it can fall back from, rather than a refusal.
pub(crate) fn agent_can_open(state: &Arc<AppState>, agent: Agent) -> Result<bool> {
    Ok(agent_is_available(state, agent) && agent_is_added(state, agent)?)
}

/// Whether a new session of `agent` could run on `account_id` (`None` the default account) as
/// things stand: the account is still one of that agent's ([`checked_account_reference`]), it is
/// not turned off ([`refuse_if_account_disabled`]), its quota is not used up
/// ([`refuse_if_account_quota_exhausted`]) and it is not found logged out
/// ([`refuse_if_account_not_signed_in`]). For a caller choosing a default
/// it can fall back from, rather than a refusal.
pub(crate) fn account_can_open(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<&str>,
) -> Result<bool> {
    if state.store.account_disabled(agent, account_id)? {
        return Ok(false);
    }
    let config_dir = match account_id {
        None => None,
        Some(id) => match account_of_agent(state, agent, id)? {
            Some(account) => Some(account.config_dir),
            None => return Ok(false),
        },
    };
    if state
        .account_quotas
        .signed_out(agent, account_id, config_dir.as_deref())
    {
        return Ok(false);
    }
    Ok(used_up_quota(
        state,
        agent,
        account_id,
        config_dir.as_deref(),
        now_millis(),
    )
    .is_none())
}

/// Chooses a new session's agent and account while automatic selection is on. Each agent
/// keeps its own account preference before another agent is tried. An explicit agent restricts
/// the search to it; an explicit account never comes through this function.
pub(crate) fn automatic_open_choice(
    state: &Arc<AppState>,
    preferred: Agent,
    console: &Console,
    project: Option<&Project>,
    inherited: Option<(Agent, Option<String>)>,
    allow_other_agents: bool,
) -> Result<Option<(Agent, Option<String>)>> {
    let usable = |agent, account: Option<&str>| open_now(state, agent, account);
    let mut agents = vec![preferred];
    if allow_other_agents {
        for agent in state
            .store
            .added_agents()?
            .unwrap_or_else(|| Agent::ALL.to_vec())
        {
            if agent != preferred {
                agents.push(agent);
            }
        }
    }
    for agent in agents {
        if !agent_can_open(state, agent)? {
            continue;
        }
        if let Some(account) = preferred_weekly_account(state, agent)? {
            return Ok(Some((agent, account)));
        }
        let inherited = match inherited.as_ref() {
            Some((inherited_agent, account))
                if *inherited_agent == agent && usable(agent, account.as_deref())? =>
            {
                Some(account.clone())
            }
            _ => None,
        };
        let (account, _) = session_account(state, agent, console, project, inherited)?;
        if usable(agent, account.as_deref())? {
            return Ok(Some((agent, account)));
        }
        let mut accounts = vec![None];
        accounts.extend(
            state
                .store
                .list_accounts()?
                .into_iter()
                .filter(|account| account.agent == agent)
                .map(|account| Some(account.id)),
        );
        let order = state.store.account_order(agent)?;
        accounts.sort_by_key(|account| {
            order
                .iter()
                .position(|id| id == account)
                .unwrap_or(usize::MAX)
        });
        for account in accounts {
            if usable(agent, account.as_deref())? {
                return Ok(Some((agent, account)));
            }
        }
    }
    // No agent has a usable account: the preferred agent decides, as in the session dialog.
    if agent_can_open(state, preferred)? {
        if let Some(account) = soonest_available_account(state, preferred)? {
            return Ok(Some((preferred, account)));
        }
    }
    Ok(None)
}

/// Whether a new session of `agent` could open on `account` now: [`account_can_open`] and no
/// sharing change running on it (the launch would be refused as `account_sharing_in_progress`).
fn open_now(state: &Arc<AppState>, agent: Agent, account: Option<&str>) -> Result<bool> {
    Ok(
        !account.is_some_and(|id| state.sharing_locked(id).is_some())
            && account_can_open(state, agent, account)?,
    )
}

/// The account of `agent` that becomes usable soonest, for a new session that leaves its account
/// out while automatic selection is on and no account of the agent can be used. `Some(None)` is
/// the default account. Only an account kept off by a used-up quota alone qualifies (not one turned
/// off, logged out or with a sharing change running); it is usable when the window that blocks it
/// resets, and a blocking window with no reset time counts as never. The earlier candidate keeps a
/// tie, in the order the session dialog uses: the default account, then the stored accounts as
/// they are kept. `None` when an account of the agent is usable, or none qualifies.
pub(crate) fn soonest_available_account(
    state: &Arc<AppState>,
    agent: Agent,
) -> Result<Option<Option<String>>> {
    let mut candidates = vec![None];
    for account in state.store.list_accounts()? {
        if account.agent == agent {
            candidates.push(Some(account.id));
        }
    }
    let now = now_millis();
    let mut soonest: Option<(i64, Option<String>)> = None;
    for account_id in candidates {
        if open_now(state, agent, account_id.as_deref())? {
            return Ok(None);
        }
        if state.store.account_disabled(agent, account_id.as_deref())? {
            continue;
        }
        if account_id
            .as_deref()
            .is_some_and(|id| state.sharing_locked(id).is_some())
        {
            continue;
        }
        let config_dir = match account_id.as_deref() {
            None => None,
            Some(id) => match account_of_agent(state, agent, id)? {
                Some(account) => Some(account.config_dir),
                None => continue,
            },
        };
        if state
            .account_quotas
            .signed_out(agent, account_id.as_deref(), config_dir.as_deref())
        {
            continue;
        }
        let Some(exhausted) = used_up_quota(
            state,
            agent,
            account_id.as_deref(),
            config_dir.as_deref(),
            now,
        ) else {
            continue;
        };
        let at = exhausted.resets_at.unwrap_or(i64::MAX);
        if soonest.as_ref().is_none_or(|(best, _)| at < *best) {
            soonest = Some((at, account_id));
        }
    }
    Ok(soonest.map(|(_, account)| account))
}

/// The account of `agent` to use when a new session leaves its account out and
/// `auto_select_account` is on. `Some(Some(id))` is a stored account, `Some(None)` the default
/// account, and `None` when no account has a weekly window that still has quota and resets
/// ahead, skipping a disabled account, one found logged out, and an account whose sharing change
/// is running (a launch on it would be refused as `account_sharing_in_progress`). The soonest
/// weekly reset wins, except that weekly resets less than one session window apart from the
/// soonest count as tied. Among the tied, an account whose 5-hour session window is running goes
/// first, then the sooner session reset, the sooner weekly reset and the larger remainder, in
/// that order (see [`account_quotas::preferred_signal`]); an earlier candidate wins one that is
/// still tied — the default account, then the stored accounts in the order they are kept. Rank
/// from the reading held for the account's current directory. The list of readings carries no
/// directory, so one still stored under another directory would be chosen.
pub(crate) fn preferred_weekly_account(
    state: &Arc<AppState>,
    agent: Agent,
) -> Result<Option<Option<String>>> {
    preferred_weekly_account_except(state, agent, &[], false)
}

/// [`preferred_weekly_account`] without the accounts in `except` (`None` the default account), for
/// a session that is to be moved off one of them. With `readable_only` also without an account
/// whose last attempt to read its quota failed, whatever the reading still holds: a session is
/// not moved onto an account that cannot be seen to have quota left.
pub(crate) fn preferred_weekly_account_except(
    state: &Arc<AppState>,
    agent: Agent,
    except: &[Option<String>],
    readable_only: bool,
) -> Result<Option<Option<String>>> {
    let mut candidates = vec![None];
    for account in state.store.list_accounts()? {
        if account.agent == agent {
            candidates.push(Some(account.id));
        }
    }
    candidates.retain(|candidate| !except.contains(candidate));
    let now = now_millis();
    let mut signals = Vec::new();
    let mut accounts = Vec::new();
    for account_id in candidates {
        if !account_can_open(state, agent, account_id.as_deref())? {
            continue;
        }
        if account_id
            .as_deref()
            .is_some_and(|id| state.sharing_locked(id).is_some())
        {
            continue;
        }
        let config_dir = match account_id.as_deref() {
            None => None,
            Some(id) => match account_of_agent(state, agent, id)? {
                Some(account) => Some(account.config_dir),
                None => continue,
            },
        };
        let Some(quota) =
            state
                .account_quotas
                .held(agent, account_id.as_deref(), config_dir.as_deref())
        else {
            continue;
        };
        if readable_only && quota.unavailable.is_some() {
            continue;
        }
        let Some(signal) = account_quotas::AccountSignal::of(&quota.windows, now) else {
            continue;
        };
        signals.push(signal);
        accounts.push(account_id);
    }
    Ok(account_quotas::preferred_signal(&signals).map(|index| accounts.swap_remove(index)))
}

/// A span of milliseconds in the rough terms of a message to read: `3d 4h`, `5h 20m` or `7m`.
fn duration_label(millis: i64) -> String {
    let minutes = millis.max(0) / 60_000;
    let (days, hours, minutes) = (minutes / (24 * 60), minutes / 60 % 24, minutes % 60);
    match (days, hours) {
        (0, 0) => format!("{}m", minutes.max(1)),
        (0, _) => format!("{hours}h {minutes}m"),
        _ => format!("{days}d {hours}h"),
    }
}

/// Refuses to open a session of an agent that is not on the added list; see "When the agent or the
/// account cannot open a session" in `docs/product/opening-sessions.md`. An undecided list refuses
/// nothing. Only a session being opened is checked, so a live session of an agent removed since
/// keeps running, and resuming, restarting or reopening an existing session never comes here.
fn refuse_unless_agent_added(state: &Arc<AppState>, agent: Agent) -> Result<()> {
    if agent_is_added(state, agent)? {
        return Ok(());
    }
    Err(CodedError::raised(
        error_code::AGENT_NOT_ADDED,
        format!(
            "{} is not added. Add it in Settings → Agent accounts to open a session with it.",
            agent.label()
        ),
        &[("agent", agent.label())],
    ))
}

/// Whether `agent` is on the added list; while the list is undecided every agent is.
fn agent_is_added(state: &Arc<AppState>, agent: Agent) -> Result<bool> {
    Ok(state
        .store
        .added_agents()?
        .is_none_or(|added| added.contains(&agent)))
}

/// The agent a session is of: an explicit choice wins over the project's default, then the
/// console's project session default. A console session has no project, so it falls straight
/// back to the console's console session default.
pub(crate) fn session_agent(
    chosen: Option<Agent>,
    project: Option<&Project>,
    console: &Console,
) -> Agent {
    chosen.unwrap_or_else(|| match project {
        Some(project) => project
            .default_agent
            .unwrap_or_else(|| console.project_session_default()),
        None => console.default_agent,
    })
}

/// The account id and the config directory a session of `agent` should launch with, and that
/// account's directory at this moment — `None` for both when it is the default account (the state
/// of pinning nothing). In descending priority: the choice made for this session (`chosen`, where
/// `Some(None)` is a choice of the default account), the project's account for the agent, the
/// console's account for the agent, the agent's default account. A project that says the default
/// account ends the chain there, whatever the console references; one that inherits falls through
/// to the console. Mirrors the agent chain in
/// [`open_session`]. A console session has no project, so its chain skips that step.
fn session_account(
    state: &Arc<AppState>,
    agent: Agent,
    console: &Console,
    project: Option<&Project>,
    chosen: Option<Option<String>>,
) -> Result<(Option<String>, Option<String>)> {
    let account_id = match chosen {
        Some(choice) => checked_account_reference(state, agent, choice)?,
        None => match project {
            Some(project) => console.resolve_project_account(agent, project.account(agent)),
            None => console.account(agent).clone(),
        },
    };
    let config_dir = state.store.account_dir(&account_id)?;
    Ok((account_id, config_dir))
}

/// A reference to an account of `agent` as stored, or the `unknown_account` refusal when `id`
/// names no account or one of another agent: an account belongs to one agent, and a reference
/// across agents would launch it with another agent's directory.
fn checked_account_reference(
    state: &Arc<AppState>,
    agent: Agent,
    id: Option<String>,
) -> Result<Option<String>> {
    let Some(id) = id else { return Ok(None) };
    match account_of_agent(state, agent, &id)? {
        Some(_) => Ok(Some(id)),
        None => Err(CodedError::unknown_account(&id)),
    }
}

/// The stored account `id`, when it is one of `agent`'s.
fn account_of_agent(state: &Arc<AppState>, agent: Agent, id: &str) -> Result<Option<Account>> {
    Ok(state
        .store
        .get_account(id)?
        .filter(|account| account.agent == agent))
}

/// A project's setting for `agent` as stored, or the `unknown_account` refusal when it names an
/// account that does not exist or belongs to another agent; the other two states name nothing to
/// check.
fn checked_project_account(
    state: &Arc<AppState>,
    agent: Agent,
    account: ProjectAccount,
) -> Result<ProjectAccount> {
    match account {
        ProjectAccount::Account { id } => {
            checked_account_reference(state, agent, Some(id.clone()))?;
            Ok(ProjectAccount::Account { id })
        }
        other => Ok(other),
    }
}

/// The comparison key for an account name collision: trimmed and lower-cased, so "Work" and
/// " work " read as the same name.
pub(crate) fn account_name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// The name of the account whose name collides with `name` within `agent`, if any — the default
/// account's name takes part, compared against [`crate::protocol::DEFAULT_ACCOUNT_NAME`] since the
/// daemon carries no locale to compare the user's own translated word against. `exclude_id` lets a
/// rename compare against every *other* account without colliding with itself.
pub(crate) fn colliding_account_name(
    state: &Arc<AppState>,
    agent: Agent,
    name: &str,
    exclude_id: Option<&str>,
) -> Result<Option<String>> {
    let wanted = account_name_key(name);
    if wanted == account_name_key(crate::protocol::DEFAULT_ACCOUNT_NAME) {
        return Ok(Some(crate::protocol::DEFAULT_ACCOUNT_NAME.to_string()));
    }
    if let Some(custom) = state
        .store
        .default_account_names()?
        .get(agent.wire_name())
        .filter(|custom| account_name_key(custom) == wanted)
    {
        return Ok(Some(custom.clone()));
    }
    colliding_owned_account_name(state, agent, name, exclude_id)
}

/// The name of the account of `agent` that is a row (so not the default) whose name collides with
/// `name`, if any.
pub(crate) fn colliding_owned_account_name(
    state: &Arc<AppState>,
    agent: Agent,
    name: &str,
    exclude_id: Option<&str>,
) -> Result<Option<String>> {
    let wanted = account_name_key(name);
    for existing in state.store.list_accounts()? {
        if existing.agent == agent
            && Some(existing.id.as_str()) != exclude_id
            && account_name_key(&existing.name) == wanted
        {
            return Ok(Some(existing.name));
        }
    }
    Ok(None)
}

/// Names the agent's default account, or clears the name when `name` is blank or is the word the
/// daemon shows for an unnamed default account, in any letter case. The name takes part in the
/// same collision check as an account's, so neither can take the other's; the default's own name
/// is no collision, so only the accounts that are rows are compared.
pub(crate) fn set_default_account_name(
    state: &Arc<AppState>,
    agent: Agent,
    name: &str,
) -> Result<()> {
    let _writes = state.lock_account_writes();
    write_default_account_name(state, agent, name.trim())
}

/// [`set_default_account_name`] for a default account that still has no name of its own, checked
/// under the lock the write takes so a rename made in between is not overwritten. Whether the name
/// was written: `false` when the default account was named already, or when `name` is blank or
/// the word shown for an unnamed one, which clears the name and so writes nothing here.
pub(crate) fn name_unnamed_default_account(
    state: &Arc<AppState>,
    agent: Agent,
    name: &str,
) -> Result<bool> {
    let name = name.trim();
    if clears_default_account_name(name) {
        return Ok(false);
    }
    let _writes = state.lock_account_writes();
    if state
        .store
        .default_account_names()?
        .contains_key(agent.wire_name())
    {
        return Ok(false);
    }
    write_default_account_name(state, agent, name)?;
    Ok(true)
}

/// Whether `name`, given to a default account, clears its name instead: it is blank, or the word
/// shown for an unnamed default account.
pub(crate) fn clears_default_account_name(name: &str) -> bool {
    let key = account_name_key(name);
    key.is_empty() || key == account_name_key(crate::protocol::DEFAULT_ACCOUNT_NAME)
}

/// Writes the trimmed `name` of `agent`'s default account; the caller holds the account writes
/// lock.
fn write_default_account_name(state: &Arc<AppState>, agent: Agent, name: &str) -> Result<()> {
    let changed = if clears_default_account_name(name) {
        state.store.set_default_account_name(agent, None)?
    } else {
        if let Some(existing) = colliding_owned_account_name(state, agent, name, None)? {
            return Err(account_name_taken(agent, name, &existing));
        }
        state.store.set_default_account_name(agent, Some(name))?
    };
    if changed {
        broadcast_settings(state)?;
    }
    Ok(())
}

/// Turns an account off for new work, or back on, and broadcasts the settings when the flag
/// changed. `None` is `agent`'s default account. A running session is not looked at: it keeps its
/// account. Turning an account back on starts the usage window its held reading shows unstarted,
/// when the setting that does that is on ([`account_quotas::kick_held`]).
fn set_account_disabled(
    state: &Arc<AppState>,
    agent: Agent,
    account: Option<&str>,
    disabled: bool,
) -> Result<()> {
    let _writes = state.lock_account_writes();
    let changed = match account {
        None => state.store.set_default_account_disabled(agent, disabled)?,
        Some(id) => {
            if account_of_agent(state, agent, id)?.is_none() {
                return Err(CodedError::unknown_account(id));
            }
            state.store.set_account_disabled(id, disabled)?
        }
    };
    if changed {
        broadcast_settings(state)?;
        // The held readings are looked over again, as when the setting that starts quota windows
        // is turned on, so a window the re-enabled account shows unstarted is not left waiting.
        if !disabled {
            account_quotas::kick_held(state);
        }
    }
    Ok(())
}

/// Refuses to put a new session on an account the user turned off, going by the flag as it stands.
/// Applies to every origin, the user's own request included; a session already on the account is
/// never asked about.
pub(crate) fn refuse_if_account_disabled(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<&str>,
) -> Result<()> {
    if !state.store.account_disabled(agent, account_id)? {
        return Ok(());
    }
    let account = state.store.account_display_name(agent, account_id)?;
    Err(CodedError::raised(
        error_code::ACCOUNT_DISABLED,
        format!(
            "{account} is disabled for {}. Choose another account, or turn it back on in Settings \
             → Agent accounts.",
            agent.label()
        ),
        &[("agent", agent.label()), ("account", account.as_str())],
    ))
}

/// Refuses a name that collides within its agent, naming the account it collides with.
fn check_account_name_unique(
    state: &Arc<AppState>,
    agent: Agent,
    name: &str,
    exclude_id: Option<&str>,
) -> Result<()> {
    match colliding_account_name(state, agent, name, exclude_id)? {
        Some(existing) => Err(account_name_taken(agent, name, &existing)),
        None => Ok(()),
    }
}

pub(crate) fn account_name_taken(agent: Agent, name: &str, existing: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::ACCOUNT_NAME_TAKEN,
        format!(
            "the name `{name}` is already used by `{existing}` for {}",
            agent.label()
        ),
        &[("agent", agent.label()), ("name", existing)],
    )
}

/// The name of the account whose directory is `dir` (as [`normalize_account_dir`] stores it)
/// within `agent`, if any. The default account takes part under
/// [`crate::protocol::DEFAULT_ACCOUNT_NAME`], by the directory the daemon resolved when it
/// determined availability; until then, and for the whole run if that never succeeds, by the
/// conventional one the adapter falls back to when nothing overrides it (`~/.claude`, `~/.codex`,
/// `~/.grok`). Both go through the same expansion and lexical normalisation as `dir`. Symlinks are
/// not followed. `exclude_id` lets an update compare against every *other* account.
pub(crate) fn colliding_account_dir(
    state: &Arc<AppState>,
    agent: Agent,
    dir: &str,
    exclude_id: Option<&str>,
) -> Result<Option<String>> {
    let key = account_dir_key(dir);
    for existing in state.store.list_accounts()? {
        if existing.agent == agent
            && Some(existing.id.as_str()) != exclude_id
            && account_dir_key(&existing.config_dir) == key
        {
            return Ok(Some(existing.name));
        }
    }
    let default_dir = match state.agent_availability_of(agent).default_account_dir {
        Some(resolved) => hostfs::expand(&resolved),
        None => adapter::default_account_dir(agent, &HashMap::new()),
    };
    let default_dir = hostfs::lexically_normalise(&default_dir);
    if account_dir_key(&default_dir.to_string_lossy()) != key {
        return Ok(None);
    }
    Ok(Some(state.store.default_account_name(agent)?))
}

/// Refuses a directory another account of the agent already uses, naming that account.
fn check_account_dir_unique(
    state: &Arc<AppState>,
    agent: Agent,
    dir: &str,
    exclude_id: Option<&str>,
) -> Result<()> {
    if let Some(existing) = colliding_account_dir(state, agent, dir, exclude_id)? {
        return Err(CodedError::raised(
            error_code::CONFIG_DIR_TAKEN,
            format!(
                "the directory `{dir}` is already used by `{existing}` for {}",
                agent.label()
            ),
            &[("agent", agent.label()), ("name", &existing)],
        ));
    }
    Ok(())
}

/// A name as stored: trimmed, and refused if that leaves nothing.
pub(crate) fn normalize_account_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(field_required("name"));
    }
    Ok(trimmed.to_string())
}

/// Broadcasts the current settings record — the pattern every settings-shaped mutation (the
/// trusted directories, `auto_pull_repositories`, and now the accounts) follows.
pub(crate) fn broadcast_settings(state: &AppState) -> Result<()> {
    state.broadcast(Event::SettingsUpdated {
        settings: state.store.get_settings()?,
    });
    Ok(())
}

/// Where one agent's launch override is kept: one setting for the agent, applied to a session of
/// any of its accounts. Codex's approval reviewer and Claude Code's approval mode are handled
/// alike through it.
struct LaunchOverride<T> {
    agent: Agent,
    get: fn(&Store) -> Result<Option<T>>,
    set: fn(&Store, Option<T>) -> Result<()>,
}

const CODEX_REVIEWER: LaunchOverride<ApprovalReviewer> = LaunchOverride {
    agent: Agent::Codex,
    get: Store::codex_approval_reviewer,
    set: Store::set_codex_approval_reviewer,
};

const CLAUDE_MODE: LaunchOverride<ClaudeApprovalMode> = LaunchOverride {
    agent: Agent::Claude,
    get: Store::claude_approval_mode,
    set: Store::set_claude_approval_mode,
};

impl<T: PartialEq + Copy> LaunchOverride<T> {
    /// Resolves the override at every launch: the agent's setting, whichever account the session
    /// uses, and nothing for a session of another agent.
    fn for_session(&self, state: &Arc<AppState>, session: &Session) -> Result<Option<T>> {
        if session.agent != self.agent {
            return Ok(None);
        }
        (self.get)(&state.store)
    }

    /// Sets the agent's override and broadcasts the settings unless it already holds that value.
    fn set(&self, state: &Arc<AppState>, value: Option<T>) -> Result<()> {
        if (self.get)(&state.store)? == value {
            return Ok(());
        }
        (self.set)(&state.store, value)?;
        broadcast_settings(state)?;
        Ok(())
    }
}

fn create_account(
    state: &Arc<AppState>,
    agent: Agent,
    name: String,
    config_dir: String,
) -> Result<()> {
    insert_new_account(state, agent, name, config_dir, false)?;
    Ok(())
}

/// Creates an account through the checks of the Add account form (the name rules, the directory
/// rules, a name and a directory no other account of the agent has), stores it with the given
/// `disabled` flag, broadcasts the settings and returns it. Sharing is not set: it is a change of
/// its own (`shared_config::set`).
pub(crate) fn insert_new_account(
    state: &Arc<AppState>,
    agent: Agent,
    name: String,
    config_dir: String,
    disabled: bool,
) -> Result<Account> {
    let name = normalize_account_name(&name)?;
    let config_dir = normalize_account_dir(agent, &config_dir)?;
    let account = Account {
        id: Uuid::new_v4().to_string(),
        agent,
        name,
        config_dir,
        share_default_config: false,
        share_default_config_incomplete: false,
        disabled,
    };
    {
        // The checks and the insert are one step: another create or update cannot slip between.
        let _writes = state.lock_account_writes();
        check_account_name_unique(state, agent, &account.name, None)?;
        check_account_dir_unique(state, agent, &account.config_dir, None)?;
        state.store.insert_account(&account)?;
    }
    broadcast_settings(state)?;
    Ok(account)
}

fn update_account(
    state: &Arc<AppState>,
    id: &str,
    name: Option<String>,
    config_dir: Option<String>,
) -> Result<()> {
    let repointed = {
        // Reading the account, the checks and the write are one step: another create or update
        // cannot slip between them.
        let _writes = state.lock_account_writes();
        let mut account = state
            .store
            .get_account(id)?
            .ok_or_else(|| CodedError::unknown_account(id))?;
        let mut changed = false;
        let mut dir_changed = false;
        if let Some(name) = name {
            let name = normalize_account_name(&name)?;
            if name != account.name {
                check_account_name_unique(state, account.agent, &name, Some(id))?;
                account.name = name;
                changed = true;
            }
        }
        if let Some(dir) = config_dir {
            let dir = normalize_account_dir(account.agent, &dir)?;
            if dir != account.config_dir {
                check_account_dir_unique(state, account.agent, &dir, Some(id))?;
                account.config_dir = dir;
                changed = true;
                dir_changed = true;
            }
        }
        if !changed {
            return Ok(());
        }
        state.store.update_account(&account)?;
        dir_changed.then_some(account.agent)
    };
    // A reading belongs to the directory it was taken under, and a login signs in to it; a rename
    // leaves both valid.
    if let Some(agent) = repointed {
        state.account_quotas.forget_account(state, id);
        let _ = state.account_logins.cancel(agent, Some(id.to_string()));
        // What runs under the account's directory is now what runs under the new one.
        crate::shared_config::publish(state);
    }
    broadcast_settings(state)?;
    Ok(())
}

fn delete_account(state: &Arc<AppState>, id: &str) -> Result<()> {
    let account = state
        .store
        .get_account(id)?
        .ok_or_else(|| CodedError::unknown_account(id))?;
    let affected_consoles: Vec<String> = state
        .store
        .list_consoles()?
        .into_iter()
        .filter(|console| console_references_account(console, id))
        .map(|console| console.id)
        .collect();
    let affected_projects: Vec<String> = state
        .store
        .list_projects()?
        .into_iter()
        .filter(|project| project_references_account(project, id))
        .map(|project| project.id)
        .collect();
    state.store.delete_account(id)?;
    state.account_quotas.forget_account(state, id);
    crate::shared_config::publish(state);
    let _ = state
        .account_logins
        .cancel(account.agent, Some(id.to_string()));
    for console_id in affected_consoles {
        if let Some(console) = state.store.get_console(&console_id)? {
            state.broadcast(Event::ConsoleUpserted { console });
        }
    }
    for project_id in affected_projects {
        if let Some(project) = state.store.get_project(&project_id)? {
            state.broadcast(Event::ProjectUpserted { project });
        }
    }
    broadcast_settings(state)?;
    Ok(())
}

/// Adds an agent that is installed. While availability is still undetermined it is refused rather
/// than guessed at, and the list is filled first if the determination has landed since: the list
/// must be decided by `availability::fill_added_agents` before a user's edit takes it over, or the
/// agents with accounts would be left off it.
fn add_agent(state: &Arc<AppState>, agent: Agent) -> Result<()> {
    match state.agent_availability_of(agent).availability {
        Availability::Available => {}
        Availability::Unavailable => {
            return Err(CodedError::raised(
                error_code::AGENT_NOT_INSTALLED,
                format!("{} is not installed on this machine", agent.label()),
                &[("agent", agent.label())],
            ));
        }
        Availability::NotDetermined => {
            return Err(CodedError::raised(
                error_code::AGENT_AVAILABILITY_PENDING,
                format!(
                    "{} cannot be added until it is known which agents are installed",
                    agent.label()
                ),
                &[("agent", agent.label())],
            ));
        }
    }
    availability::fill_added_agents(state)?;
    if !state.store.add_added_agent(agent)? {
        return Err(CodedError::raised(
            error_code::AGENT_ALREADY_ADDED,
            format!("{} is already added", agent.label()),
            &[("agent", agent.label())],
        ));
    }
    broadcast_settings(state)?;
    Ok(())
}

/// Takes an agent off the list. Its accounts stay, so they reappear when it is added again. An
/// undecided list is filled first, as `add_agent` does, so the fill cannot put the agent back; if
/// availability is still unknown it stays undecided and the request is refused.
fn remove_agent(state: &Arc<AppState>, agent: Agent) -> Result<()> {
    availability::fill_added_agents(state)?;
    if state.store.added_agents()?.is_none() {
        return Err(added_agents_undecided("removed from"));
    }
    if state.store.remove_added_agent(agent)? {
        broadcast_settings(state)?;
    }
    Ok(())
}

/// Puts the added agents in the given order. An undecided list is filled first, as `add_agent`
/// does; if availability is still unknown it stays undecided and the request is refused.
fn reorder_agents(state: &Arc<AppState>, order: &[Agent]) -> Result<()> {
    availability::fill_added_agents(state)?;
    match state.store.reorder_added_agents(order)? {
        ReorderOutcome::Undecided => Err(added_agents_undecided("reordered")),
        ReorderOutcome::NotAPermutation => Err(CodedError::raised(
            error_code::AGENT_ORDER_INVALID,
            "the new order must name each added agent exactly once".to_string(),
            &[],
        )),
        ReorderOutcome::Unchanged => Ok(()),
        ReorderOutcome::Reordered => broadcast_settings(state),
    }
}

/// The refusal of a change to the added agents while the list is still undecided.
fn added_agents_undecided(what: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::ADDED_AGENTS_UNDECIDED,
        format!("the added agents cannot be {what} until it is known which agents are installed"),
        &[],
    )
}

fn console_references_account(console: &Console, account_id: &str) -> bool {
    [
        &console.claude_account_id,
        &console.codex_account_id,
        &console.grok_account_id,
    ]
    .iter()
    .any(|referenced| referenced.as_deref() == Some(account_id))
}

fn project_references_account(project: &Project, account_id: &str) -> bool {
    [
        &project.claude_account_id,
        &project.codex_account_id,
        &project.grok_account_id,
    ]
    .iter()
    .any(|referenced| referenced.account_id() == Some(account_id))
}

/// The longest avatar `data:` URL a console may carry. The UI sends a 128x128 image, which is
/// far smaller; the bound is there because the value goes to every client with each console.
const MAX_ICON_BYTES: usize = 256 * 1024;

/// Renames a session and broadcasts it. The title is stored trimmed and cannot be blank.
pub fn rename_session(state: &Arc<AppState>, id: &str, title: &str) -> Result<Session> {
    state.set_session_title(id, &required_text("title", title)?)
}

/// Pins or unpins a session and broadcasts it.
pub fn set_session_pinned(state: &Arc<AppState>, id: &str, pinned: bool) -> Result<Session> {
    state.store.set_session_pinned(id, pinned)?;
    let session = state.session_record(id)?;
    state.publish_session(&session);
    Ok(session)
}

pub fn set_session_locked(state: &Arc<AppState>, id: &str, locked: bool) -> Result<Session> {
    state.set_session_locked(id, locked)?;
    let session = state.session_record(id)?;
    state.publish_session(&session);
    Ok(session)
}

/// Sets a console session's badge colour and broadcasts it. A project session carries none.
pub fn set_session_colour(
    state: &Arc<AppState>,
    id: &str,
    colour: ConsoleSessionColour,
) -> Result<Session> {
    if state.session_record(id)?.role != Role::Console {
        anyhow::bail!("only a console session has a colour");
    }
    state.store.set_session_colour(id, colour)?;
    let session = state.session_record(id)?;
    state.publish_session(&session);
    Ok(session)
}

/// A console's avatar as it is stored. Blank means unset. Checked lightly: it has to be an image
/// `data:` URL and not too long; whether it decodes is left to the client that draws it.
fn normalize_icon(text: Option<&str>) -> Result<Option<String>> {
    let Some(text) = trimmed_or_none(text) else {
        return Ok(None);
    };
    if !text.starts_with("data:image/") {
        return Err(CodedError::raised(
            error_code::ICON_NOT_AN_IMAGE,
            "the console avatar must be an image `data:` URL",
            &[],
        ));
    }
    if text.len() > MAX_ICON_BYTES {
        return Err(CodedError::raised(
            error_code::ICON_TOO_LARGE,
            format!(
                "the console avatar is larger than {} KiB",
                MAX_ICON_BYTES / 1024
            ),
            &[("limit_kib", &(MAX_ICON_BYTES / 1024).to_string())],
        ));
    }
    Ok(Some(text))
}

/// What `update_settings` changes; a field left at its default is left alone.
#[derive(Default)]
pub struct SettingsChanges {
    pub auto_pull_repositories: Option<bool>,
    pub auto_delete_unresumable_archives: Option<bool>,
    pub auto_approve_session_requests: Option<bool>,
    pub auto_approve_resume_requests: Option<bool>,
    pub auto_approve_account_switches: Option<bool>,
    pub auto_approve_handoffs: Option<bool>,
    pub ask_questions_as_choices: Option<bool>,
    /// Refused unless it is a tag the interface offers.
    pub interface_language: Option<String>,
    pub auto_select_account: Option<bool>,
    pub auto_switch_on_usage_limit: Option<bool>,
    pub sidebar_order_by_status: Option<bool>,
    pub auto_start_quota_windows: Option<bool>,
    /// Blank text goes back to the built-in default.
    pub default_clone_dir: Option<String>,
    pub git_workflow: GitWorkflowSettingsChanges,
    /// By agent wire name; an agent left out is left alone. Refused for Claude Code, which has no
    /// such setting, and for a name that is no agent.
    pub claude_compatibility: std::collections::BTreeMap<String, bool>,
}

/// Applies `changes`: absent means "leave it alone", as in `update_project`. Broadcasts
/// `settings_updated` only when something actually changed, the trusted-directories pattern.
pub(crate) fn update_settings(state: &Arc<AppState>, changes: SettingsChanges) -> Result<()> {
    let SettingsChanges {
        auto_pull_repositories,
        auto_delete_unresumable_archives,
        auto_approve_session_requests,
        auto_approve_resume_requests,
        auto_approve_account_switches,
        auto_approve_handoffs,
        ask_questions_as_choices,
        interface_language,
        auto_select_account,
        auto_switch_on_usage_limit,
        sidebar_order_by_status,
        auto_start_quota_windows,
        default_clone_dir,
        git_workflow,
        claude_compatibility,
    } = changes;
    if let Some(tag) = &interface_language {
        if !crate::interface_language::is_offered(tag) {
            anyhow::bail!("`{tag}` is not a language the interface is offered in");
        }
    }
    let claude_compatibility = claude_compatibility
        .into_iter()
        .map(|(name, enabled)| match Agent::from_wire_name(&name) {
            Some(agent) if agent.has_claude_compatibility() => Ok((agent, enabled)),
            _ => Err(anyhow::anyhow!(
                "`{name}` has no Claude compatibility setting; it applies to codex and grok"
            )),
        })
        .collect::<Result<Vec<_>>>()?;
    // Checked before anything is written, so a refused directory leaves the other field unapplied
    // too rather than half the request taking effect.
    let default_clone_dir = match default_clone_dir {
        Some(text) if text.trim().is_empty() => Some(None),
        Some(text) => Some(Some(absolute_path(&text)?.to_string_lossy().into_owned())),
        None => None,
    };
    let mut changed = false;
    let mut pull_turned_on = false;
    let mut quota_windows_turned_on = false;
    if let Some(value) = auto_pull_repositories {
        let updated = state.store.set_auto_pull_repositories(value)?;
        pull_turned_on = updated && value;
        changed |= updated;
    }
    if let Some(value) = auto_delete_unresumable_archives {
        changed |= state.store.set_auto_delete_unresumable_archives(value)?;
    }
    if let Some(value) = auto_approve_session_requests {
        changed |= state.store.set_auto_approve_session_requests(value)?;
    }
    if let Some(value) = auto_approve_resume_requests {
        changed |= state.store.set_auto_approve_resume_requests(value)?;
    }
    if let Some(value) = auto_approve_account_switches {
        changed |= state.store.set_auto_approve_account_switches(value)?;
    }
    if let Some(value) = auto_approve_handoffs {
        changed |= state.store.set_auto_approve_handoffs(value)?;
    }
    if let Some(value) = ask_questions_as_choices {
        changed |= state.store.set_ask_questions_as_choices(value)?;
    }
    if let Some(tag) = interface_language {
        changed |= state.store.set_interface_language(&tag)?;
    }
    if let Some(value) = auto_select_account {
        changed |= state.store.set_auto_select_account(value)?;
    }
    if let Some(value) = auto_switch_on_usage_limit {
        changed |= state.store.set_auto_switch_on_usage_limit(value)?;
        if !value {
            state.quota_recovery.clear();
        }
    }
    if let Some(value) = sidebar_order_by_status {
        changed |= state.store.set_sidebar_order_by_status(value)?;
    }
    if let Some(value) = auto_start_quota_windows {
        let updated = state.store.set_auto_start_quota_windows(value)?;
        quota_windows_turned_on = updated && value;
        changed |= updated;
    }
    if let Some(value) = default_clone_dir {
        changed |= state.store.set_default_clone_dir(value.as_deref())?;
    }
    for (agent, enabled) in claude_compatibility {
        changed |= state.store.set_claude_compatibility(agent, enabled)?;
    }
    changed |= state.store.change_git_workflow_settings(&git_workflow)?;
    if !changed {
        return Ok(());
    }
    state.broadcast(Event::SettingsUpdated {
        settings: state.store.get_settings()?,
    });
    // Turning the automatic pull on takes effect at once rather than at the client's next
    // five-minute sweep: the branches already known to be behind are fast-forwarded now (see
    // `git_status::pull_behind_projects`).
    if pull_turned_on {
        git_status::pull_behind_projects(state);
    }
    // Likewise the readings already held are looked over now, so an account whose window has not
    // started is not left waiting for its next reading.
    if quota_windows_turned_on {
        account_quotas::kick_held(state);
    }
    Ok(())
}

/// Starts a git-status check of every project of `console_id`, concurrently; a project already
/// being checked is left alone. The statuses follow as `project_git_status` broadcasts, so this
/// returns as soon as the checks are started rather than waiting for them.
fn refresh_git_status(state: &Arc<AppState>, console_id: &str) -> Result<()> {
    state
        .store
        .get_console(console_id)?
        .ok_or_else(|| CodedError::unknown_console(console_id))?;
    let projects: Vec<Project> = state
        .store
        .list_projects()?
        .into_iter()
        .filter(|project| project.console_id == console_id)
        .collect();
    git_status::refresh(state, projects);
    Ok(())
}

fn session_cwd(state: &Arc<AppState>, session: &Session) -> Result<PathBuf> {
    match &session.project_id {
        Some(project_id) => {
            let project = state
                .store
                .get_project(project_id)?
                .ok_or_else(|| CodedError::unknown_project(project_id))?;
            Ok(PathBuf::from(project.path))
        }
        None => {
            let console = state
                .store
                .get_console(&session.console_id)?
                .ok_or_else(|| CodedError::unknown_console(&session.console_id))?;
            Ok(PathBuf::from(console.workdir))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths;
    use crate::protocol::Console;
    use crate::test_support::{
        bare_session, console_session, console_with_project_and_accounts, fake_live_session,
        idle_stand_in, project_session, session_ids, ScratchDir,
    };

    /// Owned sessions are counted as the walks of a team count them: in any status, and none for a
    /// session with the role of a bound one, whatever a row says is bound to it.
    #[test]
    fn owned_counts_leave_out_owners_with_the_bound_role() {
        let owned = |id: &str, owner: Option<&str>, lead: bool, status: SessionStatus| Session {
            bound_to: owner.map(str::to_string),
            lead,
            ..project_session(id, Agent::Claude, status)
        };
        let sessions = [
            console_session("console", SessionStatus::Idle),
            owned("lead", Some("console"), true, SessionStatus::Idle),
            owned("bound", Some("console"), false, SessionStatus::Archived),
            owned(
                "under-lead",
                Some("lead"),
                false,
                SessionStatus::Interrupted,
            ),
            owned("under-bound", Some("bound"), false, SessionStatus::Idle),
        ];

        let counts = owned_counts(&sessions);

        assert_eq!(counts.get("console"), Some(&2));
        assert_eq!(counts.get("lead"), Some(&1));
        assert_eq!(counts.get("bound"), None);
        assert_eq!(counts.len(), 2);
    }

    /// The count of a single session is the one the count of every session gives, without
    /// listing them: the same owners, the same numbers, and none for an owner with the role of a
    /// bound one.
    #[test]
    fn the_count_of_one_session_agrees_with_the_count_of_all() {
        let (state, dir) = crate::test_support::app_state("coordinator-owned-count-of");
        let owned = |id: &str, owner: Option<&str>, lead: bool, status: SessionStatus| Session {
            project_id: Some("p1".to_string()),
            bound_to: owner.map(str::to_string),
            lead,
            ..project_session(id, Agent::Claude, status)
        };
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        crate::test_support::insert_project(&state, &dir);
        for session in [
            console_session("console", SessionStatus::Idle),
            owned("lead", Some("console"), true, SessionStatus::Idle),
            owned("bound", Some("console"), false, SessionStatus::Archived),
            owned(
                "under-lead",
                Some("lead"),
                false,
                SessionStatus::Interrupted,
            ),
            owned("under-bound", Some("bound"), false, SessionStatus::Idle),
            owned("alone", None, false, SessionStatus::Idle),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        let all = owned_counts(&state.store.list_sessions().unwrap());

        for session in state.store.list_sessions().unwrap() {
            let one = owned_count_of(&state, &session).unwrap();
            assert_eq!(one.get(&session.id), all.get(&session.id), "{}", session.id);
            assert!(one.len() <= 1);
        }
    }

    #[test]
    fn tags_are_trimmed_and_deduplicated_ignoring_case_keeping_the_first_spelling() {
        let tags = |tags: &[&str]| normalise_tags(tags.iter().map(|tag| tag.to_string()).collect());
        assert_eq!(
            tags(&["  Rust ", "", "   ", "backend", "rust", "BACKEND", "web"]),
            ["Rust", "backend", "web"]
        );
        assert!(tags(&[]).is_empty());
    }

    #[test]
    fn update_settings_applies_all_or_nothing_and_a_blank_clone_dir_clears_it() {
        let (state, _dir) = crate::test_support::app_state("coordinator-update-settings");
        let built_in = state.store.default_clone_dir().unwrap();

        update_settings(
            &state,
            SettingsChanges {
                default_clone_dir: Some("/work/repos/".to_string()),
                ..SettingsChanges::default()
            },
        )
        .unwrap();
        assert_eq!(state.store.default_clone_dir().unwrap(), "/work/repos");

        // A refused directory leaves the other field of the same request unapplied too.
        assert!(update_settings(
            &state,
            SettingsChanges {
                auto_pull_repositories: Some(true),
                default_clone_dir: Some("relative".to_string()),
                ..SettingsChanges::default()
            },
        )
        .is_err());
        assert!(!state.store.get_settings().unwrap().auto_pull_repositories);
        assert_eq!(state.store.default_clone_dir().unwrap(), "/work/repos");

        update_settings(
            &state,
            SettingsChanges {
                default_clone_dir: Some("  ".to_string()),
                ..SettingsChanges::default()
            },
        )
        .unwrap();
        assert_eq!(state.store.default_clone_dir().unwrap(), built_in);

        // The interface language is one of the tags the interface offers; any other is refused
        // before anything of the same request is written.
        assert!(update_settings(
            &state,
            SettingsChanges {
                auto_pull_repositories: Some(true),
                interface_language: Some("klingon".to_string()),
                ..SettingsChanges::default()
            },
        )
        .is_err());
        assert!(!state.store.get_settings().unwrap().auto_pull_repositories);
        assert_eq!(state.store.interface_language().unwrap(), None);
        let mut events = state.subscribe();
        update_settings(
            &state,
            SettingsChanges {
                interface_language: Some("zh-Hans".to_string()),
                ..SettingsChanges::default()
            },
        )
        .unwrap();
        assert_eq!(
            state.store.interface_language().unwrap().as_deref(),
            Some("zh-Hans")
        );
        assert!(matches!(
            events.try_recv(),
            Ok(Event::SettingsUpdated { settings })
                if settings.interface_language.as_deref() == Some("zh-Hans")
        ));
        update_settings(
            &state,
            SettingsChanges {
                interface_language: Some("zh-Hans".to_string()),
                ..SettingsChanges::default()
            },
        )
        .unwrap();
        assert!(events.try_recv().is_err());

        let mut events = state.subscribe();
        update_settings(
            &state,
            SettingsChanges {
                auto_delete_unresumable_archives: Some(false),
                ..SettingsChanges::default()
            },
        )
        .unwrap();
        assert!(!state.store.auto_delete_unresumable_archives().unwrap());
        assert!(matches!(
            events.try_recv(),
            Ok(Event::SettingsUpdated { settings }) if !settings.auto_delete_unresumable_archives
        ));
        update_settings(
            &state,
            SettingsChanges {
                auto_delete_unresumable_archives: Some(false),
                ..SettingsChanges::default()
            },
        )
        .unwrap();
        assert!(events.try_recv().is_err(), "a repeat changes nothing");
    }

    /// Turning the setting that starts quota windows on looks over the readings already held;
    /// saying it is on again changes nothing, so it looks at nothing.
    #[tokio::test]
    async fn turning_the_quota_window_setting_on_kicks_held_readings_and_repeating_it_does_not() {
        use crate::protocol::{AccountQuota, QuotaWindow, QuotaWindowKind};
        let (state, _dir) = crate::test_support::app_state("coordinator-quota-windows");
        let unstarted = |agent| AccountQuota {
            agent,
            account: None,
            fetched_at: 1,
            plan: None,
            email: None,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Session,
                label: None,
                used_percent: 0.0,
                resets_at: None,
                window_minutes: None,
            }],
            unavailable: None,
        };
        let turn_on = |sidebar_order_by_status: Option<bool>| {
            update_settings(
                &state,
                SettingsChanges {
                    auto_start_quota_windows: Some(true),
                    sidebar_order_by_status,
                    ..SettingsChanges::default()
                },
            )
            .unwrap()
        };
        state.account_quotas.hold_for_test(&unstarted(Agent::Codex));
        turn_on(None);
        let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
        while !state.account_quotas.is_kicked_for_test(Agent::Codex, None) {
            assert!(
                std::time::Instant::now() < deadline,
                "the held reading was not kicked"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        // Already on, though the same request does change another setting, so the quota setting
        // itself is what has to be told apart from a request that changed something: a reading
        // held since is left alone.
        state.account_quotas.hold_for_test(&unstarted(Agent::Grok));
        let ordered = state.store.sidebar_order_by_status().unwrap();
        turn_on(Some(!ordered));
        assert_eq!(state.store.sidebar_order_by_status().unwrap(), !ordered);
        // The test's runtime has the one thread, so yielding runs whatever `turn_on` spawned up to
        // its first wait, which is where a kick of the reading would already be claimed.
        tokio::task::yield_now().await;
        assert!(!state.account_quotas.is_kicked_for_test(Agent::Grok, None));
    }

    fn console(default_agent: Agent, workdir: &Path) -> Console {
        Console {
            id: "console-1".to_string(),
            name: "Console".to_string(),
            workdir: workdir.to_string_lossy().into_owned(),
            default_agent,
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
        }
    }

    /// A console workdir inside a store's scratch directory, kept apart from the database, which
    /// deleting the console would otherwise wipe.
    fn console_workdir(dir: &Path) -> PathBuf {
        let workdir = dir.join("workdir");
        std::fs::create_dir(&workdir).expect("workdir");
        workdir
    }

    fn temp_dir(name: &str) -> ScratchDir {
        ScratchDir::new(&format!("coordinator-{name}"))
    }

    /// An account directory is stored absolute and lexically normalised or not at all; unlike a
    /// console's pinned path before accounts existed, it is accepted even when it does not exist,
    /// since it is created on first launch (Grok Build excepted, checked at launch instead).
    #[test]
    fn an_account_dir_is_stored_absolute_or_not_at_all_and_existence_is_not_checked() {
        let dir = temp_dir("config-dir");
        let text = dir.to_string_lossy().into_owned();
        let normalize = |text: &str| normalize_account_dir(Agent::Codex, text);

        assert_eq!(normalize(&format!("  {text} ")).unwrap(), text.clone());
        assert_eq!(normalize(&format!("{text}/")).unwrap(), text.clone());
        assert_eq!(
            normalize(&format!("{text}/./sub/..")).unwrap(),
            text.clone()
        );

        // Now intentionally accepted, where a console's pinned path used to refuse it.
        let missing = format!("{text}/missing");
        assert_eq!(normalize(&missing).unwrap(), missing);

        // The refusal names the agent whose field it was.
        let relative = normalize(".codex-alt").unwrap_err();
        assert!(relative.to_string().contains("absolute"), "{relative}");
        assert!(relative.to_string().contains("Codex"), "{relative}");
    }

    /// An account always requires a non-blank directory.
    #[test]
    fn an_account_directory_may_not_be_blank() {
        assert!(normalize_account_dir(Agent::Codex, "").is_err());
        assert!(normalize_account_dir(Agent::Codex, "   ").is_err());
    }

    /// Only an archived session can be deleted, and the bulk delete takes exactly its scope: a
    /// project's archived sessions, or with no project the console's own archived console sessions.
    #[test]
    fn deleting_sessions_is_limited_to_archived_ones_in_scope() {
        let (state, dir) = crate::test_support::app_state("coordinator-delete-sessions");
        let workdir = console_workdir(&dir);
        for id in ["console-1", "console-2"] {
            let mut other = console(Agent::Claude, &workdir);
            other.id = id.to_string();
            state.store.insert_console(&other).unwrap();
        }
        state
            .store
            .insert_project(&Project {
                id: "project-1".to_string(),
                console_id: "console-1".to_string(),
                host_id: LOCAL_HOST_ID.to_string(),
                name: "Project".to_string(),
                path: "/tmp/project-1".to_string(),
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
                git_workflow: Default::default(),
            })
            .unwrap();
        let add = |id: &str, console: &str, role: Role, project: Option<&str>, status| {
            state
                .store
                .insert_session(&Session {
                    id: id.to_string(),
                    agent: Agent::Claude,
                    agent_session_id: None,
                    console_id: console.to_string(),
                    project_id: project.map(str::to_string),
                    host_id: LOCAL_HOST_ID.to_string(),
                    role,
                    origin: Origin::User,
                    title: id.to_string(),
                    status,
                    terminal_title: None,
                    record_titles: Default::default(),
                    background_tasks: Vec::new(),
                    switching_account: false,
                    default_effort: None,
                    orchestration_role: None,
                    context: None,
                    observed_model: None,
                    observed_effort: None,
                    model: None,
                    effort: None,
                    has_conversation: false,
                    bound_to: None,
                    lead: false,
                    colour: None,
                    ordinal: None,
                    account_id: None,
                    config_dir: None,
                    pinned: false,
                    locked: false,
                    started_at: 0,
                    ended_at: None,
                })
                .unwrap();
        };
        let archived = SessionStatus::Archived;
        add(
            "project-archived",
            "console-1",
            Role::Project,
            Some("project-1"),
            archived,
        );
        add(
            "project-idle",
            "console-1",
            Role::Project,
            Some("project-1"),
            SessionStatus::Idle,
        );
        add(
            "console-archived",
            "console-1",
            Role::Console,
            None,
            archived,
        );
        add(
            "other-console-archived",
            "console-2",
            Role::Console,
            None,
            archived,
        );
        let remaining = || -> Vec<String> {
            let mut ids: Vec<String> = state
                .store
                .list_sessions()
                .unwrap()
                .into_iter()
                .map(|session| session.id)
                .collect();
            ids.sort();
            ids
        };

        // A resume in flight holds the session's launch claim, which keeps it from being deleted.
        let claim = state.begin_launch("project-archived").unwrap();
        assert!(!state.delete_if_archived("project-archived").unwrap());
        drop(claim);

        delete_archived_sessions(&state, "console-1", Some("project-1"), None).unwrap();
        assert_eq!(
            remaining(),
            ["console-archived", "other-console-archived", "project-idle"]
        );

        delete_archived_sessions(&state, "console-1", None, None).unwrap();
        assert_eq!(remaining(), ["other-console-archived", "project-idle"]);
    }

    /// A project's deletion can be told to stop its sessions, but never goes ahead under a session
    /// that is being launched; with nothing in flight it removes the project and its sessions.
    #[tokio::test]
    async fn deleting_a_project_with_stop_sessions_refuses_a_launch_in_flight() {
        let (state, dir) = crate::test_support::app_state("coordinator-delete-project-stop");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        state
            .store
            .insert_project(&Project {
                id: "project-1".to_string(),
                console_id: "console-1".to_string(),
                host_id: LOCAL_HOST_ID.to_string(),
                name: "Project".to_string(),
                path: "/tmp/project-1".to_string(),
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
                git_workflow: Default::default(),
            })
            .unwrap();
        state
            .store
            .insert_session(&Session {
                id: "project".to_string(),
                agent: Agent::Claude,
                agent_session_id: None,
                console_id: "console-1".to_string(),
                project_id: Some("project-1".to_string()),
                host_id: LOCAL_HOST_ID.to_string(),
                role: Role::Project,
                origin: Origin::User,
                title: "Project".to_string(),
                status: SessionStatus::Archived,
                terminal_title: None,
                record_titles: Default::default(),
                background_tasks: Vec::new(),
                switching_account: false,
                default_effort: None,
                orchestration_role: None,
                context: None,
                observed_model: None,
                observed_effort: None,
                model: None,
                effort: None,
                has_conversation: false,
                bound_to: None,
                lead: false,
                colour: None,
                ordinal: None,
                account_id: None,
                config_dir: None,
                pinned: false,
                locked: false,
                started_at: 0,
                ended_at: None,
            })
            .unwrap();
        let delete = || RequestBody::DeleteProject {
            project: "project-1".to_string(),
            stop_sessions: true,
            trash_directory: false,
            force_remove: Vec::new(),
            inspection: None,
        };

        let claim = state.begin_launch("project").unwrap();
        let refused = handle(&state, None, delete()).await.expect_err("launching");
        let coded = refused.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::PROJECT_HAS_RUNNING_SESSIONS);
        drop(claim);

        handle(&state, None, delete()).await.unwrap();
        assert!(state.store.get_project("project-1").unwrap().is_none());
        assert!(state.store.get_session("project").unwrap().is_none());
    }

    /// Every request that removes a session's record drops what the daemon holds for it as well —
    /// its saved output, the accounts it left for a usage limit, the hand a session request raised —
    /// including the two where the record goes by cascade; and a console takes its stalled
    /// directory pass with it.
    #[tokio::test]
    async fn a_session_record_takes_what_is_held_for_it_with_it() {
        let requests = [
            RequestBody::DeleteSession {
                session: "s".to_string(),
                trash_agent_records: false,
                only_if_unused: false,
                force_remove: Vec::new(),
                keep: Vec::new(),
            },
            RequestBody::DeleteProject {
                project: "project-1".to_string(),
                stop_sessions: false,
                trash_directory: false,
                force_remove: Vec::new(),
                inspection: None,
            },
            RequestBody::DeleteConsole {
                console: "console-1".to_string(),
            },
        ];
        for request in requests {
            let (state, dir) = crate::test_support::app_state("coordinator-saved-output-deleted");
            let workdir = console_workdir(&dir);
            state
                .store
                .insert_console(&console(Agent::Claude, &workdir))
                .unwrap();
            state
                .store
                .insert_project(&Project {
                    id: "project-1".to_string(),
                    console_id: "console-1".to_string(),
                    host_id: LOCAL_HOST_ID.to_string(),
                    name: "Project".to_string(),
                    path: "/tmp/project-1".to_string(),
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
                    git_workflow: Default::default(),
                })
                .unwrap();
            state
                .store
                .insert_session(&project_session("s", Agent::Claude, SessionStatus::Idle))
                .unwrap();
            // Raised while it ran, and left behind by the archive.
            state.raise_request_hand("s").unwrap();
            state.archive_session_record("s").unwrap();
            assert!(state.has_request_hand("s"));
            let saved = dir.join("output").join("s");
            std::fs::create_dir_all(saved.parent().unwrap()).unwrap();
            std::fs::write(&saved, b"last output").unwrap();
            assert_eq!(state.saved_output("s"), Some(b"last output".to_vec()));
            state.note_left_for_limit("s", Some("account-1"));
            let stuck = Arc::new(());
            state.stalled_directory_passes.record_from(
                "console-1".to_string(),
                Arc::downgrade(&stuck),
                std::time::Instant::now() - Duration::from_secs(1),
            );
            assert!(state.stalled_directory_passes.is_stuck("console-1"));

            let label = format!("{request:?}");
            let console_goes = matches!(request, RequestBody::DeleteConsole { .. });
            handle(&state, None, request).await.unwrap();
            assert!(state.store.get_session("s").unwrap().is_none(), "{label}");
            assert!(!saved.exists(), "{label}");
            assert!(
                state
                    .recently_left_for_limit("s", Duration::from_secs(3600))
                    .is_empty(),
                "{label}"
            );
            assert!(!state.has_request_hand("s"), "{label}");
            assert_eq!(
                state.stalled_directory_passes.is_stuck("console-1"),
                !console_goes,
                "{label}"
            );
        }
    }

    /// A clone reads nothing, and a directory source reads only the directories not yet
    /// associated, keeping the URLs the reader finds.
    #[tokio::test]
    async fn remote_urls_are_read_for_unassociated_directories_of_the_directory_sources_only() {
        let (state, dir) = crate::test_support::app_state("coordinator-directory-remote-urls");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        let (taken, fresh, bare) = (dir.join("taken"), dir.join("fresh"), dir.join("bare"));
        let mut associated = project_of("console-1", "taken");
        associated.path = taken.to_string_lossy().into_owned();
        state.store.insert_project(&associated).unwrap();
        let directories = vec![taken.clone(), fresh.clone(), bare.clone()];

        let reads = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let reader = |reads: &std::sync::Arc<std::sync::Mutex<Vec<PathBuf>>>| {
            let reads = reads.clone();
            move |path: &Path| {
                reads.lock().unwrap().push(path.to_path_buf());
                path.ends_with("fresh")
                    .then(|| "https://example.com/f.git".to_string())
            }
        };

        let associated = associated_directories(&state, "console-1").await.unwrap();
        let urls = directory_remote_urls(
            &associated,
            ProjectSource::Git,
            &directories,
            reader(&reads),
        )
        .await
        .unwrap();
        assert!(urls.is_empty() && reads.lock().unwrap().is_empty());

        for source in [ProjectSource::Local, ProjectSource::Parent] {
            reads.lock().unwrap().clear();
            let urls = directory_remote_urls(&associated, source, &directories, reader(&reads))
                .await
                .unwrap();
            let mut read = reads.lock().unwrap().clone();
            read.sort();
            assert_eq!(read, vec![bare.clone(), fresh.clone()]);
            assert_eq!(
                urls,
                HashMap::from([(fresh.clone(), "https://example.com/f.git".to_string())])
            );
        }
    }

    /// An explicit default agent is kept as given; an absent one takes the one the directory is
    /// set up for, unless that agent is determined unavailable or the client asked for no
    /// detection.
    #[tokio::test]
    async fn add_project_detects_the_agent_only_when_none_is_given() {
        let (state, dir) = crate::test_support::app_state("coordinator-add-project-detect");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        let add = |name: &str, default_agent: Option<Agent>, detect_default_agent: bool| {
            let repo = dir.join(name);
            std::fs::create_dir(&repo).expect("repo");
            std::fs::write(repo.join("AGENTS.md"), "").expect("marker");
            add_project(
                &state,
                AddProjectRequest {
                    console_id: "console-1".to_string(),
                    source: ProjectSource::Local,
                    path: Some(repo.to_string_lossy().into_owned()),
                    remote_url: None,
                    name: None,
                    default_agent,
                    settings: NewProjectSettings::default(),
                    detect_default_agent,
                    tags: None,
                    description: None,
                    check_launch: true,
                    check_text_limits: true,
                },
            )
        };

        assert_eq!(
            add("auto", None, true).await.unwrap()[0].default_agent,
            Some(Agent::Codex)
        );
        assert_eq!(
            add("explicit", Some(Agent::Grok), true).await.unwrap()[0].default_agent,
            Some(Agent::Grok)
        );
        // A client that has shown the detection and was told to keep "inherit" gets none stored.
        assert_eq!(
            add("kept-auto", None, false).await.unwrap()[0].default_agent,
            None
        );
        state.set_agent_availability(vec![crate::protocol::AgentAvailability {
            agent: Agent::Codex,
            availability: Availability::Unavailable,
            default_account_dir: None,
        }]);
        assert_eq!(
            add("missing", None, true).await.unwrap()[0].default_agent,
            None
        );
    }

    /// A description is the user's words about one project, so a parent directory that adds
    /// several takes none of it, as it takes none of a name; a single project keeps it trimmed.
    #[tokio::test]
    async fn add_project_applies_a_description_to_a_single_project_only() {
        let (state, dir) = crate::test_support::app_state("coordinator-add-project-description");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        let add = |source: ProjectSource, path: &Path| {
            add_project(
                &state,
                AddProjectRequest {
                    console_id: "console-1".to_string(),
                    source,
                    path: Some(path.to_string_lossy().into_owned()),
                    remote_url: None,
                    name: None,
                    default_agent: None,
                    settings: NewProjectSettings::default(),
                    detect_default_agent: false,
                    tags: None,
                    description: Some("  Billing.  ".to_string()),
                    check_launch: true,
                    check_text_limits: true,
                },
            )
        };

        let single = dir.join("single");
        std::fs::create_dir(&single).unwrap();
        let added = add(ProjectSource::Local, &single).await.unwrap();
        assert_eq!(added[0].description.as_deref(), Some("Billing."));

        let parent = dir.join("parent");
        for name in ["one", "two"] {
            std::fs::create_dir_all(parent.join(name).join(".git")).unwrap();
        }
        let added = add(ProjectSource::Parent, &parent).await.unwrap();
        assert_eq!(added.len(), 2);
        assert!(added.iter().all(|project| project.description.is_none()));
    }

    /// A directory already associated is refused when written through a link or, where the file
    /// system folds case, in another case; another console's project does not count.
    #[tokio::test]
    async fn add_project_refuses_a_directory_spelled_another_way() {
        let (state, dir) = crate::test_support::app_state("coordinator-add-project-spellings");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        let mut other = console(Agent::Claude, &workdir);
        other.id = "console-2".to_string();
        other.name = "Other".to_string();
        state.store.insert_console(&other).unwrap();
        let add = |console_id: &str, path: &Path| {
            add_project(
                &state,
                AddProjectRequest {
                    console_id: console_id.to_string(),
                    source: ProjectSource::Local,
                    path: Some(path.to_string_lossy().into_owned()),
                    remote_url: None,
                    name: None,
                    default_agent: None,
                    settings: NewProjectSettings::default(),
                    detect_default_agent: false,
                    tags: None,
                    description: None,
                    check_launch: true,
                    check_text_limits: true,
                },
            )
        };
        let is_already_added = |err: anyhow::Error| {
            err.downcast_ref::<CodedError>().map(|coded| coded.code)
                == Some(error_code::ALL_PROJECTS_ALREADY_ADDED)
        };

        let real = dir.join("Real");
        std::fs::create_dir(&real).unwrap();
        let link = dir.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        add("console-1", &link).await.unwrap();
        assert!(is_already_added(add("console-1", &real).await.unwrap_err()));
        // The path is stored as it was given.
        assert_eq!(
            state.store.project_paths("console-1").unwrap(),
            vec![link.to_string_lossy().into_owned()]
        );
        assert_eq!(add("console-2", &real).await.unwrap().len(), 1);
    }

    /// Two directories of one parent that are the same directory become one project.
    #[tokio::test]
    async fn add_project_associates_a_linked_repository_of_a_parent_once() {
        let (state, dir) = crate::test_support::app_state("coordinator-add-project-parent-link");
        state
            .store
            .insert_console(&console(Agent::Claude, &console_workdir(&dir)))
            .unwrap();
        let parent = dir.join("parent");
        std::fs::create_dir_all(parent.join("repo").join(".git")).unwrap();
        std::os::unix::fs::symlink(parent.join("repo"), parent.join("repo-link")).unwrap();

        let added = add_project(
            &state,
            AddProjectRequest {
                console_id: "console-1".to_string(),
                source: ProjectSource::Parent,
                path: Some(parent.to_string_lossy().into_owned()),
                remote_url: None,
                name: None,
                default_agent: None,
                settings: NewProjectSettings::default(),
                detect_default_agent: false,
                tags: None,
                description: None,
                check_launch: true,
                check_text_limits: true,
            },
        )
        .await
        .unwrap();

        assert_eq!(added.len(), 1);
    }

    /// Paths whose look at the file system outlasts the deadline are compared by their text, and
    /// while that look is stuck no second thread is started for the console.
    #[tokio::test]
    async fn a_stalled_look_at_the_file_system_falls_back_to_the_paths_text() {
        let (state, _dir) = crate::test_support::app_state("coordinator-stalled-identities");
        let (release, stuck) = std::sync::mpsc::channel::<()>();
        let paths = vec!["/work/project".to_string()];
        let identities = identities_within(
            &state,
            "console-1",
            paths.clone(),
            Duration::from_millis(50),
            move |_| {
                stuck.recv().ok();
                hostfs::DirectoryIdentities::default()
            },
        )
        .await
        .unwrap();
        assert!(identities.contains(Path::new("/work/./project/")));
        assert!(!identities.contains(Path::new("/work/other")));
        assert!(state.stalled_directory_passes.is_stuck("console-1"));
        assert!(!state.stalled_directory_passes.is_stuck("console-2"));

        // A second call neither starts its build nor waits out a deadline.
        let identities =
            identities_within(&state, "console-1", paths, Duration::from_secs(60), |_| {
                panic!("a second look was started while one is stuck")
            })
            .await
            .unwrap();
        assert!(identities.contains(Path::new("/work/project")));

        // Once the stuck thread ends the console is looked at again.
        release.send(()).unwrap();
        let ended = tokio::time::timeout(crate::test_support::PATIENCE, async {
            while state.stalled_directory_passes.is_stuck("console-1") {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(ended.is_ok(), "the stalled pass was never cleared");
    }

    #[tokio::test]
    async fn adding_project_text_checks_trimmed_character_limits_before_storing_or_cloning() {
        let (state, dir) = crate::test_support::app_state("coordinator-add-project-text-limits");
        state
            .store
            .insert_console(&console(Agent::Claude, &console_workdir(&dir)))
            .unwrap();
        let repo = dir.join("repo");
        std::fs::create_dir(&repo).unwrap();
        let request = |name, description| AddProjectRequest {
            console_id: "console-1".to_string(),
            source: ProjectSource::Local,
            path: Some(repo.to_string_lossy().into_owned()),
            remote_url: None,
            name: Some(name),
            default_agent: None,
            settings: NewProjectSettings::default(),
            detect_default_agent: false,
            tags: None,
            description: Some(description),
            check_launch: true,
            check_text_limits: true,
        };
        for (field, name, description, limit) in [
            ("name", "界".repeat(101), "ok".to_string(), "100"),
            ("description", "ok".to_string(), "😀".repeat(2_001), "2000"),
        ] {
            for source in [ProjectSource::Local, ProjectSource::Git] {
                let mut given = request(name.clone(), description.clone());
                given.source = source;
                given.remote_url = Some("invalid-remote-that-must-not-be-cloned".to_string());
                let err = add_project(&state, given).await.unwrap_err();
                let coded = err.downcast_ref::<CodedError>().expect("coded refusal");
                assert_eq!(coded.code, error_code::FIELD_TOO_LONG);
                assert_eq!(coded.params["field"], field);
                assert_eq!(coded.params["limit"], limit);
                assert!(state.store.list_projects().unwrap().is_empty());
            }
        }
        let name = "界".repeat(100);
        let description = "😀".repeat(2_000);
        let added = add_project(
            &state,
            request(format!("  {name}  "), format!("\n{description}\n")),
        )
        .await
        .unwrap();
        assert_eq!(added[0].name, name);
        assert_eq!(added[0].description.as_deref(), Some(description.as_str()));
    }

    #[tokio::test]
    async fn derived_names_and_ignored_parent_text_do_not_block_association() {
        let (state, dir) = crate::test_support::app_state("coordinator-derived-project-text");
        state
            .store
            .insert_console(&console(Agent::Claude, &console_workdir(&dir)))
            .unwrap();
        let long_name = "a".repeat(101);
        let repo = dir.join(&long_name);
        std::fs::create_dir(&repo).unwrap();
        let request = AddProjectRequest {
            console_id: "console-1".to_string(),
            source: ProjectSource::Local,
            path: Some(repo.to_string_lossy().into_owned()),
            remote_url: None,
            name: None,
            default_agent: None,
            settings: NewProjectSettings::default(),
            detect_default_agent: false,
            tags: None,
            description: None,
            check_launch: true,
            check_text_limits: true,
        };
        assert_eq!(
            add_project(&state, request).await.unwrap()[0].name,
            long_name
        );
        let parent = dir.join("parent");
        std::fs::create_dir_all(parent.join("one").join(".git")).unwrap();
        let request = || AddProjectRequest {
            console_id: "console-1".to_string(),
            source: ProjectSource::Parent,
            path: Some(parent.to_string_lossy().into_owned()),
            remote_url: None,
            name: Some(long_name.clone()),
            default_agent: None,
            settings: NewProjectSettings::default(),
            detect_default_agent: false,
            tags: None,
            description: Some("x".repeat(2_001)),
            check_launch: true,
            check_text_limits: true,
        };
        let err = add_project(&state, request()).await.unwrap_err();
        assert_eq!(
            err.downcast_ref::<CodedError>().unwrap().code,
            error_code::FIELD_TOO_LONG
        );
        std::fs::create_dir_all(parent.join("two").join(".git")).unwrap();
        let added = add_project(&state, request()).await.unwrap();
        assert_eq!(added.len(), 2);
        assert!(added
            .iter()
            .all(|project| project.name != long_name && project.description.is_none()));
    }

    #[tokio::test]
    async fn project_text_updates_preserve_legacy_values_and_reject_new_overlong_text() {
        let (state, dir) = crate::test_support::app_state("coordinator-update-project-text-limits");
        state
            .store
            .insert_console(&console(Agent::Claude, &console_workdir(&dir)))
            .unwrap();
        let mut legacy = project_of("console-1", "project-1");
        legacy.name = format!("  {}  ", "n".repeat(101));
        legacy.description = Some(format!("\n{}\n", "d".repeat(2_001)));
        state.store.insert_project(&legacy).unwrap();
        let updated = update_project(
            &state,
            &legacy.id,
            ProjectChanges {
                pinned: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.name, legacy.name);
        assert_eq!(updated.description, legacy.description);
        let updated = update_project(
            &state,
            &legacy.id,
            ProjectChanges {
                name: Some(legacy.name.clone()),
                description: Some(legacy.description.clone()),
                pinned: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(!updated.pinned);
        assert_eq!(updated.name, legacy.name);
        assert_eq!(updated.description, legacy.description);
        for (field, changes) in [
            (
                "name",
                ProjectChanges {
                    name: Some("m".repeat(101)),
                    pinned: Some(true),
                    ..Default::default()
                },
            ),
            (
                "description",
                ProjectChanges {
                    description: Some(Some("e".repeat(2_001))),
                    pinned: Some(true),
                    ..Default::default()
                },
            ),
        ] {
            let err = update_project(&state, &legacy.id, changes)
                .await
                .unwrap_err();
            let coded = err.downcast_ref::<CodedError>().expect("coded refusal");
            assert_eq!(coded.code, error_code::FIELD_TOO_LONG);
            assert_eq!(coded.params["field"], field);
            let stored = state.store.get_project(&legacy.id).unwrap().unwrap();
            assert_eq!(stored.name, legacy.name);
            assert_eq!(stored.description, legacy.description);
            assert!(!stored.pinned);
        }
        let name = "😀".repeat(100);
        let description = "界".repeat(2_000);
        let updated = update_project(
            &state,
            &legacy.id,
            ProjectChanges {
                name: Some(format!(" {name} ")),
                description: Some(Some(format!("\n{description}\n"))),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.name, name);
        assert_eq!(updated.description.as_deref(), Some(description.as_str()));
    }

    /// A session whose resolved agent has been determined unavailable is refused before anything
    /// is launched or written — no record, no claim, no process.
    #[tokio::test]
    async fn opening_a_session_is_refused_when_its_resolved_agent_is_unavailable() {
        let (state, dir) =
            crate::test_support::app_state("coordinator-open-session-agent-unavailable");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        state.set_agent_availability(
            [Agent::Claude, Agent::Codex]
                .into_iter()
                .map(|agent| crate::protocol::AgentAvailability {
                    agent,
                    availability: Availability::Unavailable,
                    default_account_dir: None,
                })
                .collect(),
        );
        for (chosen, expected) in [(None, Agent::Claude), (Some(Agent::Codex), Agent::Codex)] {
            let err = open_session(
                &state,
                OpenRequest {
                    console_id: "console-1".to_string(),
                    project_id: None,
                    agent: chosen,
                    account: None,
                    task: None,
                    title: None,
                    model: None,
                    effort: None,
                    origin: Origin::User,
                    bound_to: None,
                    resume_agent_session_id: None,
                    session_id: None,
                    terminal_appearance: None,
                },
            )
            .await
            .expect_err("refused");
            let coded = err.downcast_ref::<CodedError>().expect("a coded error");
            assert_eq!(coded.code, error_code::AGENT_NOT_AVAILABLE);
            assert_eq!(coded.params["agent"], expected.label());
            assert!(state.store.list_sessions().unwrap().is_empty());
        }
    }

    /// An account whose held quota reading has a window used up is refused with the window and
    /// its reset time when known, for the default account and a stored one alike; an account with
    /// no reading is not.
    #[test]
    fn opening_a_session_is_refused_on_an_account_whose_quota_is_used_up() {
        use crate::protocol::{AccountQuota, Params, QuotaWindow, QuotaWindowKind};

        let (state, dir) = crate::test_support::app_state("coordinator-open-quota-exhausted");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        state
            .store
            .insert_account(&Account {
                id: "claude-acct".to_string(),
                agent: Agent::Claude,
                name: "Claude work".to_string(),
                config_dir: "/home/u/.claude-alt".to_string(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
        let reset = now_millis() + 60_000;
        let used_up = |account: Option<&str>, resets_at| AccountQuota {
            agent: Agent::Claude,
            account: account.map(str::to_string),
            fetched_at: 1,
            plan: None,
            email: None,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Weekly,
                label: None,
                used_percent: 100.0,
                resets_at,
                window_minutes: None,
            }],
            unavailable: None,
        };
        let request = |account: Option<String>| OpenRequest {
            console_id: "console-1".to_string(),
            project_id: None,
            agent: None,
            account: Some(account),
            task: None,
            title: None,
            model: None,
            effort: None,
            origin: Origin::User,
            bound_to: None,
            resume_agent_session_id: None,
            session_id: None,
            terminal_appearance: None,
        };
        let refusal = |account: Option<&str>| {
            resolve_open(&state, &request(account.map(str::to_string))).err()
        };
        let coded = |err: &anyhow::Error| -> (&'static str, Params) {
            let coded = err.downcast_ref::<CodedError>().expect("a coded error");
            (coded.code, coded.params.clone())
        };

        // Nothing read yet: usable.
        assert!(refusal(Some("claude-acct")).is_none());
        assert!(refusal(None).is_none());

        state.account_quotas.hold_in_dir_for_test(
            &used_up(Some("claude-acct"), Some(reset)),
            "/home/u/.claude-alt",
        );
        let (code, params) = coded(&refusal(Some("claude-acct")).expect("refused"));
        assert_eq!(code, error_code::ACCOUNT_QUOTA_EXHAUSTED);
        assert_eq!(params["agent"], Agent::Claude.label());
        assert_eq!(params["account"], "Claude work");
        assert_eq!(params["window"], "Weekly");
        assert_eq!(params["window_kind"], "weekly");
        assert!(!params.contains_key("window_minutes"));
        assert!(!params.contains_key("model_weekly"));
        assert_eq!(params["resets_at"], reset.to_string());

        // The default account, whose reading does not say when it resets.
        state.account_quotas.hold_for_test(&used_up(None, None));
        let (code, params) = coded(&refusal(None).expect("refused"));
        assert_eq!(code, error_code::ACCOUNT_QUOTA_EXHAUSTED);
        assert_eq!(params["account"], crate::protocol::DEFAULT_ACCOUNT_NAME);
        assert!(!params.contains_key("resets_at"));

        // An account the reading found not logged in is refused too, the default one included.
        let signed_out = |account: Option<&str>| AccountQuota {
            windows: Vec::new(),
            unavailable: Some(crate::protocol::QuotaUnavailable::new(
                crate::protocol::quota_unavailable_code::NOT_SIGNED_IN,
                "x",
                &[("agent", "Claude Code")],
            )),
            ..used_up(account, None)
        };
        state
            .account_quotas
            .hold_in_dir_for_test(&signed_out(Some("claude-acct")), "/home/u/.claude-alt");
        state.account_quotas.hold_for_test(&signed_out(None));
        for account in [Some("claude-acct"), None] {
            let (code, params) = coded(&refusal(account).expect("refused"));
            assert_eq!(code, error_code::ACCOUNT_NOT_SIGNED_IN);
            assert_eq!(params["agent"], Agent::Claude.label());
        }
    }

    /// A disabled account, the default one included, is refused for a new session whether the
    /// request names it or the console inherits it, is passed over by automatic selection and by
    /// `account_can_open`, and turning it back on undoes all of that. A session already on the
    /// account keeps it.
    #[tokio::test]
    async fn a_disabled_account_is_skipped_and_refused_for_new_sessions_but_not_for_running_ones() {
        let (state, _dir) = console_with_project_and_accounts("coordinator-account-disabled");
        let alt_dir = state
            .store
            .account_dir(&Some("claude-a".to_string()))
            .unwrap()
            .unwrap();
        state
            .store
            .insert_session(&project_session("s", Agent::Claude, SessionStatus::Working))
            .unwrap();
        state
            .store
            .set_session_account("s", Some("claude-a"), Some(&alt_dir))
            .unwrap();
        let request = |account: Option<Option<String>>| OpenRequest {
            console_id: "console-1".to_string(),
            project_id: None,
            agent: None,
            account,
            task: None,
            title: None,
            model: None,
            effort: None,
            origin: Origin::User,
            bound_to: None,
            resume_agent_session_id: None,
            session_id: None,
            terminal_appearance: None,
        };
        let refused = |account: Option<Option<String>>| {
            resolve_open(&state, &request(account)).err().map(|err| {
                let coded = err.downcast_ref::<CodedError>().expect("a coded error");
                (coded.code, coded.params.clone())
            })
        };
        assert!(refused(Some(Some("claude-a".to_string()))).is_none());

        // An id that is no account of the agent cannot be turned off.
        let err = set_account_disabled(&state, Agent::Codex, Some("claude-a"), true)
            .expect_err("another agent's account");
        assert_eq!(code_of(&err), error_code::UNKNOWN_ACCOUNT);

        set_account_disabled(&state, Agent::Claude, Some("claude-a"), true).unwrap();
        set_account_disabled(&state, Agent::Claude, None, true).unwrap();
        let settings = state.store.get_settings().unwrap();
        assert!(settings.accounts[0].disabled);
        assert!(settings.disabled_default_accounts.contains("claude"));

        for account in [Some(Some("claude-a".to_string())), Some(None)] {
            let (code, params) = refused(account).expect("refused");
            assert_eq!(code, error_code::ACCOUNT_DISABLED);
            assert_eq!(params["agent"], Agent::Claude.label());
        }
        // Inherited from the console as well as named.
        assert_eq!(refused(None).unwrap().0, error_code::ACCOUNT_DISABLED);
        assert!(!account_can_open(&state, Agent::Claude, Some("claude-a")).unwrap());
        assert!(!account_can_open(&state, Agent::Claude, None).unwrap());
        // Another agent's default account is unaffected.
        assert!(account_can_open(&state, Agent::Codex, None).unwrap());

        // Automatic selection passes over both, even with a reading that ranks them.
        let weekly = |account: Option<&str>| crate::protocol::AccountQuota {
            agent: Agent::Claude,
            account: account.map(str::to_string),
            fetched_at: 1,
            plan: None,
            email: None,
            windows: vec![crate::protocol::QuotaWindow {
                kind: crate::protocol::QuotaWindowKind::Weekly,
                label: None,
                used_percent: 10.0,
                resets_at: Some(now_millis() + 3_600_000),
                window_minutes: None,
            }],
            unavailable: None,
        };
        state
            .account_quotas
            .hold_in_dir_for_test(&weekly(Some("claude-a")), &alt_dir);
        state.account_quotas.hold_for_test(&weekly(None));
        assert_eq!(
            preferred_weekly_account(&state, Agent::Claude).unwrap(),
            None
        );

        // The running session keeps its account.
        let running = state.store.get_session("s").unwrap().unwrap();
        assert_eq!(running.account_id.as_deref(), Some("claude-a"));
        assert_eq!(running.status, SessionStatus::Working);

        // Turning one back on lets it be picked again; the other stays off.
        set_account_disabled(&state, Agent::Claude, Some("claude-a"), false).unwrap();
        assert!(refused(Some(Some("claude-a".to_string()))).is_none());
        assert_eq!(
            preferred_weekly_account(&state, Agent::Claude).unwrap(),
            Some(Some("claude-a".to_string()))
        );
        set_account_disabled(&state, Agent::Claude, None, false).unwrap();
        assert!(state
            .store
            .get_settings()
            .unwrap()
            .disabled_default_accounts
            .is_empty());
    }

    /// Turning an account off or on broadcasts the settings only when the flag really changed, and
    /// an id that is no account of the agent is refused.
    #[tokio::test]
    async fn turning_an_account_off_broadcasts_only_a_real_change() {
        let (state, _dir) = console_with_project_and_accounts("coordinator-disabled-broadcast");
        let mut events = state.subscribe();
        let toggle = |agent, account: Option<&str>, disabled| RequestBody::SetAccountDisabled {
            agent,
            account: account.map(str::to_string),
            disabled,
        };
        let flags = |event: Event| match event {
            Event::SettingsUpdated { settings } => (
                settings.accounts.iter().any(|account| account.disabled),
                settings.disabled_default_accounts.len(),
            ),
            other => panic!("unexpected {other:?}"),
        };

        handle(&state, None, toggle(Agent::Claude, Some("claude-a"), true))
            .await
            .unwrap();
        assert_eq!(flags(events.try_recv().unwrap()), (true, 0));
        handle(&state, None, toggle(Agent::Claude, Some("claude-a"), true))
            .await
            .unwrap();
        assert!(events.try_recv().is_err(), "a repeat changes nothing");

        handle(&state, None, toggle(Agent::Claude, None, true))
            .await
            .unwrap();
        assert_eq!(flags(events.try_recv().unwrap()), (true, 1));
        handle(&state, None, toggle(Agent::Claude, None, true))
            .await
            .unwrap();
        assert!(events.try_recv().is_err());

        for (agent, account) in [(Agent::Claude, "nope"), (Agent::Codex, "claude-a")] {
            let err = handle(&state, None, toggle(agent, Some(account), true))
                .await
                .expect_err("not an account of the agent");
            assert_eq!(code_of(&err), error_code::UNKNOWN_ACCOUNT);
        }
        assert!(events.try_recv().is_err());
        assert!(
            state
                .store
                .get_account("claude-a")
                .unwrap()
                .unwrap()
                .disabled
        );
    }

    /// Turning an account back on starts the usage window its held reading shows unstarted, when
    /// the setting for that is on; turning it off starts nothing.
    #[tokio::test]
    async fn turning_an_account_back_on_kicks_its_held_window() {
        use crate::protocol::{AccountQuota, QuotaWindow, QuotaWindowKind};

        let (state, _dir) = console_with_project_and_accounts("coordinator-disabled-kick");
        let unstarted = AccountQuota {
            agent: Agent::Claude,
            account: Some("claude-a".to_string()),
            fetched_at: 1,
            plan: None,
            email: None,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Session,
                label: None,
                used_percent: 0.0,
                resets_at: None,
                window_minutes: Some(300),
            }],
            unavailable: None,
        };
        let dir = state
            .store
            .account_dir(&Some("claude-a".to_string()))
            .unwrap()
            .unwrap();
        state.account_quotas.hold_in_dir_for_test(&unstarted, &dir);
        state.store.set_auto_start_quota_windows(true).unwrap();
        let kicked = || {
            state
                .account_quotas
                .is_kicked_for_test(Agent::Claude, Some("claude-a"))
        };

        set_account_disabled(&state, Agent::Claude, Some("claude-a"), true).unwrap();
        assert!(!kicked(), "turning it off starts nothing");

        set_account_disabled(&state, Agent::Claude, Some("claude-a"), false).unwrap();
        let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
        while !kicked() && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(kicked(), "turning it back on counts its unstarted window");
    }

    /// A switch onto a disabled account is refused before anything is recorded or stopped.
    #[tokio::test]
    async fn a_switch_onto_a_disabled_account_is_refused() {
        let (state, dir) = console_with_project_and_accounts("coordinator-switch-disabled");
        state
            .store
            .insert_session(&project_session(
                "s",
                Agent::Claude,
                SessionStatus::Interrupted,
            ))
            .unwrap();
        set_account_disabled(&state, Agent::Claude, Some("claude-a"), true).unwrap();

        let err = switch_session_account(
            &state,
            "s",
            Some(Some("claude-a".to_string())),
            None,
            shell_env(&dir),
        )
        .await
        .expect_err("refused");
        assert_eq!(code_of(&err), error_code::ACCOUNT_DISABLED);
        let after = state.store.get_session("s").unwrap().unwrap();
        assert_eq!(after.account_id, None);
        assert_eq!(after.status, SessionStatus::Interrupted);
    }

    /// An account the last quota reading found not logged in is refused for a switch and passed
    /// over by `account_can_open` and by the choice of a target, the signed-out reasons both
    /// included; a reading that failed for another reason refuses and passes over nothing there
    /// (the choice of a target for the automatic switch alone asks for a readable one).
    #[tokio::test]
    async fn an_account_found_not_logged_in_is_refused_and_a_failed_reading_is_not() {
        use crate::protocol::{
            quota_unavailable_code as code, AccountQuota, QuotaUnavailable, QuotaWindow,
            QuotaWindowKind,
        };

        let (state, dir) = console_with_project_and_accounts("coordinator-switch-signed-out");
        state
            .store
            .insert_session(&project_session(
                "s",
                Agent::Claude,
                SessionStatus::Interrupted,
            ))
            .unwrap();
        let alt_dir = state
            .store
            .account_dir(&Some("claude-a".to_string()))
            .unwrap()
            .unwrap();
        let reading = |unavailable: Option<QuotaUnavailable>| AccountQuota {
            agent: Agent::Claude,
            account: Some("claude-a".to_string()),
            fetched_at: 1,
            plan: None,
            email: None,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Weekly,
                label: None,
                used_percent: 10.0,
                resets_at: Some(now_millis() + 60_000),
                window_minutes: None,
            }],
            unavailable,
        };
        let switch = || {
            switch_session_account(
                &state,
                "s",
                Some(Some("claude-a".to_string())),
                None,
                shell_env(&dir),
            )
        };

        for reason in [code::NOT_SIGNED_IN, code::CONFIG_DIR_MISSING] {
            let held = reading(Some(QuotaUnavailable::new(
                reason,
                "x",
                &[("agent", "Claude")],
            )));
            state.account_quotas.hold_in_dir_for_test(&held, &alt_dir);
            let err = switch().await.expect_err("refused");
            assert_eq!(code_of(&err), error_code::ACCOUNT_NOT_SIGNED_IN, "{reason}");
            let coded = err.downcast_ref::<CodedError>().unwrap();
            assert_eq!(coded.params["agent"], Agent::Claude.label());
            assert!(!account_can_open(&state, Agent::Claude, Some("claude-a")).unwrap());
            assert_eq!(
                preferred_weekly_account(&state, Agent::Claude).unwrap(),
                None,
                "{reason}: not picked for a new session either"
            );
            assert_eq!(
                state.store.get_session("s").unwrap().unwrap().account_id,
                None
            );
        }

        // A reading that failed for a passing reason still counts for what it holds.
        let failed = reading(Some(QuotaUnavailable::new(
            code::COMMAND_FAILED,
            "x",
            &[("agent", "Claude"), ("detail", "network")],
        )));
        state.account_quotas.hold_in_dir_for_test(&failed, &alt_dir);
        assert!(account_can_open(&state, Agent::Claude, Some("claude-a")).unwrap());
        assert_eq!(
            preferred_weekly_account(&state, Agent::Claude).unwrap(),
            Some(Some("claude-a".to_string()))
        );
        assert_eq!(
            preferred_weekly_account_except(&state, Agent::Claude, &[], true).unwrap(),
            None,
            "the automatic switch wants an account it can read now"
        );
        let err = switch()
            .await
            .expect_err("fails further on, for want of a process");
        assert_ne!(code_of(&err), error_code::ACCOUNT_NOT_SIGNED_IN);
    }

    #[test]
    fn automatic_open_choice_uses_another_agent_only_after_all_preferred_accounts() {
        let (state, _dir) = state_for_opening("automatic-open-agent-fallback");
        state.store.set_auto_select_account(true).unwrap();
        state
            .store
            .set_added_agents_if_undecided(&[Agent::Claude, Agent::Grok, Agent::Codex])
            .unwrap();
        set_account_disabled(&state, Agent::Claude, None, true).unwrap();
        let resolved = resolve_open(&state, &open_request()).unwrap();
        assert_eq!(resolved.agent, Agent::Grok);
        assert_eq!(resolved.account_id, None);

        let explicit = OpenRequest {
            agent: Some(Agent::Claude),
            ..open_request()
        };
        assert_eq!(
            code_of(&resolve_open(&state, &explicit).err().unwrap()),
            error_code::ACCOUNT_DISABLED
        );
        let explicit = OpenRequest {
            account: Some(None),
            ..open_request()
        };
        assert_eq!(
            code_of(&resolve_open(&state, &explicit).err().unwrap()),
            error_code::ACCOUNT_DISABLED
        );

        state
            .store
            .insert_account(&Account {
                id: "claude-unread".to_string(),
                agent: Agent::Claude,
                name: "Unread".to_string(),
                config_dir: "/tmp/claude-unread".to_string(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
        let resolved = resolve_open(&state, &open_request()).unwrap();
        assert_eq!(resolved.agent, Agent::Claude);
        assert_eq!(resolved.account_id.as_deref(), Some("claude-unread"));
        let account = state.store.get_account("claude-unread").unwrap().unwrap();
        let (_, lock) = state.lock_account_sharing(&account, |_, _| Ok(())).unwrap();
        assert_eq!(
            resolve_open(&state, &open_request()).unwrap().agent,
            Agent::Grok
        );
        drop(lock);
        state.store.set_auto_select_account(false).unwrap();
        assert_eq!(
            code_of(&resolve_open(&state, &open_request()).err().unwrap()),
            error_code::ACCOUNT_DISABLED
        );
    }

    #[test]
    fn automatic_open_choice_uses_saved_account_order_for_unranked_accounts() {
        let (state, _dir) = state_for_opening("automatic-open-account-order");
        state.store.set_auto_select_account(true).unwrap();
        set_account_disabled(&state, Agent::Claude, None, true).unwrap();
        for id in ["first", "second"] {
            state
                .store
                .insert_account(&Account {
                    id: id.to_string(),
                    agent: Agent::Claude,
                    name: id.to_string(),
                    config_dir: format!("/tmp/{id}"),
                    share_default_config: false,
                    share_default_config_incomplete: false,
                    disabled: false,
                })
                .unwrap();
        }
        state
            .store
            .reorder_accounts(
                Agent::Claude,
                &[Some("second".to_string()), None, Some("first".to_string())],
            )
            .unwrap();
        assert_eq!(
            resolve_open(&state, &open_request())
                .unwrap()
                .account_id
                .as_deref(),
            Some("second")
        );
    }

    #[test]
    fn automatic_open_choice_keeps_a_weekly_preference_within_its_agent() {
        use crate::protocol::{AccountQuota, QuotaWindow, QuotaWindowKind};
        let (state, _dir) = state_for_opening("automatic-open-weekly");
        state.store.set_auto_select_account(true).unwrap();
        state
            .store
            .insert_account(&Account {
                id: "claude-weekly".to_string(),
                agent: Agent::Claude,
                name: "Weekly".to_string(),
                config_dir: "/tmp/claude-weekly".to_string(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
        state.account_quotas.hold_in_dir_for_test(
            &AccountQuota {
                agent: Agent::Claude,
                account: Some("claude-weekly".to_string()),
                fetched_at: 1,
                plan: None,
                email: None,
                unavailable: None,
                windows: vec![QuotaWindow {
                    kind: QuotaWindowKind::Weekly,
                    label: None,
                    used_percent: 70.0,
                    resets_at: Some(now_millis() + 60_000),
                    window_minutes: None,
                }],
            },
            "/tmp/claude-weekly",
        );
        assert_eq!(
            resolve_open(&state, &open_request())
                .unwrap()
                .account_id
                .as_deref(),
            Some("claude-weekly")
        );
        assert_eq!(
            resolve_open(
                &state,
                &OpenRequest {
                    account: Some(None),
                    ..open_request()
                }
            )
            .unwrap()
            .account_id,
            None
        );
    }

    /// Holds a reading of a Claude account whose session window is used up, resetting in
    /// `resets_in` milliseconds (`None` no reset time), as `signed_out` or not.
    fn hold_used_up(
        state: &Arc<AppState>,
        account: Option<&str>,
        resets_in: Option<i64>,
        signed_out: bool,
    ) {
        use crate::protocol::{AccountQuota, QuotaUnavailable, QuotaWindow, QuotaWindowKind};
        let quota = AccountQuota {
            agent: Agent::Claude,
            account: account.map(str::to_string),
            fetched_at: 1,
            plan: None,
            email: None,
            unavailable: signed_out.then(|| {
                QuotaUnavailable::new(
                    crate::protocol::quota_unavailable_code::NOT_SIGNED_IN,
                    "not signed in",
                    &[],
                )
            }),
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Session,
                label: None,
                used_percent: 100.0,
                resets_at: resets_in.map(|span| now_millis() + span),
                window_minutes: None,
            }],
        };
        match account {
            None => state.account_quotas.hold_for_test(&quota),
            Some(id) => state
                .account_quotas
                .hold_in_dir_for_test(&quota, &format!("/tmp/{id}")),
        }
    }

    fn insert_claude_account(state: &Arc<AppState>, id: &str, disabled: bool) {
        state
            .store
            .insert_account(&Account {
                id: id.to_string(),
                agent: Agent::Claude,
                name: id.to_string(),
                config_dir: format!("/tmp/{id}"),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled,
            })
            .unwrap();
    }

    /// With automatic selection on and no usable account, the session opens on the account of the
    /// preferred agent that becomes usable soonest, so the refusal names it; an account that is
    /// turned off or logged out is not chosen, a missing reset time counts as never, and an equal
    /// time keeps the default account.
    #[test]
    fn automatic_open_without_a_usable_account_is_refused_on_the_soonest_one() {
        let (state, _dir) = state_for_opening("automatic-open-soonest");
        state.store.set_auto_select_account(true).unwrap();
        state
            .store
            .set_added_agents_if_undecided(&[Agent::Claude])
            .unwrap();
        for (id, disabled) in [
            ("later", false),
            ("soonest", false),
            ("off", true),
            ("out", false),
        ] {
            insert_claude_account(&state, id, disabled);
        }
        let hour = 3_600_000;
        hold_used_up(&state, None, Some(5 * hour), false);
        hold_used_up(&state, Some("later"), Some(3 * hour), false);
        hold_used_up(&state, Some("soonest"), Some(2 * hour), false);
        hold_used_up(&state, Some("off"), Some(hour), false);
        hold_used_up(&state, Some("out"), Some(hour / 2), true);

        let refused = resolve_open(&state, &open_request()).err().unwrap();
        let coded = refused.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::ACCOUNT_QUOTA_EXHAUSTED);
        assert!(coded.message.starts_with("soonest "), "{}", coded.message);
        assert_eq!(
            soonest_available_account(&state, Agent::Claude).unwrap(),
            Some(Some("soonest".to_string()))
        );

        // A reset time that is missing counts as never; equal times keep the earlier candidate.
        hold_used_up(&state, Some("soonest"), None, false);
        let mut tied = state
            .account_quotas
            .held(Agent::Claude, None, None)
            .unwrap();
        tied.account = Some("later".to_string());
        state
            .account_quotas
            .hold_in_dir_for_test(&tied, "/tmp/later");
        assert_eq!(
            soonest_available_account(&state, Agent::Claude).unwrap(),
            Some(None)
        );
        assert_eq!(
            resolve_open_with(&state, &open_request(), false)
                .unwrap()
                .account_id,
            None
        );

        // A usable account of the agent, or automatic selection off, leaves it as it was.
        insert_claude_account(&state, "fresh", false);
        assert_eq!(
            soonest_available_account(&state, Agent::Claude).unwrap(),
            None
        );
        assert_eq!(
            resolve_open(&state, &open_request())
                .unwrap()
                .account_id
                .as_deref(),
            Some("fresh")
        );
        state.store.delete_account("fresh").unwrap();
        state.store.set_auto_select_account(false).unwrap();
        let refused = resolve_open(&state, &open_request()).err().unwrap();
        assert!(
            refused
                .downcast_ref::<CodedError>()
                .unwrap()
                .message
                .starts_with("Default "),
            "{refused}"
        );
    }

    /// A session is opened only on an added agent, wherever the agent comes from: the request's
    /// own choice or the console's default. An undecided list refuses nothing.
    #[tokio::test]
    async fn opening_a_session_is_refused_when_its_resolved_agent_is_not_added() {
        let (state, _dir) = state_for_opening("coordinator-open-session-agent-not-added");
        let refused = |err: anyhow::Error, agent: Agent| {
            let coded = err.downcast_ref::<CodedError>().expect("a coded error");
            assert_eq!(coded.code, error_code::AGENT_NOT_ADDED);
            assert_eq!(coded.params["agent"], agent.label());
            assert!(coded.message.contains("Settings → Agent accounts"));
        };

        assert!(check_open(&state, &open_request()).await.is_ok());
        state
            .store
            .set_added_agents_if_undecided(&[Agent::Codex])
            .unwrap();

        // The console's default agent, Claude, is not added.
        refused(
            check_open(&state, &open_request()).await.unwrap_err(),
            Agent::Claude,
        );
        let err = open_session(&state, open_request()).await.unwrap_err();
        refused(err, Agent::Claude);
        let err = open_session_detached(
            &state,
            OpenRequest {
                agent: Some(Agent::Grok),
                ..open_request()
            },
        )
        .await
        .map(|_| ())
        .unwrap_err();
        refused(err, Agent::Grok);
        assert!(state.store.list_sessions().unwrap().is_empty());

        let codex = OpenRequest {
            agent: Some(Agent::Codex),
            ..open_request()
        };
        assert_eq!(check_open(&state, &codex).await.unwrap(), Agent::Codex);
    }

    /// A list emptied on purpose is decided, so it refuses every agent.
    #[tokio::test]
    async fn an_empty_added_list_refuses_every_agent() {
        let (state, _dir) = state_for_opening("coordinator-open-session-nothing-added");
        state.store.set_added_agents_if_undecided(&[]).unwrap();
        for agent in Agent::ALL {
            let err = check_open(
                &state,
                &OpenRequest {
                    agent: Some(agent),
                    ..open_request()
                },
            )
            .await
            .unwrap_err();
            let coded = err.downcast_ref::<CodedError>().expect("a coded error");
            assert_eq!(coded.code, error_code::AGENT_NOT_ADDED);
        }
    }

    /// Reopening a session of an agent that has since been removed is not refused for that: its
    /// settings and its place stay, so it fails here only for its own reason.
    #[tokio::test]
    async fn a_reopen_is_not_refused_for_an_agent_that_is_not_added() {
        let (state, dir) = console_with_project_and_accounts("coordinator-reopen-agent-removed");
        state
            .store
            .set_added_agents_if_undecided(&[Agent::Codex])
            .unwrap();
        state
            .store
            .insert_session(&Session {
                ended_at: Some(5),
                ..project_session("s", Agent::Claude, SessionStatus::Archived)
            })
            .unwrap();

        let err = resume_session_with(&state, "s", None, None, stub_shell(&dir))
            .await
            .expect_err("the project's directory does not exist");
        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
    }

    /// `~` is the home directory of the host the daemon runs on, like every other path it takes.
    #[test]
    fn a_leading_tilde_in_a_config_dir_is_expanded() {
        // `$HOME` always exists, so this needs no fixture and no environment edit.
        let home = paths::home_dir();
        assert_eq!(
            normalize_account_dir(Agent::Grok, "~").unwrap(),
            home.to_string_lossy().into_owned()
        );
    }

    #[test]
    fn a_console_icon_is_an_image_data_url_of_bounded_size() {
        assert_eq!(normalize_icon(None).unwrap(), None);
        assert_eq!(normalize_icon(Some("  ")).unwrap(), None);
        assert_eq!(
            normalize_icon(Some(" data:image/webp;base64,AAAA ")).unwrap(),
            Some("data:image/webp;base64,AAAA".to_string())
        );
        assert!(normalize_icon(Some("https://example.com/a.png")).is_err());
        assert!(normalize_icon(Some("data:text/html;base64,AAAA")).is_err());
        let huge = format!("data:image/png;base64,{}", "A".repeat(MAX_ICON_BYTES));
        assert!(normalize_icon(Some(&huge)).is_err());
    }

    /// A session's account is its own pick, else the project's reference for its agent, else the
    /// console's, else the default account: a project's reference overrides the console's, and one
    /// left unset falls back to it, agent by agent. Another agent's account is never borrowed.
    #[test]
    fn a_sessions_account_is_its_own_pick_else_its_projects_else_its_consoles() {
        let (state, _dir) = crate::test_support::app_state("coordinator-session-account");
        for (id, agent, name, dir) in [
            (
                "claude-acct",
                Agent::Claude,
                "Claude work",
                "/home/u/.claude-alt",
            ),
            (
                "claude-other",
                Agent::Claude,
                "Claude other",
                "/home/u/.claude-other",
            ),
            (
                "claude-project",
                Agent::Claude,
                "Claude project",
                "/home/u/.claude-project",
            ),
            (
                "codex-acct",
                Agent::Codex,
                "Codex work",
                "/home/u/.codex-alt",
            ),
            (
                "grok-project",
                Agent::Grok,
                "Grok project",
                "/home/u/.grok-project",
            ),
        ] {
            state
                .store
                .insert_account(&Account {
                    id: id.to_string(),
                    agent,
                    name: name.to_string(),
                    config_dir: dir.to_string(),
                    share_default_config: false,
                    share_default_config_incomplete: false,
                    disabled: false,
                })
                .unwrap();
        }
        let mut console = console(Agent::Claude, Path::new("/tmp/unused"));
        console.claude_account_id = Some("claude-acct".to_string());
        console.codex_account_id = Some("codex-acct".to_string());

        let resolved = |agent: Agent, project: Option<&Project>, chosen: Option<Option<&str>>| {
            session_account(
                &state,
                agent,
                &console,
                project,
                chosen.map(|choice| choice.map(str::to_string)),
            )
            .unwrap()
        };
        let account = |id: &str, dir: &str| (Some(id.to_string()), Some(dir.to_string()));
        let project = |claude: ProjectAccount, grok: ProjectAccount| {
            let mut project = project_of("console-1", "project-1");
            project.claude_account_id = claude;
            project.grok_account_id = grok;
            Some(project)
        };
        let claude_acct = || account("claude-acct", "/home/u/.claude-alt");
        let claude_other = || account("claude-other", "/home/u/.claude-other");
        let claude_project = || account("claude-project", "/home/u/.claude-project");
        let codex_acct = || account("codex-acct", "/home/u/.codex-alt");

        // (agent, the session's project, the choice made for the session, what it launches under)
        let cases = [
            // No project and no choice: the console's account for the session's own agent.
            (Agent::Claude, None, None, claude_acct()),
            (Agent::Codex, None, None, codex_acct()),
            // Nothing referenced for Grok, and another agent's account is never borrowed for it:
            // the default account, which pins nothing.
            (Agent::Grok, None, None, (None, None)),
            // A choice wins over the console's reference...
            (
                Agent::Claude,
                None,
                Some(Some("claude-other")),
                claude_other(),
            ),
            // ...and a choice of the default account wins over it too.
            (Agent::Claude, None, Some(None), (None, None)),
            // The project's reference wins over the console's...
            (
                Agent::Claude,
                project(pins("claude-project"), pins("grok-project")),
                None,
                claude_project(),
            ),
            // ...a project that sets nothing for the agent falls back to the console's...
            (
                Agent::Codex,
                project(pins("claude-project"), pins("grok-project")),
                None,
                codex_acct(),
            ),
            // ...and to the default account when the console sets nothing either; the project's
            // reference for another agent is never borrowed.
            (
                Agent::Grok,
                project(pins("claude-project"), pins("grok-project")),
                None,
                account("grok-project", "/home/u/.grok-project"),
            ),
            (
                Agent::Grok,
                project(pins("claude-project"), ProjectAccount::Inherit),
                None,
                (None, None),
            ),
            // A project that says the default account ends the chain there, where the console would
            // have named an account; the agents it says nothing for still inherit.
            (
                Agent::Claude,
                project(ProjectAccount::Default, ProjectAccount::Inherit),
                None,
                (None, None),
            ),
            (
                Agent::Codex,
                project(ProjectAccount::Default, ProjectAccount::Inherit),
                None,
                codex_acct(),
            ),
            // The session's own pick wins over the project's reference, the default account
            // included, and over a project that says the default account.
            (
                Agent::Claude,
                project(ProjectAccount::Default, ProjectAccount::Inherit),
                Some(Some("claude-other")),
                claude_other(),
            ),
            (
                Agent::Claude,
                project(pins("claude-project"), ProjectAccount::Inherit),
                Some(Some("claude-other")),
                claude_other(),
            ),
            (
                Agent::Claude,
                project(pins("claude-project"), ProjectAccount::Inherit),
                Some(None),
                (None, None),
            ),
        ];
        for (index, (agent, project, chosen, expected)) in cases.into_iter().enumerate() {
            assert_eq!(
                resolved(agent, project.as_ref(), chosen),
                expected,
                "case {index}: {agent:?} {chosen:?}"
            );
        }

        // A choice naming no account, or an account of another agent, is refused.
        for chosen in ["missing", "codex-acct"] {
            let err = session_account(
                &state,
                Agent::Claude,
                &console,
                None,
                Some(Some(chosen.to_string())),
            )
            .expect_err(chosen);
            let coded = err.downcast_ref::<CodedError>().expect("a coded error");
            assert_eq!(coded.code, error_code::UNKNOWN_ACCOUNT, "{chosen}");
        }
    }

    /// A project with nothing of its own set for the session's agent, scratch-built for the tests
    /// that read its settings.
    fn project_of(console_id: &str, id: &str) -> Project {
        Project {
            id: id.to_string(),
            console_id: console_id.to_string(),
            host_id: LOCAL_HOST_ID.to_string(),
            name: id.to_string(),
            path: format!("/tmp/{id}"),
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
            git_workflow: Default::default(),
        }
    }

    /// A project's setting naming the account `id`.
    fn pins(id: &str) -> ProjectAccount {
        ProjectAccount::Account { id: id.to_string() }
    }

    /// A project's path is stored absolute and written one way, so the trusted directories can be
    /// compared against it; a relative one is refused.
    #[test]
    fn a_project_path_is_stored_absolute_and_normalised() {
        assert_eq!(
            absolute_path("/work/a/../project/./").unwrap(),
            PathBuf::from("/work/project")
        );
        assert_eq!(
            absolute_path("~/code/app/").unwrap(),
            paths::home_dir().join("code/app")
        );
        let err = absolute_path("work/project").expect_err("relative");
        assert!(err.to_string().contains("not an absolute path"), "{err}");
        assert!(absolute_path("").is_err());
    }

    /// A console session is never bound, whatever the request named — `resolve_bound_to` does not
    /// even look at `requested` for one.
    #[test]
    fn a_console_session_is_never_bound() {
        let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-console");
        let bound = resolve_bound_to(
            &state,
            "console-1",
            None,
            Role::Console,
            Some("anything".into()),
        )
        .expect("resolved");
        assert_eq!(bound, None);
    }

    /// With nothing named, a project session opens unbound — the ordinary case for one the user
    /// starts by hand without checking a box.
    #[test]
    fn a_project_session_named_nothing_to_bind_to_opens_unbound() {
        let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-none");
        let bound =
            resolve_bound_to(&state, "console-1", None, Role::Project, None).expect("resolved");
        assert_eq!(bound, None);
    }

    /// Naming a real console session of the same console is honoured.
    #[test]
    fn a_project_session_binds_to_the_console_session_it_names() {
        let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-valid");
        state
            .store
            .insert_console(&console(Agent::Claude, Path::new("/tmp/unused")))
            .unwrap();
        state
            .store
            .insert_session(&bare_session("console-session-1", Role::Console))
            .unwrap();

        let bound = resolve_bound_to(
            &state,
            "console-1",
            None,
            Role::Project,
            Some("console-session-1".to_string()),
        )
        .expect("resolved");
        assert_eq!(bound, Some("console-session-1".to_string()));
    }

    /// A binding changes afterwards only at the session's or its owner's request, so naming one
    /// that turns out not to exist, to belong to another console, or to be a project session
    /// outside the new session's own project must be refused outright rather than silently dropped
    /// or discovered later.
    #[test]
    fn binding_to_an_invalid_target_is_refused() {
        let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-invalid");
        state
            .store
            .insert_console(&console(Agent::Claude, Path::new("/tmp/unused")))
            .unwrap();
        let mut other_console = console(Agent::Claude, Path::new("/tmp/unused"));
        other_console.id = "console-2".to_string();
        state.store.insert_console(&other_console).unwrap();

        let mut elsewhere = bare_session("console-session-elsewhere", Role::Console);
        elsewhere.console_id = "console-2".to_string();
        state.store.insert_session(&elsewhere).unwrap();
        state
            .store
            .insert_session(&bare_session("project-session-1", Role::Project))
            .unwrap();

        // (requested target, why it is not a valid binding)
        let cases = [
            ("does-not-exist", "no such session at all"),
            (
                "console-session-elsewhere",
                "a console session of another console",
            ),
            (
                "project-session-1",
                "a project session when the new session has no project",
            ),
        ];
        for (target, why) in cases {
            let err = resolve_bound_to(
                &state,
                "console-1",
                None,
                Role::Project,
                Some(target.to_string()),
            )
            .expect_err(why);
            let coded = err.downcast_ref::<CodedError>().expect("a coded error");
            assert_eq!(coded.code, error_code::UNKNOWN_SESSION, "{why}");
        }
    }

    /// An unbound project session or a lead session of the new session's own project may own it.
    /// One of another project, or one with the role of a bound session, may not — among them a
    /// session bound to a lead session, which would make a third level.
    #[test]
    fn a_project_session_binds_only_to_an_unbound_or_lead_session_of_its_own_project() {
        let (state, _dir) = crate::test_support::app_state("coordinator-resolve-bound-to-project");
        state
            .store
            .insert_console(&console(Agent::Claude, Path::new("/tmp/unused")))
            .unwrap();
        state
            .store
            .insert_session(&bare_session("console-session-1", Role::Console))
            .unwrap();
        for id in ["project-1", "project-2"] {
            state
                .store
                .insert_project(&Project {
                    id: id.to_string(),
                    console_id: "console-1".to_string(),
                    host_id: LOCAL_HOST_ID.to_string(),
                    name: id.to_string(),
                    path: format!("/tmp/{id}"),
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
                    git_workflow: Default::default(),
                })
                .unwrap();
        }
        for (id, project, owner, lead) in [
            ("starter", "project-1", None, false),
            ("elsewhere", "project-2", None, false),
            (
                "bound-starter",
                "project-1",
                Some("console-session-1"),
                false,
            ),
            ("lead", "project-1", Some("console-session-1"), true),
            ("of-lead", "project-1", Some("lead"), false),
        ] {
            let mut session = bare_session(id, Role::Project);
            session.project_id = Some(project.to_string());
            session.bound_to = owner.map(str::to_string);
            session.lead = lead;
            state.store.insert_session(&session).unwrap();
        }
        let resolve = |target: &str| {
            resolve_bound_to(
                &state,
                "console-1",
                Some("project-1"),
                Role::Project,
                Some(target.to_string()),
            )
        };

        assert_eq!(resolve("starter").unwrap(), Some("starter".to_string()));
        assert_eq!(resolve("lead").unwrap(), Some("lead".to_string()));
        for target in ["elsewhere", "bound-starter", "of-lead"] {
            let err = resolve(target).expect_err(target);
            let coded = err.downcast_ref::<CodedError>().expect("a coded error");
            assert_eq!(coded.code, error_code::UNKNOWN_SESSION, "{target}");
        }
    }

    // -- accounts --------------------------------------------------------------

    #[tokio::test]
    async fn the_codex_approval_reviewer_applies_to_every_codex_account_and_only_to_codex() {
        let (state, _dir) = crate::test_support::app_state("codex-approval-reviewer");
        create_account(
            &state,
            Agent::Codex,
            "Work".into(),
            "/tmp/codex-work".into(),
        )
        .unwrap();
        let account = only_account(&state);
        let mut session = bare_session("session-1", Role::Project);
        session.agent = Agent::Codex;
        assert_eq!(CODEX_REVIEWER.for_session(&state, &session).unwrap(), None);

        handle(
            &state,
            None,
            RequestBody::SetCodexApprovalReviewer {
                reviewer: Some(ApprovalReviewer::AutoReview),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            state.store.get_settings().unwrap().codex_approval_reviewer,
            Some(ApprovalReviewer::AutoReview)
        );
        // The default account, a pinned one and a legacy pinned directory with no account id all
        // resolve the same value.
        assert_eq!(
            CODEX_REVIEWER.for_session(&state, &session).unwrap(),
            Some(ApprovalReviewer::AutoReview)
        );
        session.config_dir = Some("/tmp/codex-legacy".into());
        assert_eq!(
            CODEX_REVIEWER.for_session(&state, &session).unwrap(),
            Some(ApprovalReviewer::AutoReview)
        );
        session.account_id = Some(account.id.clone());
        session.config_dir = Some(account.config_dir.clone());
        assert_eq!(
            CODEX_REVIEWER.for_session(&state, &session).unwrap(),
            Some(ApprovalReviewer::AutoReview)
        );
        // Setting the same value again broadcasts nothing.
        let mut events = state.subscribe();
        handle(
            &state,
            None,
            RequestBody::SetCodexApprovalReviewer {
                reviewer: Some(ApprovalReviewer::AutoReview),
            },
        )
        .await
        .unwrap();
        assert!(events.try_recv().is_err());
        // A session of another agent reads nothing.
        session.agent = Agent::Claude;
        assert_eq!(CODEX_REVIEWER.for_session(&state, &session).unwrap(), None);

        handle(
            &state,
            None,
            RequestBody::SetCodexApprovalReviewer { reviewer: None },
        )
        .await
        .unwrap();
        session.agent = Agent::Codex;
        assert_eq!(CODEX_REVIEWER.for_session(&state, &session).unwrap(), None);
    }

    #[tokio::test]
    async fn the_claude_approval_mode_applies_to_every_claude_account_and_only_to_claude() {
        let (state, _dir) = crate::test_support::app_state("claude-approval-mode");
        create_account(
            &state,
            Agent::Claude,
            "Work".into(),
            "/tmp/claude-work".into(),
        )
        .unwrap();
        let account = only_account(&state);
        let mut session = bare_session("session-1", Role::Project);
        session.agent = Agent::Claude;
        assert_eq!(CLAUDE_MODE.for_session(&state, &session).unwrap(), None);

        handle(
            &state,
            None,
            RequestBody::SetClaudeApprovalMode {
                mode: Some(ClaudeApprovalMode::Manual),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            state.store.get_settings().unwrap().claude_approval_mode,
            Some(ClaudeApprovalMode::Manual)
        );
        assert_eq!(
            CLAUDE_MODE.for_session(&state, &session).unwrap(),
            Some(ClaudeApprovalMode::Manual)
        );
        session.config_dir = Some("/tmp/claude-legacy".into());
        assert_eq!(
            CLAUDE_MODE.for_session(&state, &session).unwrap(),
            Some(ClaudeApprovalMode::Manual)
        );
        session.account_id = Some(account.id.clone());
        session.config_dir = Some(account.config_dir.clone());
        assert_eq!(
            CLAUDE_MODE.for_session(&state, &session).unwrap(),
            Some(ClaudeApprovalMode::Manual)
        );
        session.agent = Agent::Codex;
        assert_eq!(CLAUDE_MODE.for_session(&state, &session).unwrap(), None);

        handle(
            &state,
            None,
            RequestBody::SetClaudeApprovalMode { mode: None },
        )
        .await
        .unwrap();
        session.agent = Agent::Claude;
        assert_eq!(CLAUDE_MODE.for_session(&state, &session).unwrap(), None);
    }

    fn create_account_request(agent: Agent, name: &str, config_dir: &str) -> RequestBody {
        RequestBody::CreateAccount {
            agent,
            name: name.to_string(),
            config_dir: config_dir.to_string(),
        }
    }

    fn set_availability(state: &Arc<AppState>, agent: Agent, availability: Availability) {
        state.set_agent_availability(vec![crate::protocol::AgentAvailability {
            agent,
            availability,
            default_account_dir: None,
        }]);
    }

    /// Claude compatibility is set per agent, is refused for Claude Code and unknown names without
    /// applying any of the request, and broadcasts only a change. Codex starts off here, since
    /// Claude Code is not among the added agents.
    #[tokio::test]
    async fn claude_compatibility_is_set_per_agent_and_refused_for_claude_code() {
        let (state, _dir) = crate::test_support::app_state("coordinator-claude-compat");
        let set = |changes: &[(&str, bool)]| {
            update_settings(
                &state,
                SettingsChanges {
                    claude_compatibility: changes
                        .iter()
                        .map(|(name, enabled)| (name.to_string(), *enabled))
                        .collect(),
                    ..SettingsChanges::default()
                },
            )
        };
        assert!(!state.store.claude_compatibility_of(Agent::Codex).unwrap());

        let mut events = state.subscribe();
        set(&[("codex", true)]).unwrap();
        assert!(matches!(
            events.try_recv(),
            Ok(Event::SettingsUpdated { settings })
                if settings.claude_compatibility
                    == [("codex".to_string(), true), ("grok".to_string(), true)].into()
        ));
        assert!(state.store.claude_compatibility_of(Agent::Codex).unwrap());
        assert!(state.store.claude_compatibility_of(Agent::Grok).unwrap());
        set(&[("codex", true)]).unwrap();
        assert!(events.try_recv().is_err(), "no change, no broadcast");

        assert!(set(&[("grok", false), ("claude", true)]).is_err());
        assert!(set(&[("nobody", true)]).is_err());
        assert!(state.store.claude_compatibility_of(Agent::Grok).unwrap());
    }

    /// Adding is refused for an agent determined not installed, for one whose availability is not
    /// determined yet, and for one already added; each leaves the list as it was.
    #[tokio::test]
    async fn adding_an_agent_is_refused_unless_it_is_installed_and_not_yet_added() {
        let (state, _dir) = crate::test_support::app_state("coordinator-add-agent");
        for agent in Agent::ALL {
            set_availability(&state, agent, Availability::Unavailable);
        }
        set_availability(&state, Agent::Claude, Availability::Available);
        state.store.set_added_agents_if_undecided(&[]).unwrap();

        let add = |agent| handle(&state, None, RequestBody::AddAgent { agent });
        let refusal = |result: Result<Option<Event>>| {
            let err = result.expect_err("refused");
            let (code, params, _) = error_parts(&err);
            (code, params.get("agent").cloned())
        };
        assert_eq!(
            refusal(add(Agent::Codex).await),
            (
                error_code::AGENT_NOT_INSTALLED.to_string(),
                Some("Codex".to_string())
            )
        );
        let mut events = state.subscribe();
        add(Agent::Claude).await.expect("added");
        assert!(matches!(
            events.try_recv(),
            Ok(Event::SettingsUpdated { settings })
                if settings.added_agents == Some(vec![Agent::Claude])
        ));
        assert_eq!(
            refusal(add(Agent::Claude).await).0,
            error_code::AGENT_ALREADY_ADDED
        );
        assert_eq!(
            state.store.added_agents().unwrap(),
            Some(vec![Agent::Claude])
        );

        let (pending, _dir) = crate::test_support::app_state("coordinator-add-agent-pending");
        assert_eq!(
            refusal(
                handle(
                    &pending,
                    None,
                    RequestBody::AddAgent {
                        agent: Agent::Claude
                    }
                )
                .await
            )
            .0,
            error_code::AGENT_AVAILABILITY_PENDING
        );
        assert_eq!(pending.store.added_agents().unwrap(), None);
    }

    /// An add that arrives once availability is known but before the fill ran settles the list
    /// first, so the agents with accounts are not left off it and the agent asked for, being
    /// installed, is found already in it.
    #[tokio::test]
    async fn adding_an_agent_first_settles_an_undecided_list() {
        let (state, _dir) = crate::test_support::app_state("coordinator-add-agent-fill");
        handle(
            &state,
            None,
            create_account_request(Agent::Codex, "Work", "/home/u/.codex-work"),
        )
        .await
        .expect("created");
        for agent in Agent::ALL {
            set_availability(&state, agent, Availability::Unavailable);
        }
        set_availability(&state, Agent::Claude, Availability::Available);

        let err = handle(
            &state,
            None,
            RequestBody::AddAgent {
                agent: Agent::Claude,
            },
        )
        .await
        .expect_err("already in the filled list");

        assert_eq!(error_parts(&err).0, error_code::AGENT_ALREADY_ADDED);
        assert_eq!(
            state.store.added_agents().unwrap(),
            Some(vec![Agent::Claude, Agent::Codex])
        );
    }

    /// Reordering takes a permutation of the decided list, persists and broadcasts it, answers a
    /// repeat without a broadcast, and refuses a missing, repeated or not-added agent.
    #[tokio::test]
    async fn reordering_agents_takes_only_a_permutation_of_the_list() {
        let (state, _dir) = crate::test_support::app_state("coordinator-reorder-agents");
        state
            .store
            .set_added_agents_if_undecided(&[Agent::Claude, Agent::Codex, Agent::Grok])
            .unwrap();
        let mut events = state.subscribe();
        let reorder =
            |agents: Vec<Agent>| handle(&state, None, RequestBody::ReorderAgents { agents });

        reorder(vec![Agent::Grok, Agent::Claude, Agent::Codex])
            .await
            .expect("reordered");
        let order = Some(vec![Agent::Grok, Agent::Claude, Agent::Codex]);
        assert_eq!(state.store.added_agents().unwrap(), order);
        assert!(matches!(
            events.try_recv(),
            Ok(Event::SettingsUpdated { settings }) if settings.added_agents == order
        ));

        reorder(vec![Agent::Grok, Agent::Claude, Agent::Codex])
            .await
            .expect("an unchanged order is accepted");
        assert!(events.try_recv().is_err(), "nothing changed");

        for bad in [
            vec![Agent::Grok, Agent::Claude],
            vec![Agent::Grok, Agent::Grok, Agent::Claude],
            vec![Agent::Grok, Agent::Claude, Agent::Codex, Agent::Codex],
            vec![],
        ] {
            let err = reorder(bad).await.expect_err("refused");
            assert_eq!(error_parts(&err).0, error_code::AGENT_ORDER_INVALID);
        }
        state.store.remove_added_agent(Agent::Codex).unwrap();
        let err = reorder(vec![Agent::Grok, Agent::Codex])
            .await
            .expect_err("a not-added agent is refused");
        assert_eq!(error_parts(&err).0, error_code::AGENT_ORDER_INVALID);
        assert_eq!(
            state.store.added_agents().unwrap(),
            Some(vec![Agent::Grok, Agent::Claude])
        );
    }

    #[tokio::test]
    async fn account_order_broadcasts_only_a_changed_valid_order() {
        let (state, _dir) = crate::test_support::app_state("coordinator-reorder-accounts");
        let mut events = state.subscribe();
        let reorder = |accounts| {
            handle(
                &state,
                None,
                RequestBody::ReorderAccounts {
                    agent: Agent::Claude,
                    accounts,
                },
            )
        };
        reorder(vec![None]).await.unwrap();
        assert!(
            matches!(events.try_recv(), Ok(Event::SettingsUpdated { settings })
            if settings.account_order["claude"] == vec![None])
        );
        reorder(vec![None]).await.unwrap();
        assert!(events.try_recv().is_err());
        let err = reorder(vec![]).await.unwrap_err();
        assert_eq!(error_parts(&err).0, error_code::ACCOUNT_ORDER_INVALID);
        assert!(events.try_recv().is_err());
    }

    /// An undecided list is filled first when availability is known, and refused otherwise.
    #[tokio::test]
    async fn reordering_agents_settles_an_undecided_list_or_is_refused() {
        let (state, _dir) = crate::test_support::app_state("coordinator-reorder-undecided");
        let err = handle(&state, None, RequestBody::ReorderAgents { agents: vec![] })
            .await
            .expect_err("undecided");
        assert_eq!(error_parts(&err).0, error_code::ADDED_AGENTS_UNDECIDED);
        let err = handle(
            &state,
            None,
            RequestBody::RemoveAgent {
                agent: Agent::Claude,
            },
        )
        .await
        .expect_err("undecided");
        assert_eq!(error_parts(&err).0, error_code::ADDED_AGENTS_UNDECIDED);

        for agent in Agent::ALL {
            set_availability(&state, agent, Availability::Unavailable);
        }
        set_availability(&state, Agent::Grok, Availability::Available);
        set_availability(&state, Agent::Claude, Availability::Available);
        handle(
            &state,
            None,
            RequestBody::ReorderAgents {
                agents: vec![Agent::Grok, Agent::Claude],
            },
        )
        .await
        .expect("filled, then reordered");
        assert_eq!(
            state.store.added_agents().unwrap(),
            Some(vec![Agent::Grok, Agent::Claude])
        );
    }

    /// Removing an agent takes it off the list and nothing else: its accounts stay, and clients
    /// are told only when the list changed.
    #[tokio::test]
    async fn removing_an_agent_keeps_its_accounts() {
        let (state, _dir) = crate::test_support::app_state("coordinator-remove-agent");
        state
            .store
            .set_added_agents_if_undecided(&[Agent::Claude, Agent::Codex])
            .unwrap();
        handle(
            &state,
            None,
            create_account_request(Agent::Codex, "Work", "/home/u/.codex-work"),
        )
        .await
        .expect("created");
        let account = only_account(&state);
        let mut events = state.subscribe();

        handle(
            &state,
            None,
            RequestBody::RemoveAgent {
                agent: Agent::Codex,
            },
        )
        .await
        .expect("removed");

        assert_eq!(
            state.store.added_agents().unwrap(),
            Some(vec![Agent::Claude])
        );
        assert_eq!(only_account(&state), account);
        assert!(matches!(
            events.try_recv(),
            Ok(Event::SettingsUpdated { settings })
                if settings.added_agents == Some(vec![Agent::Claude]) && settings.accounts == [account]
        ));

        handle(
            &state,
            None,
            RequestBody::RemoveAgent {
                agent: Agent::Codex,
            },
        )
        .await
        .expect("removing it again is not an error");
        assert!(events.try_recv().is_err(), "nothing changed");
    }

    fn only_account(state: &Arc<AppState>) -> Account {
        let accounts = state.store.list_accounts().unwrap();
        assert_eq!(accounts.len(), 1, "{accounts:?}");
        accounts.into_iter().next().unwrap()
    }

    /// Creating, renaming, repointing and removing an account over the protocol, each change
    /// visible afterwards through the settings record the account list travels with.
    #[tokio::test]
    async fn an_account_can_be_created_renamed_repointed_and_removed_over_the_protocol() {
        let (state, _dir) = crate::test_support::app_state("coordinator-account-crud");

        handle(
            &state,
            None,
            create_account_request(Agent::Codex, "Work", "/home/u/.codex-work"),
        )
        .await
        .expect("created");
        let created = only_account(&state);
        assert_eq!(created.name, "Work");
        assert_eq!(created.config_dir, "/home/u/.codex-work");
        assert_eq!(created.agent, Agent::Codex);

        handle(
            &state,
            None,
            RequestBody::UpdateAccount {
                account: created.id.clone(),
                name: Some("Work (renamed)".to_string()),
                config_dir: None,
            },
        )
        .await
        .expect("renamed");
        let renamed = only_account(&state);
        assert_eq!(renamed.name, "Work (renamed)");
        assert_eq!(renamed.config_dir, "/home/u/.codex-work", "untouched");

        handle(
            &state,
            None,
            RequestBody::UpdateAccount {
                account: created.id.clone(),
                name: None,
                config_dir: Some("/home/u/.codex-work-2".to_string()),
            },
        )
        .await
        .expect("repointed");
        let repointed = only_account(&state);
        assert_eq!(repointed.name, "Work (renamed)", "untouched");
        assert_eq!(repointed.config_dir, "/home/u/.codex-work-2");

        handle(
            &state,
            None,
            RequestBody::DeleteAccount {
                account: created.id.clone(),
                trash_directory: false,
            },
        )
        .await
        .expect("removed");
        assert!(state.store.list_accounts().unwrap().is_empty());
    }

    /// A name colliding within its agent is refused, naming the account it collides with — against
    /// an existing account, against the default account's own name, and not at all across agents.
    #[tokio::test]
    async fn a_colliding_account_name_is_refused_within_its_agent_only() {
        let (state, _dir) = crate::test_support::app_state("coordinator-account-name-collision");
        handle(
            &state,
            None,
            create_account_request(Agent::Claude, "Work", "/home/u/.claude-work"),
        )
        .await
        .expect("created");

        // Trimmed and case-insensitive, against an existing account of the same agent.
        let refused = handle(
            &state,
            None,
            create_account_request(Agent::Claude, "  work ", "/home/u/.claude-other"),
        )
        .await
        .expect_err("collides");
        let coded = refused.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::ACCOUNT_NAME_TAKEN);
        assert_eq!(coded.params.get("name").map(String::as_str), Some("Work"));

        // The default account's own name takes part in the comparison too.
        let refused_default = handle(
            &state,
            None,
            create_account_request(Agent::Claude, "default", "/home/u/.claude-other"),
        )
        .await
        .expect_err("collides with the default account");
        let coded = refused_default.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::ACCOUNT_NAME_TAKEN);

        // The same name is free for another agent: names are not compared across agents.
        handle(
            &state,
            None,
            create_account_request(Agent::Codex, "Work", "/home/u/.codex-work"),
        )
        .await
        .expect("a different agent, so no collision");
        assert_eq!(state.store.list_accounts().unwrap().len(), 2);
    }

    /// A directory another account of the agent already uses is refused on create and on update,
    /// however it is written (`~/`, a trailing slash, `./`), naming that account; the default
    /// account's resolved directory counts as taken; an account may keep its own directory; and
    /// another agent may use the same path.
    #[tokio::test]
    async fn a_taken_account_directory_is_refused_within_its_agent_only() {
        let (state, _dir) = crate::test_support::app_state("coordinator-account-dir-collision");
        let home = crate::paths::home_dir();
        let home_dir = home.to_string_lossy().into_owned();
        // Before availability is determined, the conventional default directory takes part.
        let before = handle(
            &state,
            None,
            create_account_request(Agent::Codex, "Early", "~/.codex/"),
        )
        .await
        .expect_err("the conventional default directory is taken");
        let coded = before.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::CONFIG_DIR_TAKEN);
        assert_eq!(
            coded.params.get("name").map(String::as_str),
            Some("Default")
        );
        state.set_agent_availability(vec![crate::protocol::AgentAvailability {
            agent: Agent::Claude,
            availability: Availability::Available,
            default_account_dir: Some(format!("{home_dir}/.claude")),
        }]);
        handle(
            &state,
            None,
            create_account_request(Agent::Claude, "Work", "/home/u/.claude-work"),
        )
        .await
        .expect("created");
        handle(
            &state,
            None,
            create_account_request(Agent::Claude, "Home", "~/.claude-home"),
        )
        .await
        .expect("created");

        let taken_by = |err: anyhow::Error| {
            let coded = err.downcast_ref::<CodedError>().expect("coded");
            assert_eq!(coded.code, error_code::CONFIG_DIR_TAKEN);
            assert_eq!(
                coded.params.get("agent").map(String::as_str),
                Some("Claude Code")
            );
            coded.params.get("name").cloned().expect("name")
        };
        for (dir, owner) in [
            ("/home/u/.claude-work", "Work"),
            ("/home/u/.claude-work/", "Work"),
            ("  /home/u/./.claude-work  ", "Work"),
            ("/home/u/x/../.claude-work", "Work"),
            ("~/.claude-home", "Home"),
            (&format!("{home_dir}/.claude-home"), "Home"),
            ("~/.claude", "Default"),
            ("~/.claude/", "Default"),
        ] {
            let err = handle(
                &state,
                None,
                create_account_request(Agent::Claude, "New", dir),
            )
            .await
            .expect_err(dir);
            assert_eq!(taken_by(err), owner, "{dir}");
        }
        assert_eq!(state.store.list_accounts().unwrap().len(), 2);

        // Another agent may use the same path.
        handle(
            &state,
            None,
            create_account_request(Agent::Codex, "Work", "/home/u/.claude-work"),
        )
        .await
        .expect("a different agent, so no collision");

        // An update into a taken directory is refused; keeping its own directory is not.
        let accounts = state.store.list_accounts().unwrap();
        let work = accounts
            .iter()
            .find(|a| a.agent == Agent::Claude && a.name == "Work")
            .unwrap();
        for dir in ["~/.claude-home", "~/.claude"] {
            let err = handle(
                &state,
                None,
                RequestBody::UpdateAccount {
                    account: work.id.clone(),
                    name: None,
                    config_dir: Some(dir.to_string()),
                },
            )
            .await
            .expect_err(dir);
            taken_by(err);
        }
        handle(
            &state,
            None,
            RequestBody::UpdateAccount {
                account: work.id.clone(),
                name: Some("Work 2".to_string()),
                config_dir: Some("/home/u/.claude-work/".to_string()),
            },
        )
        .await
        .expect("its own directory is not a collision");
        assert_eq!(
            state.store.get_account(&work.id).unwrap().unwrap().name,
            "Work 2"
        );
    }

    /// Two creates racing on one name, and on one directory, let exactly one through: the checks
    /// and the insert are a single critical section. The names differ in a non-ASCII letter's case
    /// on purpose: the table's `LOWER` index folds ASCII only, so a plain "Work"/"work" pair would
    /// be caught by it even without the lock, and only the coordinator's check under the lock
    /// catches this one. Threads are released together by a barrier
    /// and each runs the synchronous `create_account` directly, as concurrent requests on the
    /// runtime's worker threads do.
    #[test]
    fn racing_account_creates_let_exactly_one_through() {
        for (first, second) in [
            (
                ("Équipe", "/home/u/.claude-a"),
                ("équipe", "/home/u/.claude-b"),
            ),
            (
                ("Work", "/home/u/.claude-c"),
                ("Other", "/home/u/.claude-c/"),
            ),
        ] {
            for round in 0..20 {
                let (state, _dir) =
                    crate::test_support::app_state("coordinator-account-create-race");
                let barrier = std::sync::Barrier::new(2);
                let results: Vec<Result<()>> = std::thread::scope(|scope| {
                    [first, second]
                        .map(|(name, dir)| {
                            let (state, barrier) = (&state, &barrier);
                            scope.spawn(move || {
                                barrier.wait();
                                create_account(
                                    state,
                                    Agent::Claude,
                                    name.to_string(),
                                    dir.to_string(),
                                )
                            })
                        })
                        .into_iter()
                        .map(|handle| handle.join().expect("no panic"))
                        .collect()
                });
                assert_eq!(
                    results.iter().filter(|result| result.is_ok()).count(),
                    1,
                    "round {round}: {results:?}"
                );
                assert_eq!(state.store.list_accounts().unwrap().len(), 1);
            }
        }
    }

    /// On macOS the directory comparison folds case, on other platforms it is byte-exact; the
    /// stored text stays as typed either way.
    #[tokio::test]
    async fn account_directories_fold_case_only_on_macos() {
        let (state, _dir) = crate::test_support::app_state("coordinator-account-dir-case");
        handle(
            &state,
            None,
            create_account_request(Agent::Claude, "Work", "/home/u/.Claude-Work"),
        )
        .await
        .expect("created");
        let second = handle(
            &state,
            None,
            create_account_request(Agent::Claude, "Other", "/home/u/.claude-work"),
        )
        .await;
        if cfg!(target_os = "macos") {
            let coded = second.expect_err("same directory on a case-insensitive file system");
            let coded = coded.downcast_ref::<CodedError>().expect("coded");
            assert_eq!(coded.code, error_code::CONFIG_DIR_TAKEN);
        } else {
            second.expect("a different directory on a case-sensitive file system");
        }
        assert_eq!(
            state.store.list_accounts().unwrap()[0].config_dir,
            "/home/u/.Claude-Work",
            "stored as typed"
        );
    }

    /// `list_agent_models` answers with the request echoed (Claude Code's list needs no
    /// subprocess), and refuses an account that does not exist or belongs to another agent: the
    /// only way it ever errors, since anything wrong with asking the agent is an empty answer.
    #[tokio::test]
    async fn listing_agent_models_echoes_the_request_and_refuses_a_foreign_account() {
        let (state, _dir) = crate::test_support::app_state("coordinator-list-agent-models");
        handle(
            &state,
            Some("req-1".to_string()),
            create_account_request(Agent::Codex, "Work", "/home/u/.codex-work"),
        )
        .await
        .expect("created");
        let codex_account = only_account(&state).id;

        let reply = handle(
            &state,
            Some("req-2".to_string()),
            RequestBody::ListAgentModels {
                agent: Agent::Claude,
                account: None,
            },
        )
        .await
        .expect("answered");
        match reply {
            Some(Event::AgentModels {
                id,
                agent,
                account,
                models,
                unavailable,
            }) => {
                assert_eq!(id.as_deref(), Some("req-2"));
                assert_eq!((agent, account), (Agent::Claude, None));
                assert_eq!(models.len(), 4);
                assert!(unavailable.is_none());
            }
            other => panic!("expected agent_models, got {other:?}"),
        }

        for account in [codex_account, "no-such-account".to_string()] {
            let refused = handle(
                &state,
                None,
                RequestBody::ListAgentModels {
                    agent: Agent::Claude,
                    account: Some(account),
                },
            )
            .await
            .expect_err("not a Claude account");
            let coded = refused.downcast_ref::<CodedError>().expect("coded");
            assert_eq!(coded.code, error_code::UNKNOWN_ACCOUNT);
        }
    }

    #[tokio::test]
    async fn refreshing_quotas_refuses_a_foreign_or_unknown_account_and_acks_otherwise() {
        let (state, _dir) = crate::test_support::app_state("coordinator-refresh-quotas");
        handle(
            &state,
            None,
            create_account_request(Agent::Codex, "Work", "/home/u/.codex-work"),
        )
        .await
        .expect("created");
        let codex_account = only_account(&state).id;

        for (agent, account) in [
            (Some(Agent::Claude), Some(codex_account.clone())),
            (None, Some("no-such-account".to_string())),
        ] {
            let refused = handle(
                &state,
                None,
                RequestBody::RefreshAccountQuotas { agent, account },
            )
            .await
            .expect_err("not an account of that agent");
            let coded = refused.downcast_ref::<CodedError>().expect("coded");
            assert_eq!(coded.code, error_code::UNKNOWN_ACCOUNT);
        }

        // Nothing is available until the shell has been read, so nothing is run.
        for (agent, account) in [
            (None, None),
            (Some(Agent::Codex), None),
            (None, Some(codex_account.clone())),
            (Some(Agent::Codex), Some(codex_account)),
        ] {
            let reply = handle(
                &state,
                Some("req-1".to_string()),
                RequestBody::RefreshAccountQuotas { agent, account },
            )
            .await
            .expect("accepted");
            assert!(reply.is_none());
        }
    }

    #[tokio::test]
    async fn a_login_is_refused_for_a_foreign_account_and_a_cancel_for_a_login_that_is_not_running()
    {
        let (state, _dir) = crate::test_support::app_state("coordinator-account-login");
        handle(
            &state,
            None,
            create_account_request(Agent::Codex, "Work", "/home/u/.codex-work"),
        )
        .await
        .expect("created");
        let codex_account = only_account(&state).id;

        let refused = handle(
            &state,
            None,
            RequestBody::StartAccountLogin {
                agent: Agent::Claude,
                account: Some(codex_account.clone()),
            },
        )
        .await
        .expect_err("not an account of that agent");
        assert_eq!(
            refused.downcast_ref::<CodedError>().unwrap().code,
            error_code::UNKNOWN_ACCOUNT
        );
        for account in [None, Some(codex_account)] {
            let refused = handle(
                &state,
                None,
                RequestBody::CancelAccountLogin {
                    agent: Agent::Codex,
                    account,
                },
            )
            .await
            .expect_err("no login is running");
            assert_eq!(
                refused.downcast_ref::<CodedError>().unwrap().code,
                error_code::LOGIN_NOT_RUNNING
            );
        }
    }

    #[tokio::test]
    async fn the_default_account_takes_a_name_that_no_account_of_its_agent_shares_and_clears_it() {
        let (state, _dir) = crate::test_support::app_state("coordinator-default-name");
        handle(
            &state,
            None,
            create_account_request(Agent::Codex, "Work", "/home/u/.codex-work"),
        )
        .await
        .expect("created");
        let rename = |agent, name: &str| RequestBody::SetDefaultAccountName {
            agent,
            name: name.to_string(),
        };
        let code_of = |err: anyhow::Error| err.downcast_ref::<CodedError>().unwrap().code;
        let names =
            |state: &Arc<AppState>| state.store.get_settings().unwrap().default_account_names;

        // The default cannot take an owned account's name, in any letter case.
        let refused = handle(&state, None, rename(Agent::Codex, " work "))
            .await
            .expect_err("taken");
        assert_eq!(code_of(refused), error_code::ACCOUNT_NAME_TAKEN);
        // Another agent's accounts are not in the way.
        handle(&state, None, rename(Agent::Claude, "Work"))
            .await
            .unwrap();
        handle(&state, None, rename(Agent::Codex, "  Personal "))
            .await
            .unwrap();
        assert_eq!(names(&state)["codex"], "Personal");
        assert_eq!(names(&state)["claude"], "Work");

        // An owned account cannot take it either, and a rename to its own name is no collision.
        let refused = handle(
            &state,
            None,
            create_account_request(Agent::Codex, "personal", "/home/u/.codex-p"),
        )
        .await
        .expect_err("taken by the default");
        assert_eq!(code_of(refused), error_code::ACCOUNT_NAME_TAKEN);
        handle(&state, None, rename(Agent::Codex, "Personal"))
            .await
            .unwrap();
        // Nor is a change of letter case alone.
        handle(&state, None, rename(Agent::Codex, "PERSONAL"))
            .await
            .unwrap();
        assert_eq!(names(&state)["codex"], "PERSONAL");
        // The word the daemon reserves for the default stays the default's alone to keep.
        let refused = handle(
            &state,
            None,
            create_account_request(Agent::Codex, "default", "/home/u/.codex-d"),
        )
        .await
        .expect_err("reserved");
        assert_eq!(code_of(refused), error_code::ACCOUNT_NAME_TAKEN);

        // A blank name clears it back, for that agent only, and frees the name.
        let mut events = state.subscribe();
        handle(&state, None, rename(Agent::Codex, "  "))
            .await
            .unwrap();
        assert!(!names(&state).contains_key("codex"));
        assert_eq!(names(&state)["claude"], "Work");
        assert!(matches!(
            events.try_recv(),
            Ok(Event::SettingsUpdated { .. })
        ));
        // Clearing what is not set changes nothing and says nothing.
        handle(&state, None, rename(Agent::Codex, ""))
            .await
            .unwrap();
        assert!(events.try_recv().is_err());
        // The word for an unnamed default, in any letter case, clears a name like a blank does.
        handle(&state, None, rename(Agent::Codex, "Personal"))
            .await
            .unwrap();
        handle(&state, None, rename(Agent::Codex, " DEFAULT "))
            .await
            .unwrap();
        assert!(!names(&state).contains_key("codex"));
        handle(
            &state,
            None,
            create_account_request(Agent::Codex, "personal", "/home/u/.codex-p"),
        )
        .await
        .expect("free again");
    }

    #[tokio::test]
    async fn deleting_or_repointing_an_account_ends_its_running_login_as_cancelled() {
        let (state, dir) = crate::test_support::app_state("coordinator-login-ends");
        let bin = dir.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let env = crate::test_support::fake_agent(&bin, "claude", "sleep 30");
        for repoint in [true, false] {
            let home = dir.join("claude-work");
            handle(
                &state,
                None,
                create_account_request(Agent::Claude, "Work", &home.to_string_lossy()),
            )
            .await
            .expect("created");
            let account = only_account(&state).id;
            let mut events = state.subscribe();
            account_login::AccountLogins::start_for_test(
                &state,
                account_login::Start {
                    agent: Agent::Claude,
                    account: Some(account.clone()),
                    config_dir: Some(home.to_string_lossy().into_owned()),
                    resolved_dir: Some(home.to_string_lossy().into_owned()),
                },
                env.clone(),
                dir.join("run"),
            )
            .unwrap();
            assert_eq!(state.account_logins.running().len(), 1);

            let request = if repoint {
                RequestBody::UpdateAccount {
                    account: account.clone(),
                    name: None,
                    config_dir: Some(dir.join("elsewhere").to_string_lossy().into_owned()),
                }
            } else {
                RequestBody::DeleteAccount {
                    account: account.clone(),
                    trash_directory: false,
                }
            };
            handle(&state, None, request).await.expect("accepted");
            let ended = tokio::time::timeout(crate::test_support::PATIENCE, async {
                loop {
                    if let Ok(Event::AccountLoginUpdated { login }) = events.recv().await {
                        if login.status == crate::protocol::LoginStatus::Cancelled {
                            return login;
                        }
                    }
                }
            })
            .await
            .expect("the login was cancelled");
            assert_eq!(ended.account.as_deref(), Some(account.as_str()));
            assert!(state.account_logins.running().is_empty());
            if repoint {
                handle(
                    &state,
                    None,
                    RequestBody::DeleteAccount {
                        account,
                        trash_directory: false,
                    },
                )
                .await
                .expect("deleted");
            }
        }
    }

    #[tokio::test]
    async fn repointing_or_deleting_an_account_drops_its_quota_reading() {
        let (state, _dir) = crate::test_support::app_state("coordinator-forget-quotas");
        handle(
            &state,
            None,
            create_account_request(Agent::Codex, "Work", "/home/u/.codex-work"),
        )
        .await
        .expect("created");
        let account = only_account(&state).id;
        let hold = |state: &Arc<AppState>| {
            state
                .account_quotas
                .hold_for_test(&crate::protocol::AccountQuota {
                    agent: Agent::Codex,
                    account: Some(account.clone()),
                    fetched_at: 1,
                    plan: None,
                    email: None,
                    windows: Vec::new(),
                    unavailable: None,
                });
        };

        hold(&state);
        assert_eq!(state.account_quotas.list().len(), 1);
        handle(
            &state,
            None,
            RequestBody::UpdateAccount {
                account: account.clone(),
                name: Some("Renamed".to_string()),
                config_dir: None,
            },
        )
        .await
        .expect("updated");
        // A rename leaves the reading alone; repointing the account drops it.
        assert_eq!(state.account_quotas.list().len(), 1);
        handle(
            &state,
            None,
            RequestBody::UpdateAccount {
                account: account.clone(),
                name: None,
                config_dir: Some("/home/u/.codex-other".to_string()),
            },
        )
        .await
        .expect("repointed");
        assert!(state.account_quotas.list().is_empty());

        hold(&state);
        handle(
            &state,
            None,
            RequestBody::DeleteAccount {
                account: account.clone(),
                trash_directory: false,
            },
        )
        .await
        .expect("deleted");
        assert!(state.account_quotas.list().is_empty());
    }

    /// A non-absolute directory is refused with nothing saved; a directory that does not exist at
    /// all is accepted — the two rules this milestone changes about a config directory.
    #[tokio::test]
    async fn a_non_absolute_account_dir_is_refused_and_a_non_existent_one_is_accepted() {
        let (state, _dir) = crate::test_support::app_state("coordinator-account-dir-rules");

        let refused = handle(
            &state,
            None,
            create_account_request(Agent::Claude, "Relative", "relative/path"),
        )
        .await
        .expect_err("not absolute");
        let coded = refused.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::CONFIG_DIR_NOT_ABSOLUTE);
        assert!(
            state.store.list_accounts().unwrap().is_empty(),
            "nothing saved"
        );

        handle(
            &state,
            None,
            create_account_request(
                Agent::Claude,
                "Not yet created",
                "/tmp/octoboard-does-not-exist",
            ),
        )
        .await
        .expect("a missing directory is accepted");
        assert_eq!(
            only_account(&state).config_dir,
            "/tmp/octoboard-does-not-exist"
        );
    }

    /// An `update_console` request that changes nothing, then `edit` applied to it.
    fn update_console_request(console: &str, edit: impl FnOnce(&mut RequestBody)) -> RequestBody {
        let mut request = RequestBody::UpdateConsole {
            console: console.to_string(),
            name: None,
            default_agent: None,
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
        };
        edit(&mut request);
        request
    }

    /// A new console stores its default agent and launch settings; a model or effort is trimmed,
    /// with blank read as unset.
    #[tokio::test]
    async fn a_new_console_stores_its_default_agent_and_launch_settings() {
        let (state, dir) = crate::test_support::app_state("coordinator-create-console-agent");
        crate::test_support::offer_models(&state, Agent::Codex, &[("gpt-5", &["high"])]);
        handle(
            &state,
            None,
            RequestBody::CreateConsole {
                name: "Console".to_string(),
                default_agent: Agent::Codex,
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
                claude_console_session_model: Some("  ".to_string()),
                claude_console_session_effort: None,
                codex_console_session_model: Some(" gpt-5 ".to_string()),
                codex_console_session_effort: Some("high".to_string()),
                grok_console_session_model: None,
                grok_console_session_effort: None,
                icon: None,
                console_session_prompt: None,
            },
        )
        .await
        .expect("created");
        let created = state.store.list_consoles().unwrap().remove(0);
        assert!(Path::new(&created.workdir).starts_with(&*dir));
        assert_eq!(created.default_agent, Agent::Codex);
        assert_eq!(created.claude_console_session_model, None);
        assert_eq!(
            created.codex_console_session_model.as_deref(),
            Some("gpt-5")
        );
        assert_eq!(
            created.codex_console_session_effort.as_deref(),
            Some("high")
        );

        // A model the agent does not offer is refused, and no second console is created.
        let err = handle(
            &state,
            None,
            RequestBody::CreateConsole {
                name: "Other".to_string(),
                default_agent: Agent::Codex,
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
                codex_console_session_model: Some("gpt-9".to_string()),
                codex_console_session_effort: None,
                grok_console_session_model: None,
                grok_console_session_effort: None,
                icon: None,
                console_session_prompt: None,
            },
        )
        .await
        .expect_err("refused");
        let coded = err.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::INVALID_LAUNCH_SETTING);
        assert_eq!(state.store.list_consoles().unwrap().len(), 1);
    }

    /// A project session falls back to the project's own default, then the console's project
    /// session agent, then (for a console that names none) `default_agent`; a console session
    /// always takes `default_agent`.
    #[test]
    fn a_session_agent_falls_back_through_the_project_to_the_project_session_default() {
        let workdir = Path::new("/tmp/console-1");
        let mut console = console(Agent::Claude, workdir);
        let mut project = project_of("console-1", "p1");
        assert_eq!(session_agent(None, Some(&project), &console), Agent::Claude);

        console.project_session_agent = Some(Agent::Codex);
        assert_eq!(session_agent(None, Some(&project), &console), Agent::Codex);
        assert_eq!(session_agent(None, None, &console), Agent::Claude);

        project.default_agent = Some(Agent::Grok);
        assert_eq!(session_agent(None, Some(&project), &console), Agent::Grok);
        assert_eq!(
            session_agent(Some(Agent::Claude), Some(&project), &console),
            Agent::Claude
        );
    }

    /// The project session agent is stored at creation, left alone by an absent field, set by a
    /// value and cleared by `null`.
    #[tokio::test]
    async fn the_project_session_agent_is_left_set_or_cleared_by_an_update() {
        let (state, dir) = crate::test_support::app_state("coordinator-project-session-agent");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        let stored = || state.store.get_console("console-1").unwrap().unwrap();
        let update = |value: Option<Option<Agent>>| {
            update_console_request("console-1", move |request| {
                let RequestBody::UpdateConsole {
                    project_session_agent,
                    ..
                } = request
                else {
                    unreachable!()
                };
                *project_session_agent = value;
            })
        };

        handle(&state, None, update(Some(Some(Agent::Codex))))
            .await
            .unwrap();
        assert_eq!(stored().project_session_agent, Some(Agent::Codex));
        handle(&state, None, update(None)).await.unwrap();
        assert_eq!(stored().project_session_agent, Some(Agent::Codex));
        handle(&state, None, update(Some(None))).await.unwrap();
        assert_eq!(stored().project_session_agent, None);

        handle(
            &state,
            None,
            RequestBody::CreateConsole {
                name: "Other".to_string(),
                default_agent: Agent::Claude,
                project_session_agent: Some(Agent::Grok),
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
            },
        )
        .await
        .unwrap();
        let created = state
            .store
            .list_consoles()
            .unwrap()
            .into_iter()
            .find(|console| console.name == "Other")
            .unwrap();
        assert_eq!(created.project_session_agent, Some(Agent::Grok));
    }

    /// A launch setting is left alone when absent, set (trimmed) by a value, and cleared by `null`
    /// or a blank string; each agent's is independent.
    #[tokio::test]
    async fn a_console_session_launch_setting_is_left_set_or_cleared_by_an_update() {
        let (state, dir) = crate::test_support::app_state("coordinator-update-console-launch");
        let workdir = console_workdir(&dir);
        let mut seeded = console(Agent::Claude, &workdir);
        seeded.claude_console_session_model = Some("opus".to_string());
        seeded.grok_console_session_model = Some("grok-4".to_string());
        state.store.insert_console(&seeded).unwrap();
        let stored = || state.store.get_console("console-1").unwrap().unwrap();

        handle(&state, None, update_console_request("console-1", |_| {}))
            .await
            .unwrap();
        assert_eq!(
            stored().claude_console_session_model.as_deref(),
            Some("opus")
        );

        handle(
            &state,
            None,
            update_console_request("console-1", |request| {
                let RequestBody::UpdateConsole {
                    claude_console_session_model,
                    grok_console_session_model,
                    ..
                } = request
                else {
                    unreachable!()
                };
                *claude_console_session_model = Some(Some(" sonnet ".to_string()));
                *grok_console_session_model = Some(None);
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            stored().claude_console_session_model.as_deref(),
            Some("sonnet")
        );
        assert_eq!(stored().grok_console_session_model, None);

        handle(
            &state,
            None,
            update_console_request("console-1", |request| {
                let RequestBody::UpdateConsole {
                    claude_console_session_model,
                    ..
                } = request
                else {
                    unreachable!()
                };
                *claude_console_session_model = Some(Some("  ".to_string()));
            }),
        )
        .await
        .unwrap();
        assert_eq!(stored().claude_console_session_model, None);
    }

    /// The user's addition to the console session's instructions is left alone when absent, set
    /// (trimmed) by a value, and cleared by `null` or a blank string, and the instruction file
    /// holds it only while there is some.
    #[tokio::test]
    async fn a_console_session_prompt_is_left_set_or_cleared_by_an_update() {
        let (state, dir) = crate::test_support::app_state("coordinator-update-console-prompt");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        let stored = || {
            state
                .store
                .get_console("console-1")
                .unwrap()
                .unwrap()
                .console_session_prompt
        };
        let file = || std::fs::read_to_string(workdir.join("AGENTS.md")).unwrap();
        let update = |prompt: Option<Option<&str>>| {
            update_console_request("console-1", move |request| {
                let RequestBody::UpdateConsole {
                    console_session_prompt,
                    ..
                } = request
                else {
                    unreachable!()
                };
                *console_session_prompt = prompt.map(|text| text.map(str::to_string));
            })
        };

        handle(&state, None, update(Some(Some("  Be brief.  "))))
            .await
            .unwrap();
        assert_eq!(stored().as_deref(), Some("Be brief."));
        assert_eq!(
            file(),
            mcp::console_instructions::console_session_instructions(
                Some("Be brief."),
                &mcp::role::RoleOptions {
                    ask_as_choices: true,
                    ..Default::default()
                }
            )
        );

        handle(&state, None, update(None)).await.unwrap();
        assert_eq!(stored().as_deref(), Some("Be brief."));
        assert_eq!(
            file(),
            mcp::console_instructions::console_session_instructions(
                Some("Be brief."),
                &mcp::role::RoleOptions {
                    ask_as_choices: true,
                    ..Default::default()
                }
            )
        );

        handle(&state, None, update(Some(None))).await.unwrap();
        assert_eq!(stored(), None);
        assert_eq!(
            file(),
            mcp::console_instructions::console_session_instructions(
                None,
                &mcp::role::RoleOptions {
                    ask_as_choices: true,
                    ..Default::default()
                }
            )
        );

        handle(&state, None, update(Some(Some("Answer in French."))))
            .await
            .unwrap();
        assert_eq!(stored().as_deref(), Some("Answer in French."));
        assert_eq!(
            file(),
            mcp::console_instructions::console_session_instructions(
                Some("Answer in French."),
                &mcp::role::RoleOptions {
                    ask_as_choices: true,
                    ..Default::default()
                }
            )
        );

        handle(&state, None, update(Some(Some("  \n "))))
            .await
            .unwrap();
        assert_eq!(stored(), None);
        assert_eq!(
            file(),
            mcp::console_instructions::console_session_instructions(
                None,
                &mcp::role::RoleOptions {
                    ask_as_choices: true,
                    ..Default::default()
                }
            )
        );
        assert!(!file().contains("Answer in French."));

        // The file follows the language the user's interface is set to at the next write.
        state.store.set_interface_language("ja").unwrap();
        handle(&state, None, update(Some(Some("Be brief."))))
            .await
            .unwrap();
        assert!(file().contains("interface to Japanese (ja)."));
    }

    /// A name that is blank, and a model or effort the agent does not offer, are refused by the
    /// function the control socket and the console session's tools share, with nothing written; a
    /// value that is not being changed is not checked, so a stored one the agent has since dropped
    /// does not stop an unrelated edit, and an agent whose list could not be read takes anything.
    #[tokio::test]
    async fn an_update_refuses_a_blank_name_and_a_model_the_agent_does_not_offer() {
        let (state, dir) = crate::test_support::app_state("coordinator-update-validation");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        let mut project = Project {
            id: "project-1".to_string(),
            console_id: "console-1".to_string(),
            host_id: LOCAL_HOST_ID.to_string(),
            name: "Project".to_string(),
            path: "/tmp/project-1".to_string(),
            default_agent: None,
            source: ProjectSource::Local,
            remote_url: None,
            claude_account_id: ProjectAccount::Inherit,
            codex_account_id: ProjectAccount::Inherit,
            grok_account_id: ProjectAccount::Inherit,
            claude_session_model: Some("retired".to_string()),
            claude_session_effort: None,
            codex_session_model: None,
            codex_session_effort: None,
            grok_session_model: None,
            grok_session_effort: None,
            trust_consent: false,
            pinned: false,
            tags: Vec::new(),
            description: None,
            git_workflow: Default::default(),
        };
        state.store.insert_project(&project).unwrap();
        let refused = |err: anyhow::Error| {
            err.downcast_ref::<CodedError>()
                .expect("a coded error")
                .code
        };
        let change = |edit: fn(&mut ProjectChanges)| {
            let mut changes = ProjectChanges::default();
            edit(&mut changes);
            changes
        };
        let launch = |model: &str, effort: Option<&str>| LaunchChange {
            model: Some(Some(model.to_string())),
            effort: effort.map(|effort| Some(effort.to_string())),
        };

        let err = update_project(
            &state,
            "project-1",
            change(|changes| changes.name = Some("  ".to_string())),
        )
        .await
        .expect_err("blank name");
        assert_eq!(refused(err), error_code::FIELD_REQUIRED);

        let mut changes = ProjectChanges::default();
        changes.launch.insert(Agent::Claude, launch("gpt-9", None));
        let err = update_project(&state, "project-1", changes)
            .await
            .expect_err("model not offered");
        assert_eq!(refused(err), error_code::INVALID_LAUNCH_SETTING);

        let mut changes = ProjectChanges::default();
        changes
            .launch
            .insert(Agent::Claude, launch("opus", Some("extreme")));
        let err = update_project(&state, "project-1", changes)
            .await
            .expect_err("effort not offered");
        assert_eq!(refused(err), error_code::INVALID_LAUNCH_SETTING);
        project = state.store.get_project("project-1").unwrap().unwrap();
        assert_eq!(project.name, "Project");
        assert_eq!(project.claude_session_model.as_deref(), Some("retired"));

        // The stored model is not on the list, and sending it again is not a change.
        let mut changes = ProjectChanges::default();
        changes
            .launch
            .insert(Agent::Claude, launch("retired", None));
        update_project(&state, "project-1", changes)
            .await
            .expect("an unchanged model is not checked");

        // The stored model is not on the list, and saving something else is fine.
        let renamed = update_project(
            &state,
            "project-1",
            change(|changes| changes.name = Some(" Renamed ".to_string())),
        )
        .await
        .expect("renamed");
        assert_eq!(renamed.name, "Renamed");
        assert_eq!(renamed.claude_session_model.as_deref(), Some("retired"));

        // Nothing to check Codex's model against, so it is taken as given.
        crate::test_support::offer_models(&state, Agent::Codex, &[]);
        let mut changes = ProjectChanges::default();
        changes
            .launch
            .insert(Agent::Codex, launch("anything", None));
        let updated = update_project(&state, "project-1", changes)
            .await
            .expect("accepted");
        assert_eq!(updated.codex_session_model.as_deref(), Some("anything"));

        let err = update_console(
            &state,
            "console-1",
            ConsoleChanges {
                name: Some(" ".to_string()),
                ..ConsoleChanges::default()
            },
        )
        .await
        .expect_err("blank console name");
        assert_eq!(refused(err), error_code::FIELD_REQUIRED);
    }

    /// A project session reads its project's pair for its own agent and never the console's; a
    /// console session reads the console's pair and never a project's. A project that sets nothing
    /// leaves its sessions launching exactly as the agent would unasked, whatever the console sets.
    #[test]
    fn a_session_launches_with_the_pair_of_the_record_that_owns_it() {
        let (state, _dir) = crate::test_support::app_state("coordinator-launch-settings");
        let mut owner = console(Agent::Claude, Path::new("/tmp/unused"));
        owner.claude_console_session_model = Some("console-opus".to_string());
        owner.codex_console_session_effort = Some("console-high".to_string());
        owner.grok_console_session_model = Some("console-grok".to_string());
        owner.grok_console_session_effort = Some("console-low".to_string());
        state.store.insert_console(&owner).unwrap();
        let mut configured = project_of("console-1", "configured");
        configured.claude_session_model = Some("project-sonnet".to_string());
        configured.claude_session_effort = Some("project-low".to_string());
        configured.codex_session_model = Some("project-gpt".to_string());
        state.store.insert_project(&configured).unwrap();
        state
            .store
            .insert_project(&project_of("console-1", "plain"))
            .unwrap();

        let launched = |role: Role, project_id: Option<&str>, agent: Agent| {
            let mut session = bare_session("s", role);
            session.project_id = project_id.map(str::to_string);
            session.agent = agent;
            launch_settings(&state, &session).unwrap()
        };
        let pair = |model: Option<&str>, effort: Option<&str>| {
            (model.map(str::to_string), effort.map(str::to_string))
        };

        assert_eq!(
            launched(Role::Console, None, Agent::Claude),
            pair(Some("console-opus"), None)
        );
        assert_eq!(
            launched(Role::Console, None, Agent::Codex),
            pair(None, Some("console-high"))
        );
        assert_eq!(
            launched(Role::Console, None, Agent::Grok),
            pair(Some("console-grok"), Some("console-low"))
        );
        assert_eq!(
            launched(Role::Project, Some("configured"), Agent::Claude),
            pair(Some("project-sonnet"), Some("project-low"))
        );
        assert_eq!(
            launched(Role::Project, Some("configured"), Agent::Codex),
            pair(Some("project-gpt"), None)
        );
        assert_eq!(
            launched(Role::Project, Some("configured"), Agent::Grok),
            pair(None, None)
        );
        assert_eq!(
            launched(Role::Project, Some("plain"), Agent::Claude),
            pair(None, None)
        );
        assert_eq!(
            launched(Role::Project, Some("plain"), Agent::Codex),
            pair(None, None)
        );
    }

    /// Every launch reads inherited defaults live, independently per field, while a session's
    /// explicit value or explicit null takes precedence over its parent's defaults.
    #[test]
    fn session_launch_overrides_take_precedence_over_live_defaults() {
        let (state, _dir) = crate::test_support::app_state("coordinator-session-overrides");
        let mut owner = console(Agent::Claude, Path::new("/tmp/unused"));
        owner.claude_console_session_model = Some("console-model".to_string());
        owner.claude_console_session_effort = Some("console-effort".to_string());
        state.store.insert_console(&owner).unwrap();
        let mut project = project_of("console-1", "project-1");
        project.claude_session_model = Some("project-model".to_string());
        project.claude_session_effort = Some("project-effort".to_string());
        state.store.insert_project(&project).unwrap();
        for (role, project_id) in [(Role::Console, None), (Role::Project, Some("project-1"))] {
            let mut session = bare_session("s", role);
            session.project_id = project_id.map(str::to_string);
            let defaults = if role == Role::Console {
                (
                    owner.claude_console_session_model.clone(),
                    owner.claude_console_session_effort.clone(),
                )
            } else {
                (
                    project.claude_session_model.clone(),
                    project.claude_session_effort.clone(),
                )
            };
            assert_eq!(launch_settings(&state, &session).unwrap(), defaults);
            session.model = normalize_session_launch_setting(Some(Some(" custom ".to_string())));
            assert_eq!(
                launch_settings(&state, &session).unwrap(),
                (Some("custom".to_string()), defaults.1)
            );
            session.effort = normalize_session_launch_setting(Some(Some(" high ".to_string())));
            assert_eq!(
                launch_settings(&state, &session).unwrap(),
                (Some("custom".to_string()), Some("high".to_string()))
            );
            session.effort = normalize_session_launch_setting(Some(Some("  ".to_string())));
            assert_eq!(
                launch_settings(&state, &session).unwrap(),
                (Some("custom".to_string()), None)
            );
            session.model = normalize_session_launch_setting(Some(None));
            assert_eq!(launch_settings(&state, &session).unwrap(), (None, None));
        }
        let mut inherited = bare_session("legacy", Role::Project);
        inherited.project_id = Some("project-1".to_string());
        state.store.insert_session(&inherited).unwrap();
        project.claude_session_model = Some("changed-model".to_string());
        state.store.update_project(&project).unwrap();
        let reread = state.session_record("legacy").unwrap();
        assert_eq!(
            launch_settings(&state, &reread).unwrap().0.as_deref(),
            Some("changed-model")
        );
        assert_eq!(normalize_session_launch_setting(None), None);
    }

    /// A project session launches with its project's git workflow settings as they are at that
    /// launch, the project's own value winning; a console session, which is given no rule, with
    /// none.
    #[test]
    fn a_project_session_launches_with_its_projects_git_workflow_settings() {
        let (state, _dir) = crate::test_support::app_state("coordinator-git-workflow-settings");
        state
            .store
            .insert_console(&console(Agent::Claude, Path::new("/tmp/unused")))
            .unwrap();
        let mut project = project_of("console-1", "project-1");
        state.store.insert_project(&project).unwrap();
        let mut session = bare_session("s", Role::Project);
        session.project_id = Some("project-1".to_string());
        assert_eq!(
            launch_git_workflow_settings(&state, &session).unwrap(),
            Some(GitWorkflowSettings::default())
        );
        state
            .store
            .change_git_workflow_settings(&GitWorkflowSettingsChanges {
                delivery: Some(crate::git_workflow::Delivery::Merge),
                ..GitWorkflowSettingsChanges::default()
            })
            .unwrap();
        project.git_workflow.development = Some(crate::git_workflow::Development::InPlace);
        state.store.update_project(&project).unwrap();
        assert_eq!(
            launch_git_workflow_settings(&state, &session).unwrap(),
            Some(GitWorkflowSettings {
                development: crate::git_workflow::Development::InPlace,
                delivery: crate::git_workflow::Delivery::Merge,
                ..GitWorkflowSettings::default()
            })
        );
        let mut console_session = bare_session("c", Role::Console);
        console_session.project_id = Some("project-1".to_string());
        assert_eq!(
            launch_git_workflow_settings(&state, &console_session).unwrap(),
            None
        );
    }

    /// A project session takes each value from its project, else from its console's project
    /// session pair, else leaves it to the agent, independently per value and per agent; a
    /// console that names no pair leaves it to the agent, and a console session never reads it.
    #[test]
    fn a_project_session_falls_back_to_the_consoles_project_session_pair_per_value() {
        let (state, _dir) = crate::test_support::app_state("coordinator-project-session-pair");
        let mut owner = console(Agent::Claude, Path::new("/tmp/unused"));
        owner.claude_console_session_model = Some("console-session-model".to_string());
        state.store.insert_console(&owner).unwrap();
        let mut project = project_of("console-1", "project-1");
        state.store.insert_project(&project).unwrap();
        let mut session = bare_session("s", Role::Project);
        session.project_id = Some("project-1".to_string());

        // A console that names no pair: nothing to fall back to.
        assert_eq!(launch_settings(&state, &session).unwrap(), (None, None));

        owner.claude_project_session_model = Some("fallback-model".to_string());
        owner.claude_project_session_effort = Some("fallback-effort".to_string());
        owner.codex_project_session_model = Some("codex-fallback".to_string());
        state.store.update_console(&owner).unwrap();
        assert_eq!(
            launch_settings(&state, &session).unwrap(),
            (
                Some("fallback-model".to_string()),
                Some("fallback-effort".to_string())
            )
        );

        // The project's own value wins for that value alone.
        project.claude_session_model = Some("project-model".to_string());
        state.store.update_project(&project).unwrap();
        assert_eq!(
            launch_settings(&state, &session).unwrap(),
            (
                Some("project-model".to_string()),
                Some("fallback-effort".to_string())
            )
        );

        // Another agent's pair is its own.
        session.agent = Agent::Codex;
        assert_eq!(
            launch_settings(&state, &session).unwrap(),
            (Some("codex-fallback".to_string()), None)
        );
        session.agent = Agent::Claude;

        // The session's own choice, an explicit null included, wins over both.
        session.model = normalize_session_launch_setting(Some(None));
        session.effort = normalize_session_launch_setting(Some(Some(" high ".to_string())));
        assert_eq!(
            launch_settings(&state, &session).unwrap(),
            (None, Some("high".to_string()))
        );

        // A console session reads only the console session pair.
        let console_session = bare_session("c", Role::Console);
        assert_eq!(
            launch_settings(&state, &console_session).unwrap(),
            (Some("console-session-model".to_string()), None)
        );
    }

    /// The project session pair is stored at creation, left alone by an absent field, set (trimmed)
    /// by a value and cleared by `null`, each agent and value independently.
    #[tokio::test]
    async fn the_project_session_pair_is_left_set_or_cleared_by_an_update() {
        let (state, dir) = crate::test_support::app_state("coordinator-update-project-pair");
        crate::test_support::offer_models(&state, Agent::Codex, &[("gpt-5", &["high"])]);
        crate::test_support::offer_models(&state, Agent::Claude, &[("opus", &["medium"])]);
        let workdir = console_workdir(&dir);
        let mut seeded = console(Agent::Claude, &workdir);
        seeded.claude_project_session_model = Some("opus".to_string());
        seeded.grok_project_session_effort = Some("low".to_string());
        state.store.insert_console(&seeded).unwrap();
        let stored = || state.store.get_console("console-1").unwrap().unwrap();

        handle(&state, None, update_console_request("console-1", |_| {}))
            .await
            .unwrap();
        assert_eq!(
            stored().claude_project_session_model.as_deref(),
            Some("opus")
        );

        handle(
            &state,
            None,
            update_console_request("console-1", |request| {
                let RequestBody::UpdateConsole {
                    claude_project_session_model,
                    claude_project_session_effort,
                    codex_project_session_model,
                    ..
                } = request
                else {
                    unreachable!()
                };
                *claude_project_session_model = Some(None);
                *claude_project_session_effort = Some(Some("  medium ".to_string()));
                *codex_project_session_model = Some(Some("gpt-5".to_string()));
            }),
        )
        .await
        .unwrap();
        let updated = stored();
        assert_eq!(updated.claude_project_session_model, None);
        assert_eq!(
            updated.claude_project_session_effort.as_deref(),
            Some("medium")
        );
        assert_eq!(
            updated.codex_project_session_model.as_deref(),
            Some("gpt-5")
        );
        // The console session pair is a separate setting, and the other agents' values stay.
        assert_eq!(updated.claude_console_session_model, None);
        assert_eq!(updated.grok_project_session_effort.as_deref(), Some("low"));
    }

    /// A project session that sets no model takes the console's project session one, so an effort
    /// given alone is checked against that model's levels wherever it is given: a project update,
    /// an opened session, and the console's own project session pair.
    #[tokio::test]
    async fn an_effort_given_alone_is_checked_against_the_model_it_inherits_from_the_console() {
        let (state, dir) = crate::test_support::app_state("coordinator-inherited-model-check");
        crate::test_support::offer_models(
            &state,
            Agent::Claude,
            &[("fallback", &["low"]), ("other", &["high"])],
        );
        let workdir = console_workdir(&dir);
        let mut owner = console(Agent::Claude, &workdir);
        owner.claude_project_session_model = Some("fallback".to_string());
        state.store.insert_console(&owner).unwrap();
        state
            .store
            .insert_project(&project_of("console-1", "project-1"))
            .unwrap();
        let refused = |err: anyhow::Error| err.downcast_ref::<CodedError>().expect("coded").code;

        // A project given only an effort the console's model does not take.
        let effort_only = |effort: &str| LaunchChange {
            model: None,
            effort: Some(Some(effort.to_string())),
        };
        let mut changes = ProjectChanges::default();
        changes.launch.insert(Agent::Claude, effort_only("high"));
        let err = update_project(&state, "project-1", changes)
            .await
            .expect_err("effort of another model");
        assert_eq!(refused(err), error_code::INVALID_LAUNCH_SETTING);
        let mut changes = ProjectChanges::default();
        changes.launch.insert(Agent::Claude, effort_only("low"));
        update_project(&state, "project-1", changes)
            .await
            .expect("the inherited model's own effort");

        // A project added with only an effort is checked the same way.
        let added = |name: &str, effort: &str| {
            let repo = dir.join(name);
            std::fs::create_dir(&repo).expect("repo");
            add_project(
                &state,
                AddProjectRequest {
                    console_id: "console-1".to_string(),
                    source: ProjectSource::Local,
                    path: Some(repo.to_string_lossy().into_owned()),
                    remote_url: None,
                    name: None,
                    default_agent: None,
                    settings: NewProjectSettings {
                        claude_session_effort: Some(effort.to_string()),
                        ..NewProjectSettings::default()
                    },
                    detect_default_agent: false,
                    tags: None,
                    description: None,
                    check_launch: true,
                    check_text_limits: true,
                },
            )
        };
        let err = added("rejected", "high")
            .await
            .expect_err("effort of another model");
        assert_eq!(refused(err), error_code::INVALID_LAUNCH_SETTING);
        added("accepted", "low").await.expect("accepted");

        // A project session opened with only that effort.
        let open = |effort: &str| OpenRequest {
            project_id: Some("project-1".to_string()),
            effort: Some(Some(effort.to_string())),
            ..open_request()
        };
        let err = check_open(&state, &open("high"))
            .await
            .expect_err("effort of another model");
        assert_eq!(refused(err), error_code::INVALID_LAUNCH_SETTING);
        check_open(&state, &open("low")).await.expect("accepted");

        // The console's own project session pair is checked like any other setting.
        let mut changes = ConsoleChanges::default();
        changes.project_launch.insert(
            Agent::Claude,
            LaunchChange {
                model: Some(Some("unlisted".to_string())),
                effort: None,
            },
        );
        let err = update_console(&state, "console-1", changes)
            .await
            .expect_err("model not offered");
        assert_eq!(refused(err), error_code::INVALID_LAUNCH_SETTING);
    }

    /// `update_settings` changes the git workflow settings item by item, broadcasts
    /// `settings_updated` with them only when one actually changed, and refuses an unknown
    /// delivery at parsing.
    #[tokio::test]
    async fn update_settings_changes_the_git_workflow_settings_and_broadcasts_only_a_change() {
        use crate::git_workflow::{Delivery, GitWorkflowSettings};
        let (state, _dir) = crate::test_support::app_state("coordinator-git-workflow-settings");
        let request = |json: &str| {
            serde_json::from_str::<crate::protocol::Request>(json)
                .unwrap()
                .body
        };
        let mut events = state.subscribe();

        handle(
            &state,
            None,
            request(
                r#"{"type":"update_settings","git_workflow":{"delivery":"merge","revert_gate":true}}"#,
            ),
        )
        .await
        .unwrap();
        assert_eq!(
            state.store.git_workflow_settings().unwrap(),
            GitWorkflowSettings {
                delivery: Delivery::Merge,
                revert_gate: true,
                ..GitWorkflowSettings::default()
            }
        );
        assert!(matches!(
            events.try_recv(),
            Ok(Event::SettingsUpdated { settings })
                if settings.git_workflow.delivery == Delivery::Merge && settings.git_workflow.revert_gate
        ));

        for unchanged in [
            r#"{"type":"update_settings","git_workflow":{"delivery":"merge"}}"#,
            r#"{"type":"update_settings","git_workflow":{}}"#,
            r#"{"type":"update_settings"}"#,
        ] {
            handle(&state, None, request(unchanged)).await.unwrap();
            assert!(events.try_recv().is_err(), "{unchanged}");
        }

        assert!(serde_json::from_str::<crate::protocol::Request>(
            r#"{"type":"update_settings","git_workflow":{"delivery":"squash"}}"#
        )
        .is_err());
    }

    /// A project added with its own git workflow values stores them, the items it leaves out
    /// following the application-wide value, and a parent directory gives the same values to
    /// every project it adds.
    #[tokio::test]
    async fn add_project_stores_its_own_git_workflow_values_for_every_project_added() {
        use crate::git_workflow::{Delivery, Development, GitWorkflowOverride};
        let (state, dir) = crate::test_support::app_state("coordinator-add-project-git-workflow");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        let own = GitWorkflowOverride {
            development: Some(Development::InPlace),
            delivery: Some(Delivery::PullRequest),
            ..GitWorkflowOverride::default()
        };
        let add = |source: &str, path: &Path| {
            let path = serde_json::to_string(&path.to_string_lossy()).unwrap();
            let request = serde_json::from_str::<crate::protocol::Request>(&format!(
                r#"{{"type":"add_project","console_id":"console-1","source":"{source}","path":{path},
                    "git_workflow":{{"development":"in_place","delivery":"pull_request"}}}}"#
            ))
            .unwrap();
            handle(&state, None, request.body)
        };

        let single = dir.join("single");
        std::fs::create_dir(&single).unwrap();
        add("local", &single).await.unwrap();
        let added = state.store.list_projects().unwrap();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].git_workflow, own);

        let parent = dir.join("parent");
        for name in ["one", "two"] {
            std::fs::create_dir_all(parent.join(name).join(".git")).unwrap();
        }
        add("parent", &parent).await.unwrap();
        let added = state.store.list_projects().unwrap();
        assert_eq!(added.len(), 3);
        assert!(added.iter().all(|project| project.git_workflow == own));
    }

    /// `update_project` sets a project's own git workflow items on a value, leaves the override
    /// alone when `git_workflow` is absent or empty (an unrelated change included), and goes back
    /// to following the application-wide value on `null` for an item; the first change broadcasts
    /// the project.
    #[tokio::test]
    async fn a_project_git_workflow_override_is_left_set_or_cleared_by_an_update() {
        use crate::git_workflow::{Delivery, Development, GitWorkflowOverride};
        let (state, dir) =
            crate::test_support::app_state("coordinator-update-project-git-workflow");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        state
            .store
            .insert_project(&project_of("console-1", "project-1"))
            .unwrap();
        let stored = || {
            state
                .store
                .get_project("project-1")
                .unwrap()
                .unwrap()
                .git_workflow
        };
        let update = |json: &str| {
            serde_json::from_str::<crate::protocol::Request>(&format!(
                r#"{{"type":"update_project","project":"project-1",{json}}}"#
            ))
            .unwrap()
            .body
        };
        let mut events = state.subscribe();

        handle(
            &state,
            None,
            update(r#""git_workflow":{"development":"in_place","delivery":"pull_request"}"#),
        )
        .await
        .unwrap();
        let expected = GitWorkflowOverride {
            development: Some(Development::InPlace),
            rebase_before_merge: None,
            delivery: Some(Delivery::PullRequest),
            revert_gate: None,
        };
        assert_eq!(stored(), expected);
        assert!(matches!(
            events.try_recv(),
            Ok(Event::ProjectUpserted { project }) if project.git_workflow == expected
        ));

        handle(&state, None, update(r#""pinned":true"#))
            .await
            .unwrap();
        assert!(
            state
                .store
                .get_project("project-1")
                .unwrap()
                .unwrap()
                .pinned
        );
        assert_eq!(
            stored(),
            expected,
            "an unrelated change leaves the override"
        );
        handle(&state, None, update(r#""git_workflow":{}"#))
            .await
            .unwrap();
        assert_eq!(stored(), expected);

        handle(
            &state,
            None,
            update(r#""git_workflow":{"delivery":null,"revert_gate":true}"#),
        )
        .await
        .unwrap();
        assert_eq!(
            stored(),
            GitWorkflowOverride {
                development: Some(Development::InPlace),
                revert_gate: Some(true),
                ..GitWorkflowOverride::default()
            }
        );

        assert!(serde_json::from_str::<crate::protocol::Request>(
            r#"{"type":"update_project","project":"p","git_workflow":{"delivery":"squash"}}"#
        )
        .is_err());
    }

    /// A project's description is left alone by an update that omits it, stored trimmed when
    /// given, and cleared by `null` or a blank string.
    #[tokio::test]
    async fn a_project_description_is_left_set_or_cleared_by_an_update() {
        let (state, dir) = crate::test_support::app_state("coordinator-update-project-description");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        state
            .store
            .insert_project(&project_of("console-1", "project-1"))
            .unwrap();
        let stored = || {
            state
                .store
                .get_project("project-1")
                .unwrap()
                .unwrap()
                .description
        };
        let update = |description: Option<Option<&str>>| {
            update_project_request("project-1", move |request| {
                let RequestBody::UpdateProject {
                    description: slot, ..
                } = request
                else {
                    unreachable!()
                };
                *slot = description.map(|text| text.map(str::to_string));
            })
        };

        handle(&state, None, update(Some(Some("  Billing.  "))))
            .await
            .unwrap();
        assert_eq!(stored().as_deref(), Some("Billing."));

        handle(&state, None, update(None)).await.unwrap();
        assert_eq!(stored().as_deref(), Some("Billing."));

        handle(&state, None, update(Some(None))).await.unwrap();
        assert_eq!(stored(), None);

        handle(&state, None, update(Some(Some("Billing."))))
            .await
            .unwrap();
        handle(&state, None, update(Some(Some(" \n "))))
            .await
            .unwrap();
        assert_eq!(stored(), None);
    }

    /// An `update_project` request that changes nothing, then `edit` applied to it.
    fn update_project_request(project: &str, edit: impl FnOnce(&mut RequestBody)) -> RequestBody {
        let mut request = RequestBody::UpdateProject {
            project: project.to_string(),
            name: None,
            default_agent: None,
            claude_account_id: None,
            codex_account_id: None,
            grok_account_id: None,
            claude_session_model: None,
            claude_session_effort: None,
            codex_session_model: None,
            codex_session_effort: None,
            grok_session_model: None,
            grok_session_effort: None,
            pinned: None,
            tags: None,
            description: None,
            git_workflow: None,
        };
        edit(&mut request);
        request
    }

    /// A project's model and effort are left alone when absent, set (trimmed) by a value, and
    /// cleared by `null` or a blank string; each agent's is independent.
    #[tokio::test]
    async fn a_project_launch_setting_is_left_set_or_cleared_by_an_update() {
        let (state, dir) = crate::test_support::app_state("coordinator-update-project-launch");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        let mut seeded = project_of("console-1", "project-1");
        seeded.claude_session_model = Some("opus".to_string());
        seeded.grok_session_effort = Some("high".to_string());
        seeded.codex_session_model = Some("gpt-5".to_string());
        state.store.insert_project(&seeded).unwrap();
        let stored = || state.store.get_project("project-1").unwrap().unwrap();

        handle(&state, None, update_project_request("project-1", |_| {}))
            .await
            .unwrap();
        assert_eq!(stored().claude_session_model.as_deref(), Some("opus"));

        handle(
            &state,
            None,
            update_project_request("project-1", |request| {
                let RequestBody::UpdateProject {
                    claude_session_model,
                    claude_session_effort,
                    grok_session_effort,
                    codex_session_model,
                    ..
                } = request
                else {
                    unreachable!()
                };
                *claude_session_model = Some(Some(" sonnet ".to_string()));
                *claude_session_effort = Some(Some("low".to_string()));
                *grok_session_effort = Some(None);
                *codex_session_model = Some(Some("  ".to_string()));
            }),
        )
        .await
        .unwrap();
        let updated = stored();
        assert_eq!(updated.claude_session_model.as_deref(), Some("sonnet"));
        assert_eq!(updated.claude_session_effort.as_deref(), Some("low"));
        assert_eq!(updated.grok_session_effort, None);
        assert_eq!(updated.codex_session_model, None);
    }

    /// A project's account reference is checked against the agent it is set for, set or cleared by
    /// an update, and left alone when absent; a refusal changes nothing.
    #[tokio::test]
    async fn a_project_account_reference_is_checked_set_and_cleared_by_an_update() {
        let (state, dir) = crate::test_support::app_state("coordinator-update-project-account");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        state
            .store
            .insert_project(&project_of("console-1", "project-1"))
            .unwrap();
        for (id, agent) in [("claude-acct", Agent::Claude), ("codex-acct", Agent::Codex)] {
            state
                .store
                .insert_account(&Account {
                    id: id.to_string(),
                    agent,
                    name: id.to_string(),
                    config_dir: format!("/home/u/.{id}"),
                    share_default_config: false,
                    share_default_config_incomplete: false,
                    disabled: false,
                })
                .unwrap();
        }
        let stored = || state.store.get_project("project-1").unwrap().unwrap();

        handle(
            &state,
            None,
            update_project_request("project-1", |request| {
                let RequestBody::UpdateProject {
                    claude_account_id, ..
                } = request
                else {
                    unreachable!()
                };
                *claude_account_id = Some(Some(pins("claude-acct")));
            }),
        )
        .await
        .unwrap();
        assert_eq!(stored().claude_account_id, pins("claude-acct"));

        // An account of another agent is refused, and the rest of the same request with it.
        let err = handle(
            &state,
            None,
            update_project_request("project-1", |request| {
                let RequestBody::UpdateProject {
                    name,
                    grok_account_id,
                    ..
                } = request
                else {
                    unreachable!()
                };
                *name = Some("Renamed".to_string());
                *grok_account_id = Some(Some(pins("codex-acct")));
            }),
        )
        .await
        .expect_err("refused");
        let coded = err.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::UNKNOWN_ACCOUNT);
        assert_eq!(stored().name, "project-1");
        assert_eq!(stored().grok_account_id, ProjectAccount::Inherit);

        handle(&state, None, update_project_request("project-1", |_| {}))
            .await
            .unwrap();
        assert_eq!(stored().claude_account_id, pins("claude-acct"));

        // The agent's default account is stored as such, and needs no account to exist.
        handle(
            &state,
            None,
            update_project_request("project-1", |request| {
                let RequestBody::UpdateProject {
                    claude_account_id, ..
                } = request
                else {
                    unreachable!()
                };
                *claude_account_id = Some(Some(ProjectAccount::Default));
            }),
        )
        .await
        .unwrap();
        assert_eq!(stored().claude_account_id, ProjectAccount::Default);
        // Left alone by a request that does not mention it, then named again from the default.
        handle(&state, None, update_project_request("project-1", |_| {}))
            .await
            .unwrap();
        assert_eq!(stored().claude_account_id, ProjectAccount::Default);
        handle(
            &state,
            None,
            update_project_request("project-1", |request| {
                let RequestBody::UpdateProject {
                    claude_account_id, ..
                } = request
                else {
                    unreachable!()
                };
                *claude_account_id = Some(Some(pins("claude-acct")));
            }),
        )
        .await
        .unwrap();
        assert_eq!(stored().claude_account_id, pins("claude-acct"));

        handle(
            &state,
            None,
            update_project_request("project-1", |request| {
                let RequestBody::UpdateProject {
                    claude_account_id, ..
                } = request
                else {
                    unreachable!()
                };
                *claude_account_id = Some(None);
            }),
        )
        .await
        .unwrap();
        assert_eq!(stored().claude_account_id, ProjectAccount::Inherit);

        // An explicit inherit is the same as null, from the default as from an account.
        for before in [ProjectAccount::Default, pins("claude-acct")] {
            let mut project = stored();
            project.claude_account_id = before;
            state.store.update_project(&project).unwrap();
            handle(
                &state,
                None,
                update_project_request("project-1", |request| {
                    let RequestBody::UpdateProject {
                        claude_account_id, ..
                    } = request
                    else {
                        unreachable!()
                    };
                    *claude_account_id = Some(Some(ProjectAccount::Inherit));
                }),
            )
            .await
            .unwrap();
            assert_eq!(stored().claude_account_id, ProjectAccount::Inherit);
        }
    }

    /// A project is added with the settings it was given, a model or effort trimmed and a blank
    /// one read as unset, and an account of another agent is refused before anything is stored.
    #[tokio::test]
    async fn adding_a_project_stores_its_settings_and_refuses_a_mismatched_account() {
        let (state, dir) = crate::test_support::app_state("coordinator-add-project-settings");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        state
            .store
            .insert_account(&Account {
                id: "codex-acct".to_string(),
                agent: Agent::Codex,
                name: "Codex".to_string(),
                config_dir: "/home/u/.codex-alt".to_string(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
        let repo = dir.join("repo");
        std::fs::create_dir(&repo).expect("repo");
        let request = |settings: NewProjectSettings| AddProjectRequest {
            console_id: "console-1".to_string(),
            source: ProjectSource::Local,
            path: Some(repo.to_string_lossy().into_owned()),
            remote_url: None,
            name: None,
            default_agent: None,
            settings,
            detect_default_agent: false,
            tags: None,
            description: None,
            check_launch: true,
            check_text_limits: true,
        };
        // What Grok Build offers, so the effort below is checked against this and not against
        // whatever is installed on the machine running the test.
        crate::test_support::offer_models(&state, Agent::Grok, &[("grok-4", &["low", "high"])]);

        let err = add_project(
            &state,
            request(NewProjectSettings {
                claude_account_id: pins("codex-acct"),
                ..NewProjectSettings::default()
            }),
        )
        .await
        .expect_err("refused");
        let coded = err.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::UNKNOWN_ACCOUNT);
        assert!(state.store.list_projects().unwrap().is_empty());

        let added = add_project(
            &state,
            request(NewProjectSettings {
                codex_account_id: pins("codex-acct"),
                grok_account_id: ProjectAccount::Default,
                claude_session_model: Some(" opus ".to_string()),
                claude_session_effort: Some("  ".to_string()),
                grok_session_effort: Some("high".to_string()),
                ..NewProjectSettings::default()
            }),
        )
        .await
        .expect("added")
        .remove(0);
        let stored = state.store.get_project(&added.id).unwrap().unwrap();
        assert_eq!(stored.codex_account_id, pins("codex-acct"));
        assert_eq!(stored.grok_account_id, ProjectAccount::Default);
        assert_eq!(stored.claude_account_id, ProjectAccount::Inherit);
        assert_eq!(stored.claude_session_model.as_deref(), Some("opus"));
        assert_eq!(stored.claude_session_effort, None);
        assert_eq!(stored.grok_session_effort.as_deref(), Some("high"));
    }

    /// Removing an account publishes the projects whose reference it cleared, and only those.
    #[tokio::test]
    async fn removing_an_account_publishes_only_the_projects_it_cleared() {
        let (state, _dir) =
            crate::test_support::app_state("coordinator-account-delete-publishes-projects");
        handle(
            &state,
            None,
            create_account_request(Agent::Claude, "Work", "/home/u/.claude-work"),
        )
        .await
        .expect("created");
        let account_id = only_account(&state).id;

        let mut referring = console(Agent::Claude, Path::new("/tmp/unused"));
        referring.id = "console-referring".to_string();
        state.store.insert_console(&referring).unwrap();
        let mut referring_project = project_of("console-referring", "project-referring");
        referring_project.claude_account_id = pins(&account_id);
        state.store.insert_project(&referring_project).unwrap();
        let mut defaulting_project = project_of("console-referring", "project-defaulting");
        defaulting_project.claude_account_id = ProjectAccount::Default;
        state.store.insert_project(&defaulting_project).unwrap();
        let inheriting_project = project_of("console-referring", "project-inheriting");
        state.store.insert_project(&inheriting_project).unwrap();
        let mut events = state.subscribe();

        handle(
            &state,
            None,
            RequestBody::DeleteAccount {
                account: account_id,
                trash_directory: false,
            },
        )
        .await
        .expect("removed");

        let mut broadcast_projects = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let Event::ProjectUpserted { project } = event {
                broadcast_projects.push((project.id, project.claude_account_id));
            }
        }
        assert_eq!(
            broadcast_projects,
            [("project-referring".to_string(), ProjectAccount::Inherit)],
            "the cleared project is published, and only it"
        );
    }

    fn code_of(err: &anyhow::Error) -> &'static str {
        err.downcast_ref::<CodedError>()
            .expect("a coded error")
            .code
    }

    fn shell_env(dir: &Path) -> impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static {
        let env = HashMap::from([(
            "CLAUDE_CONFIG_DIR".to_string(),
            dir.join("default-claude").to_string_lossy().into_owned(),
        )]);
        move || Ok(env)
    }

    /// A conversation to resume is refused before anything is launched or written, each for its
    /// own code; blank counts as none and the id is kept trimmed.
    #[tokio::test]
    async fn a_conversation_to_resume_is_checked_before_a_session_is_opened() {
        const ID: &str = "11111111-2222-3333-4444-555555555555";
        let (state, _dir) = console_with_project_and_accounts("coordinator-resume-id-checked");
        let mut holder = project_session("holder", Agent::Claude, SessionStatus::Archived);
        holder.agent_session_id = Some(ID.to_string());
        state.store.insert_session(&holder).unwrap();
        let check = |role, project: Option<&str>, bound_to: Option<&str>, id: &str| {
            checked_resume_id(&state, role, project, bound_to, Some(id.to_string()))
        };

        // (what is refused, the result, the code it is refused with)
        let cases = [
            (
                "a console session",
                check(Role::Console, None, None, ID),
                error_code::RESUME_FOR_CONSOLE_SESSION,
            ),
            (
                "a session that reports to another",
                check(Role::Project, Some("project-1"), Some("hub"), ID),
                error_code::CONFLICTING_FIELDS,
            ),
            (
                "an id that is not a UUID",
                check(Role::Project, Some("project-1"), None, "my-title"),
                error_code::RESUME_ID_INVALID,
            ),
            (
                "an id a session of the project holds",
                check(Role::Project, Some("project-1"), None, ID),
                error_code::CONVERSATION_IN_SESSION,
            ),
            (
                "an id a session of another project holds",
                check(Role::Project, Some("project-2"), None, ID),
                error_code::CONVERSATION_OUTSIDE_PROJECT,
            ),
        ];
        for (what, result, code) in cases {
            assert_eq!(code_of(&result.expect_err(what)), code, "{what}");
        }
        // The session already holding it is named by id, which the UI words as its current title.
        let err = check(Role::Project, Some("project-1"), None, ID).unwrap_err();
        assert_eq!(
            err.downcast_ref::<CodedError>().unwrap().params["session"],
            "holder"
        );
        let err = check(Role::Project, Some("project-2"), None, ID).unwrap_err();
        assert_eq!(
            err.downcast_ref::<CodedError>().unwrap().params["session"],
            "holder"
        );
        let free = "99999999-2222-3333-4444-555555555555";
        assert_eq!(
            check(
                Role::Project,
                Some("project-1"),
                None,
                &format!(" {} \n", free.to_uppercase())
            )
            .unwrap(),
            Some(free.to_string())
        );
        assert_eq!(check(Role::Console, None, None, "  ").unwrap(), None);

        // Through `open_session`, where the refusal leaves no record of the one to open.
        let err = open_session(
            &state,
            OpenRequest {
                console_id: "console-1".to_string(),
                project_id: Some("project-1".to_string()),
                agent: None,
                account: None,
                model: None,
                effort: None,
                task: None,
                title: None,
                origin: Origin::User,
                bound_to: None,
                resume_agent_session_id: Some("my-title".to_string()),
                session_id: None,
                terminal_appearance: None,
            },
        )
        .await
        .expect_err("refused");
        assert_eq!(code_of(&err), error_code::RESUME_ID_INVALID);
        assert_eq!(state.store.list_sessions().unwrap().len(), 1);
    }

    /// The conversation to resume is looked for in the directory the session will launch with: the
    /// pinned one, else the default account's from the shell snapshot.
    #[test]
    fn a_conversation_to_resume_must_be_in_the_accounts_directory() {
        const ID: &str = "11111111-2222-3333-4444-555555555555";
        let dir = temp_dir("resume-conversation-located");
        let (pinned, default) = (dir.join("pinned"), dir.join("default-claude"));
        let no_snapshot = || panic!("a pinned account needs no snapshot");
        let snapshot = shell_env(&dir);

        let err = locate_conversation_to_resume(
            Agent::Claude,
            Some(&pinned.to_string_lossy()),
            ID,
            no_snapshot,
        )
        .expect_err("not there");
        let coded = err.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::CONVERSATION_TO_RESUME_NOT_FOUND);
        assert_eq!(coded.params["id"], ID);

        let record = default.join("projects/slug").join(format!("{ID}.jsonl"));
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        std::fs::write(&record, "").unwrap();
        locate_conversation_to_resume(Agent::Claude, None, ID, snapshot).unwrap();
    }

    /// The conversation to resume is placed in the account whose record was written to last, in the
    /// default account and the pinned ones of every agent offered, and nowhere when none holds it.
    #[test]
    fn a_conversation_to_resume_is_located_in_the_account_written_to_last() {
        const ID: &str = "11111111-2222-3333-4444-555555555555";
        let dir = temp_dir("resume-conversation-found");
        let account = |id: &str, agent, dir: PathBuf| Account {
            id: id.to_string(),
            agent,
            name: id.to_string(),
            config_dir: dir.to_string_lossy().into_owned(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
        };
        let (work, other) = (dir.join("work"), dir.join("other"));
        let accounts = [
            account("work", Agent::Claude, work.clone()),
            account("other", Agent::Claude, other.clone()),
        ];
        let write_record = |config_dir: &Path, seconds: u64| {
            let record = config_dir.join("projects/slug").join(format!("{ID}.jsonl"));
            std::fs::create_dir_all(record.parent().unwrap()).unwrap();
            std::fs::write(&record, "").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&record)
                .unwrap()
                .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
                .unwrap();
        };
        let find = |agents: &[Agent]| {
            find_conversation_to_resume(agents, &accounts, ID, shell_env(&dir)).unwrap()
        };
        let in_account = |agent, account: &str| {
            Some(ConversationLocation::Record {
                agent,
                account: Some(account.to_string()),
            })
        };

        assert_eq!(find(&[Agent::Claude, Agent::Grok]), None);

        write_record(&other, 100);
        assert_eq!(find(&[Agent::Claude]), in_account(Agent::Claude, "other"));
        assert_eq!(
            find(&[Agent::Codex, Agent::Grok]),
            None,
            "only the agents offered"
        );

        write_record(&work, 200);
        assert_eq!(find(&[Agent::Claude]), in_account(Agent::Claude, "work"));

        // The default account's directory comes from the snapshot, which only a lookup that has a
        // default account to look in takes.
        write_record(&dir.join("default-claude"), 300);
        assert_eq!(
            find(&[Agent::Claude]),
            Some(ConversationLocation::Record {
                agent: Agent::Claude,
                account: None,
            })
        );

        // A shell that cannot be snapshotted leaves the default accounts out, not the stored ones;
        // it is the failure only when nothing was found.
        let no_shell = || -> Result<HashMap<String, String>> { anyhow::bail!("no login shell") };
        assert_eq!(
            find_conversation_to_resume(&[Agent::Claude], &accounts, ID, no_shell).unwrap(),
            in_account(Agent::Claude, "work"),
        );
        let other_id = "99999999-2222-3333-4444-555555555555";
        let err = find_conversation_to_resume(&[Agent::Claude], &accounts, other_id, no_shell)
            .expect_err("nothing found, and the default account unknown");
        assert!(format!("{err:#}").contains("no login shell"));

        // A Grok record in another agent's directory layout, written later still.
        let grok = dir.join("grok-home");
        let grok_account = account("grok", Agent::Grok, grok.clone());
        std::fs::create_dir_all(grok.join("sessions/cwd").join(ID)).unwrap();
        let both = [accounts[0].clone(), grok_account];
        assert_eq!(
            find_conversation_to_resume(&[Agent::Claude, Agent::Grok], &both, ID, shell_env(&dir))
                .unwrap(),
            in_account(Agent::Grok, "grok"),
        );
    }

    /// `locate_conversation_to_resume` names the session that records the conversation, by whose
    /// it is and what state it is in, before any record on disk is looked at; an id that is no UUID
    /// and a project that does not exist are refused, and a conversation nobody holds is not found.
    #[tokio::test]
    async fn locating_a_conversation_to_resume_names_the_session_that_holds_it() {
        const ID: &str = "11111111-2222-3333-4444-555555555555";
        let (state, _dir) = console_with_project_and_accounts("coordinator-locate-held");
        let locate = |project: &str, id: &str| {
            handle(
                &state,
                Some("req-1".to_string()),
                RequestBody::LocateConversationToResume {
                    project: project.to_string(),
                    agent_session_id: id.to_string(),
                },
            )
        };
        let located = |reply: Option<Event>| match reply {
            Some(Event::ConversationLocated { id, location }) => {
                assert_eq!(id.as_deref(), Some("req-1"));
                location
            }
            other => panic!("expected conversation_located, got {other:?}"),
        };
        let mut holder = project_session("holder", Agent::Claude, SessionStatus::Archived);
        holder.agent_session_id = Some(ID.to_string());
        state.store.insert_session(&holder).unwrap();
        let project = holder.project_id.clone().expect("a project session");
        let mut other = state.store.get_project(&project).unwrap().unwrap();
        other.id = "project-2".to_string();
        other.path.push_str("-2");
        state.store.insert_project(&other).unwrap();

        // Upper case, as the user may type it: held all the same.
        let archived = located(locate(&project, &ID.to_uppercase()).await.unwrap());
        assert_eq!(
            archived,
            ConversationLocation::ArchivedSession {
                session: "holder".to_string()
            }
        );

        for (status, project_id, expected) in [
            (
                SessionStatus::Idle,
                Some(project.as_str()),
                ConversationLocation::LiveSession {
                    session: "holder".to_string(),
                },
            ),
            (
                SessionStatus::Archived,
                Some("project-2"),
                ConversationLocation::OutsideProject {
                    session: "holder".to_string(),
                },
            ),
            (
                SessionStatus::Idle,
                None,
                ConversationLocation::OutsideProject {
                    session: "holder".to_string(),
                },
            ),
        ] {
            let mut moved = holder.clone();
            moved.status = status;
            moved.project_id = project_id.map(str::to_string);
            state.store.delete_session("holder").unwrap();
            state.store.insert_session(&moved).unwrap();
            assert_eq!(located(locate(&project, ID).await.unwrap()), expected);
        }

        let err = locate(&project, "my-title").await.expect_err("refused");
        assert_eq!(code_of(&err), error_code::RESUME_ID_INVALID);
        let err = locate("no-such-project", ID).await.expect_err("refused");
        assert_eq!(code_of(&err), error_code::UNKNOWN_PROJECT);
    }

    /// Every refusal that can be told before the process is ended names its reason and records
    /// nothing: the session keeps its account, its directory and its status, and a Grok home that
    /// was never initialized is not given a `sessions/` directory by the refused attempt.
    #[tokio::test]
    async fn a_refused_switch_records_nothing() {
        let (state, dir) = console_with_project_and_accounts("coordinator-switch-refused");
        std::fs::create_dir_all(dir.join("grok-a")).unwrap();

        // (what is refused, the session, the request's account, the code it is refused with)
        let archived = project_session("s", Agent::Claude, SessionStatus::Archived);
        let mut with_conversation = project_session("s", Agent::Claude, SessionStatus::Interrupted);
        with_conversation.has_conversation = true;
        with_conversation.agent_session_id = Some("agent-1".to_string());
        let cases = [
            (
                "an account that does not exist",
                project_session("s", Agent::Claude, SessionStatus::Interrupted),
                Some(Some("nope".to_string())),
                error_code::UNKNOWN_ACCOUNT,
            ),
            (
                "another agent's account",
                project_session("s", Agent::Claude, SessionStatus::Interrupted),
                Some(Some("codex-a".to_string())),
                error_code::UNKNOWN_ACCOUNT,
            ),
            (
                "the account it is on",
                project_session("s", Agent::Claude, SessionStatus::Interrupted),
                Some(None),
                error_code::SESSION_ALREADY_ON_ACCOUNT,
            ),
            (
                "no account named at all",
                project_session("s", Agent::Claude, SessionStatus::Interrupted),
                None,
                error_code::FIELD_REQUIRED,
            ),
            (
                "an archived session",
                archived,
                Some(Some("claude-a".to_string())),
                error_code::SESSION_ARCHIVED,
            ),
            (
                "a conversation that is not in its account",
                with_conversation,
                Some(Some("claude-a".to_string())),
                error_code::CONVERSATION_NOT_FOUND,
            ),
            (
                "a Grok home that was never initialized",
                project_session("s", Agent::Grok, SessionStatus::Interrupted),
                Some(Some("grok-a".to_string())),
                error_code::GROK_HOME_NOT_INITIALIZED,
            ),
        ];
        for (why, session, account, code) in cases {
            state.store.delete_session("s").unwrap();
            state.store.insert_session(&session).unwrap();

            let err = switch_session_account(&state, "s", account, None, shell_env(&dir))
                .await
                .expect_err(why);
            assert_eq!(code_of(&err), code, "{why}");

            let after = state.store.get_session("s").unwrap().unwrap();
            assert_eq!(after.account_id, session.account_id, "{why}");
            assert_eq!(after.config_dir, session.config_dir, "{why}");
            assert_eq!(after.status, session.status, "{why}");
        }
        assert!(!dir.join("grok-a/sessions").exists());
        assert!(!dir.join("claude-a").exists());

        // A launch, resume or switch of the session under way refuses another switch of it.
        state.store.delete_session("s").unwrap();
        state
            .store
            .insert_session(&project_session(
                "s",
                Agent::Claude,
                SessionStatus::Interrupted,
            ))
            .unwrap();
        let claim = state.begin_launch("s").unwrap();
        let err = switch_session_account(
            &state,
            "s",
            Some(Some("claude-a".to_string())),
            None,
            shell_env(&dir),
        )
        .await
        .expect_err("launching");
        assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);
        drop(claim);
    }

    /// The ownership an owner's tool checked before the switch claimed the session is checked again
    /// under the claim: a session handed to another owner meanwhile is refused and left alone.
    #[tokio::test]
    async fn a_session_handed_to_another_owner_meanwhile_is_not_switched() {
        let (state, dir) = console_with_project_and_accounts("coordinator-switch-handed-over");
        let mut worker = bound_session("worker", SessionStatus::Idle, "hub");
        worker.has_conversation = true;
        state.store.insert_session(&worker).unwrap();
        let live = idle_stand_in("worker");
        state.register_live(live.clone());
        state.watch_exit(live.clone());

        let err = switch_session_account_with(
            &state,
            "worker",
            Some(Some("claude-a".to_string())),
            Some("other-hub"),
            None,
            LaunchEdit::default(),
            shell_env(&dir),
            |_| panic!("a refused switch launches nothing"),
        )
        .await
        .expect_err("no longer the asker's");

        assert!(err.to_string().contains("no longer yours"), "{err}");
        assert!(!live.poll_exit(), "the process was left running");
        let after = state.store.get_session("worker").unwrap().unwrap();
        assert_eq!(after.status, SessionStatus::Idle);
        assert_eq!((after.account_id, after.config_dir), (None, None));
    }

    /// The default account resolves from one shell snapshot, taken once, and that same snapshot is
    /// what the relaunch is handed; an account with a directory of its own needs none. A session
    /// with no conversation has nothing to relocate whichever side is the default.
    #[test]
    fn the_default_account_is_resolved_once_for_the_copy_and_the_relaunch() {
        let dir = temp_dir("switch-prepare");
        let default_dir = dir.join("default-claude");
        let record = default_dir.join("projects/slug/agent-1.jsonl");
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        std::fs::write(&record, "conversation").unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let snapshot = || {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(HashMap::from([(
                "CLAUDE_CONFIG_DIR".to_string(),
                default_dir.to_string_lossy().into_owned(),
            )]))
        };
        let target = dir.join("claude-a").to_string_lossy().into_owned();

        let plan = prepare_switch(
            Agent::Claude,
            Some("agent-1"),
            None,
            Some(&target),
            snapshot,
        )
        .unwrap();
        let relocation = plan.relocation.expect("a conversation to relocate");
        assert_eq!(
            relocation,
            (
                default_dir.clone(),
                PathBuf::from(&target),
                PathBuf::from("projects/slug/agent-1.jsonl"),
                "agent-1".to_owned()
            )
        );
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            plan.shell_env.expect("kept for the relaunch")["CLAUDE_CONFIG_DIR"],
            default_dir.to_string_lossy()
        );

        // Both sides pinned: no default to resolve, so no snapshot.
        let plan = prepare_switch(Agent::Claude, None, Some(&target), Some(&target), || {
            panic!("no snapshot needed")
        })
        .unwrap();
        assert!(plan.shell_env.is_none() && plan.relocation.is_none());

        // The default account as the target, for a session with no conversation: still one
        // snapshot, for the launch, and nothing to copy.
        let plan = prepare_switch(Agent::Claude, None, Some(&target), None, snapshot).unwrap();
        assert!(plan.shell_env.is_some() && plan.relocation.is_none());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    /// A Codex session whose rollout Codex has compressed is planned for the copy under its
    /// `.jsonl.zst` name.
    #[test]
    fn a_compressed_codex_rollout_is_planned_for_a_switch() {
        const THREAD: &str = "019a0000-0000-7000-8000-000000000001";
        let dir = temp_dir("switch-prepare-compressed");
        let (source, target) = (dir.join("codex-a"), dir.join("codex-b"));
        let record = format!("sessions/2026/10/01/rollout-2026-10-01T01-02-03-{THREAD}.jsonl.zst");
        std::fs::create_dir_all(source.join(&record).parent().unwrap()).unwrap();
        std::fs::write(source.join(&record), "bytes").unwrap();
        let (source_dir, target_dir) = (source.to_string_lossy(), target.to_string_lossy());
        let plan = |id| {
            prepare_switch(
                Agent::Codex,
                Some(id),
                Some(&source_dir),
                Some(&target_dir),
                // Both accounts are pinned, yet a Codex switch snapshots the environment: it says
                // where the target keeps its database.
                || {
                    Ok(HashMap::from([(
                        "CODEX_SQLITE_HOME".to_owned(),
                        "db".to_owned(),
                    )]))
                },
            )
        };

        let planned = plan(THREAD).unwrap();
        assert_eq!(
            planned.relocation,
            Some((source, target, PathBuf::from(&record), THREAD.to_owned()))
        );
        assert_eq!(planned.shell_env.unwrap()["CODEX_SQLITE_HOME"], "db");
    }

    /// A relaunch that is refused after the conversation was copied and the account recorded puts
    /// the account back, leaves the session interrupted rather than archived, and leaves the
    /// conversation in the account it came from as well as the one it was copied to. The
    /// switched session here is a live console session, whose process is ended on the way: the
    /// session bound to it is not touched, which is what keeps a switch from ever being an
    /// archive.
    #[tokio::test]
    async fn a_switch_whose_relaunch_fails_is_back_on_its_account_and_archives_nothing() {
        let (state, dir) = console_with_project_and_accounts("coordinator-switch-relaunch-refused");
        let source = dir.join("default-claude");
        let record = "projects/slug/agent-1.jsonl";
        std::fs::create_dir_all(source.join(record).parent().unwrap()).unwrap();
        std::fs::write(source.join(record), "conversation").unwrap();

        let mut hub = bare_session("hub", Role::Console);
        hub.has_conversation = true;
        hub.agent_session_id = Some("agent-1".to_string());
        hub.status = SessionStatus::Working;
        state.store.insert_session(&hub).unwrap();
        let mut bound = project_session("bound", Agent::Claude, SessionStatus::Interrupted);
        bound.bound_to = Some("hub".to_string());
        state.store.insert_session(&bound).unwrap();
        let live = idle_stand_in("hub");
        state.register_live(live.clone());
        state.watch_exit(live.clone());

        let err = switch_session_account(
            &state,
            "hub",
            Some(Some("claude-a".into())),
            None,
            shell_env(&dir),
        )
        .await
        .expect_err("the console's working directory does not exist");
        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);

        assert!(live.poll_exit(), "its process was ended");
        let after = state.store.get_session("hub").unwrap().unwrap();
        assert_eq!(after.status, SessionStatus::Interrupted);
        assert_eq!((after.account_id, after.config_dir), (None, None));
        assert_eq!(
            state.store.get_session("bound").unwrap().unwrap().status,
            SessionStatus::Interrupted
        );
        assert!(source.join(record).exists(), "copied, not moved");
        assert_eq!(
            std::fs::read_to_string(dir.join("claude-a").join(record)).unwrap(),
            "conversation"
        );
    }

    /// A switch of a reverted Codex thread carries the whole chain into the target and points the
    /// target's database row of the thread, which would otherwise name an older head, at the head;
    /// a target whose database cannot be updated fails the switch before any relaunch and leaves
    /// the session on its account.
    #[tokio::test]
    async fn a_codex_switch_carries_the_chain_and_repoints_the_targets_stale_row() {
        use rusqlite::Connection;
        let (state, dir) = console_with_project_and_accounts("coordinator-switch-codex-chain");
        let (source, target) = (dir.join("codex-old"), dir.join("codex-a"));
        let broken = dir.join("codex-b");
        state
            .store
            .insert_account(&Account {
                id: "codex-b".to_string(),
                agent: Agent::Codex,
                name: "codex-b".to_string(),
                config_dir: broken.to_string_lossy().into_owned(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
        let id = "019a0000-0000-7000-8000-000000000001";
        let chain = crate::test_support::codex_chain(&source, id, false);
        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(
            broken.join("state_5.sqlite"),
            "not a database at all, but long",
        )
        .unwrap();
        let database = target.join("state_5.sqlite");
        Connection::open(&database)
            .unwrap()
            .execute_batch(&format!(
                "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT);
                 INSERT INTO threads VALUES ('{id}', 'older head');"
            ))
            .unwrap();
        let mut codex = project_session("codex", Agent::Codex, SessionStatus::Idle);
        codex.has_conversation = true;
        codex.agent_session_id = Some(id.to_string());
        codex.config_dir = Some(source.to_string_lossy().into_owned());
        state.store.insert_session(&codex).unwrap();
        let live = idle_stand_in("codex");
        state.register_live(live.clone());
        state.watch_exit(live.clone());
        let switch = |account: &str| {
            switch_session_account(
                &state,
                "codex",
                Some(Some(account.into())),
                None,
                shell_env(&dir),
            )
        };

        let err = switch("codex-b")
            .await
            .expect_err("the target's database is no database");
        assert_eq!(code_of(&err), error_code::RELOCATION_FAILED);
        assert!(live.poll_exit(), "its process was ended");
        assert!(
            state.live_session("codex").is_none(),
            "nothing was relaunched"
        );
        let after = state.store.get_session("codex").unwrap().unwrap();
        assert_eq!(after.status, SessionStatus::Interrupted);
        assert_eq!(after.config_dir, codex.config_dir, "left on its account");
        assert!(after.account_id.is_none());

        // The relaunch is refused here for the missing project directory, after the copy.
        let err = switch("codex-a")
            .await
            .expect_err("the project directory does not exist");
        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
        let named: String = Connection::open(&database)
            .unwrap()
            .query_row("SELECT rollout_path FROM threads", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            Path::new(&named),
            target.join(chain.head.strip_prefix(&source).unwrap())
        );
        for file in chain.all() {
            let relative = file.strip_prefix(&source).unwrap();
            assert!(
                target.join(relative).is_file(),
                "{relative:?} was not carried"
            );
        }
    }

    /// The other half of the seam: ending a console session's process for a switch is not refused
    /// because sessions bound to it are running. That refusal belongs to archiving alone, so a
    /// switch of a console session with a working bound session goes on to the relaunch, which
    /// fails here only on the missing working directory, and leaves the bound session running.
    #[tokio::test]
    async fn a_switch_is_not_refused_for_a_running_bound_session() {
        let (state, dir) = console_with_project_and_accounts("coordinator-switch-running-bound");
        state
            .store
            .insert_session(&console_session("hub", SessionStatus::Working))
            .unwrap();
        state
            .store
            .insert_session(&bound_session("worker", SessionStatus::Working, "hub"))
            .unwrap();
        let hub = idle_stand_in("hub");
        let worker = idle_stand_in("worker");
        state.register_live(hub.clone());
        state.watch_exit(hub.clone());
        state.register_live(worker.clone());

        let err = switch_session_account(
            &state,
            "hub",
            Some(Some("claude-a".into())),
            None,
            shell_env(&dir),
        )
        .await
        .expect_err("the console's working directory does not exist");

        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
        assert!(hub.poll_exit(), "its process was ended");
        assert!(!worker.poll_exit(), "the bound session was left running");
        assert_eq!(status_of(&state, "worker"), Some(SessionStatus::Working));
    }

    /// A relaunched process that has ended by the end of the settling time did not come up; one
    /// still running did.
    #[tokio::test]
    async fn a_relaunched_process_that_ends_at_once_did_not_come_up() {
        let settle = Duration::from_millis(400);
        let gone = fake_live_session("gone", Agent::Claude, 80, 24, "exit 1");
        let up = idle_stand_in("up");

        assert!(!came_up(&gone, settle).await);
        assert!(came_up(&up, settle).await);
    }

    /// A switched session whose relaunched process ended at once is broadcast interrupted on its
    /// old account before the switch returns its error, and nothing the exit watcher publishes
    /// afterwards puts the new account back in a client's copy.
    #[tokio::test]
    async fn a_switch_that_did_not_come_up_is_restored_before_it_returns() {
        let (state, _dir) = console_with_project_and_accounts("coordinator-switch-restored-first");
        let before = project_session("s", Agent::Claude, SessionStatus::Interrupted);
        state.store.insert_session(&before).unwrap();
        state
            .set_session_account("s", Some("claude-a"), Some("/elsewhere"))
            .unwrap();
        state
            .store
            .update_session(&Session {
                status: SessionStatus::Idle,
                ..before.clone()
            })
            .unwrap();
        let mut events = state.subscribe();
        let live = fake_live_session("s", Agent::Claude, 80, 24, "exit 1");
        state.register_live(live.clone());
        state.watch_exit(live.clone());

        put_back_once_down(&state, &before).await;

        let mut last = None;
        while let Ok(event) = events.try_recv() {
            if let Event::SessionUpserted { session } = event {
                last = Some(session);
            }
        }
        let last = last.expect("the session was published");
        assert_eq!(last.status, SessionStatus::Interrupted);
        assert_eq!((last.account_id, last.config_dir), (None, None));
    }

    /// The claim a switch holds outlives the relaunched process being registered as live, and it
    /// outlives that process's exit: while the come-up verdict is pending and the account is put
    /// back, a resume (what the terminal's Resume button asks for) and a second switch are both
    /// refused.
    #[tokio::test]
    async fn a_switch_claim_is_held_after_the_relaunched_process_is_live_and_after_it_exits() {
        let (state, _dir) = crate::test_support::app_state("coordinator-switch-claim");
        let claim = state.begin_switch("s").unwrap();
        let live = fake_live_session("s", Agent::Claude, 80, 24, "exit 1");
        state.register_live(live.clone());
        state.watch_exit(live.clone());
        let deadline = Instant::now() + Duration::from_secs(5);
        while state.live_session("s").is_some() {
            assert!(Instant::now() < deadline, "the exit was never seen");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        for refused in [state.begin_launch("s"), state.begin_switch("s")] {
            let err = refused.err().expect("still claimed");
            assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);
        }
        drop(claim);
        assert!(state.begin_launch("s").is_ok());
    }

    /// Clients are shown a session as switching its account for as long as the switch's claim
    /// lasts, and the other claims on a session do not show as one.
    #[tokio::test]
    async fn a_session_shows_as_switching_its_account_while_the_switch_holds_it() {
        let (state, _dir) = console_with_project_and_accounts("coordinator-switch-shown");
        state
            .store
            .insert_session(&project_session("s", Agent::Claude, SessionStatus::Idle))
            .unwrap();
        let mut events = state.subscribe();
        let shown = |events: &mut tokio::sync::broadcast::Receiver<Event>| {
            let mut last = None;
            while let Ok(event) = events.try_recv() {
                if let Event::SessionUpserted { session } = event {
                    last = Some(session.switching_account);
                }
            }
            last
        };

        let claim = state.begin_account_switch("s", false).unwrap();
        assert!(state.is_switching_account("s"));
        assert!(
            state
                .wire_session(state.session_record("s").unwrap())
                .switching_account
        );
        assert_eq!(shown(&mut events), Some(true));
        drop(claim);
        assert!(!state.is_switching_account("s"));
        assert_eq!(shown(&mut events), Some(false));

        let claim = state.begin_switch("s").unwrap();
        assert!(
            !state
                .wire_session(state.session_record("s").unwrap())
                .switching_account
        );
        drop(claim);
    }

    /// Clients are shown a session a change of its model restarts as starting for as long as the
    /// change's claim lasts, and a mark without that claim shows nothing.
    #[tokio::test]
    async fn a_session_shows_as_starting_while_a_model_change_restarts_it() {
        let (state, _dir) = console_with_project_and_accounts("coordinator-model-change-shown");
        state
            .store
            .insert_session(&project_session("s", Agent::Claude, SessionStatus::Idle))
            .unwrap();
        let mut events = state.subscribe();
        let shown = |events: &mut tokio::sync::broadcast::Receiver<Event>| {
            let mut last = None;
            while let Ok(event) = events.try_recv() {
                if let Event::SessionUpserted { session } = event {
                    last = Some(session.status);
                }
            }
            last
        };

        state.mark_model_change("s");
        assert_eq!(shown(&mut events), None, "no claim, no mark");
        let claim = state.begin_switch("s").unwrap();
        state.mark_model_change("s");
        assert_eq!(shown(&mut events), Some(SessionStatus::Starting));
        drop(claim);
        assert!(!state.is_changing_model("s"));
        assert_eq!(shown(&mut events), Some(SessionStatus::Idle));
    }

    /// A launch that must never run, for a change that records or refuses.
    fn no_launch(_: term::LaunchRequest) -> Result<term::Launch> {
        panic!("nothing is launched")
    }

    /// A model change asks for something to change, and is refused, with nothing written, for an
    /// archived session, one being started and a value the agent does not offer; an interrupted
    /// session only has the values recorded, and values it already has change nothing.
    #[tokio::test]
    async fn a_model_change_is_checked_and_an_interrupted_session_only_records_it() {
        let (state, dir) = console_with_project_and_accounts("coordinator-model-change-checks");
        for (id, status) in [
            ("archived", SessionStatus::Archived),
            ("starting", SessionStatus::Starting),
            ("down", SessionStatus::Interrupted),
        ] {
            state
                .store
                .insert_session(&Session {
                    model: Some(Some("sonnet".to_string())),
                    ..project_session(id, Agent::Claude, status)
                })
                .unwrap();
        }
        let change = |id: &'static str, model: Option<&str>| {
            set_session_model_with(
                &state,
                id,
                LaunchEdit {
                    model: model.map(|model| Some(model.to_string())),
                    effort: None,
                },
                ModelAsker::User,
                None,
                shell_env(&dir),
                no_launch,
            )
        };

        let err = change("down", None).await.expect_err("nothing given");
        assert_eq!(code_of(&err), error_code::FIELD_REQUIRED);
        let err = change("archived", Some("opus"))
            .await
            .expect_err("archived");
        assert_eq!(code_of(&err), error_code::SESSION_ARCHIVED);
        let err = change("starting", Some("opus"))
            .await
            .expect_err("starting");
        assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);
        let err = change("down", Some("gpt-9"))
            .await
            .expect_err("not offered");
        assert_eq!(code_of(&err), error_code::INVALID_LAUNCH_SETTING);
        for id in ["archived", "starting", "down"] {
            let after = state.store.get_session(id).unwrap().unwrap();
            assert_eq!(after.model, Some(Some("sonnet".to_string())), "{id}");
        }

        assert_eq!(
            change("down", Some("sonnet")).await.unwrap(),
            ModelChange::Unchanged
        );
        assert_eq!(
            change("down", Some("opus")).await.unwrap(),
            ModelChange::Recorded
        );
        let after = state.store.get_session("down").unwrap().unwrap();
        assert_eq!(after.model, Some(Some("opus".to_string())));
        assert_eq!(after.status, SessionStatus::Interrupted);
    }

    /// The control request is answered with what the change did.
    #[tokio::test]
    async fn a_set_session_model_request_is_answered_with_its_outcome() {
        let (state, _dir) = console_with_project_and_accounts("coordinator-model-change-reply");
        crate::test_support::offer_models(&state, Agent::Claude, &[("opus", &["high"])]);
        state
            .store
            .insert_session(&project_session(
                "s",
                Agent::Claude,
                SessionStatus::Interrupted,
            ))
            .unwrap();
        let ask = || RequestBody::SetSessionModel {
            session: "s".to_string(),
            model: Some(Some("opus".to_string())),
            effort: None,
            terminal_appearance: None,
        };
        let outcome = |reply: Option<Event>| match reply {
            Some(Event::SessionModelSet {
                id,
                session,
                outcome,
                continued,
            }) => {
                assert_eq!((id.as_deref(), session.as_str()), (Some("r"), "s"));
                (outcome, continued)
            }
            other => panic!("answered with {other:?}"),
        };

        let reply = handle(&state, Some("r".to_string()), ask()).await.unwrap();
        assert_eq!(outcome(reply), (ModelChangeOutcome::Recorded, None));
        let reply = handle(&state, Some("r".to_string()), ask()).await.unwrap();
        assert_eq!(outcome(reply), (ModelChangeOutcome::Unchanged, None));
    }

    #[tokio::test]
    async fn an_unchanged_model_no_longer_in_the_catalogue_is_not_revalidated() {
        let (state, dir) = console_with_project_and_accounts("model-unlisted-noop");
        crate::test_support::offer_models(&state, Agent::Claude, &[("sonnet", &["low"])]);
        state
            .store
            .insert_session(&Session {
                model: Some(Some("retired-model".to_string())),
                effort: Some(Some("retired-effort".to_string())),
                ..project_session("s", Agent::Claude, SessionStatus::Interrupted)
            })
            .unwrap();
        let changed = set_session_model_with(
            &state,
            "s",
            LaunchEdit {
                model: Some(Some(" retired-model ".to_string())),
                effort: None,
            },
            ModelAsker::User,
            None,
            shell_env(&dir),
            no_launch,
        )
        .await
        .unwrap();
        assert_eq!(changed, ModelChange::Unchanged);
        // A changed field is checked without revalidating the unrelated legacy override.
        let changed = set_session_model_with(
            &state,
            "s",
            LaunchEdit {
                model: Some(Some("sonnet".to_string())),
                effort: None,
            },
            ModelAsker::User,
            None,
            shell_env(&dir),
            no_launch,
        )
        .await
        .unwrap();
        assert_eq!(changed, ModelChange::Recorded);
        let after = state.session_record("s").unwrap();
        assert_eq!(
            after.effort.as_ref().and_then(|value| value.as_deref()),
            Some("retired-effort")
        );
    }

    #[tokio::test]
    async fn an_interrupted_session_archived_during_model_validation_is_not_changed() {
        let (state, dir) = console_with_project_and_accounts("model-late-archive");
        crate::test_support::offer_models(&state, Agent::Claude, &[("sonnet", &["low"])]);
        state
            .store
            .insert_session(&project_session(
                "s",
                Agent::Claude,
                SessionStatus::Interrupted,
            ))
            .unwrap();
        let change = set_session_model_with(
            &state,
            "s",
            LaunchEdit {
                model: Some(Some("sonnet".to_string())),
                effort: None,
            },
            ModelAsker::User,
            None,
            shell_env(&dir),
            no_launch,
        );
        // Model validation awaits a blocking catalogue read, even for a seeded catalogue.
        let (result, ()) = tokio::join!(change, async { archive_record(&state, "s").unwrap() });
        assert_eq!(code_of(&result.unwrap_err()), error_code::SESSION_ARCHIVED);
        let after = state.session_record("s").unwrap();
        assert_eq!(after.status, SessionStatus::Archived);
        assert!(after.model.is_none());
    }

    /// A running session whose relaunch ends at once is left interrupted with the model and effort
    /// it had, on its account, with its earlier process ended; the observed model the records
    /// showed is to be read afresh.
    #[tokio::test]
    async fn a_model_change_whose_relaunch_does_not_come_up_keeps_the_model_it_had() {
        let (state, dir) = console_with_project_and_accounts("coordinator-model-change-down");
        let mut project = state.store.get_project("project-1").unwrap().unwrap();
        project.path = dir.to_string_lossy().into_owned();
        state.store.update_project(&project).unwrap();
        state
            .store
            .insert_session(&Session {
                effort: Some(Some("high".to_string())),
                ..project_session("s", Agent::Claude, SessionStatus::Idle)
            })
            .unwrap();
        let live = idle_stand_in("s");
        state.register_live(live.clone());
        state.watch_exit(live.clone());
        let launch = |request: term::LaunchRequest| {
            let gone = fake_live_session(&request.session_id, request.agent, 80, 24, "exit 1");
            Ok(term::Launch {
                session: (*gone).clone(),
                agent_session_id: None,
                notice: None,
            })
        };

        let err = set_session_model_with(
            &state,
            "s",
            LaunchEdit {
                model: Some(Some("opus".to_string())),
                effort: None,
            },
            ModelAsker::User,
            None,
            shell_env(&dir),
            launch,
        )
        .await
        .expect_err("the relaunch ended at once");

        assert_eq!(code_of(&err), error_code::MODEL_CHANGE_DID_NOT_COME_UP);
        assert!(live.poll_exit(), "its process was ended");
        let after = state.store.get_session("s").unwrap().unwrap();
        assert_eq!(after.status, SessionStatus::Interrupted);
        assert_eq!(
            (after.model, after.effort),
            (None, Some(Some("high".to_string())))
        );
        assert_eq!((after.account_id, after.config_dir), (None, None));
        assert!(!state.is_changing_model("s"));
        assert!(state.store.observed_boundary("s").unwrap().is_some());
    }

    /// An archive that lands before the relaunch writes anything is not undone by the switch: the
    /// relaunch refuses an archived session unless it is a resume reopening one, and leaves it
    /// archived.
    #[tokio::test]
    async fn a_relaunch_for_a_switch_never_reopens_an_archived_session() {
        let (state, _dir) = console_with_project_and_accounts("coordinator-switch-archived-late");
        state
            .store
            .insert_session(&project_session(
                "s",
                Agent::Claude,
                SessionStatus::Archived,
            ))
            .unwrap();
        let claim = state.begin_switch("s").unwrap();

        let err = relaunch_session_with(&state, &claim, "s", None, false, None, None, term::launch)
            .await
            .expect_err("archived");
        assert_eq!(code_of(&err), error_code::SESSION_ARCHIVED);
        assert_eq!(
            state.store.get_session("s").unwrap().unwrap().status,
            SessionStatus::Archived
        );
    }

    /// Hold the real relaunch path inside its blocking launcher while the user archives the
    /// owner. Both a late process and a late refusal must leave that archive intact.
    async fn archive_during_relaunch(switching: bool, succeeds: bool) {
        use SessionStatus::{Archived, Idle, Interrupted};
        let (state, dir) = console_with_project_and_accounts("coordinator-archive-during-relaunch");
        std::fs::create_dir_all(dir.join("claude-a")).unwrap();
        state
            .store
            .insert_session(&console_session("hub", Idle))
            .unwrap();
        state
            .store
            .insert_session(&bound_session("worker", Interrupted, "hub"))
            .unwrap();
        let (release, released) = std::sync::mpsc::channel();
        let (started_tx, started) = tokio::sync::oneshot::channel();
        let stand_in = succeeds.then(|| idle_stand_in("worker"));
        let process = stand_in.as_ref().map(|live| (*live).clone());
        let launch = move |_: term::LaunchRequest| {
            started_tx.send(()).unwrap();
            released
                .recv_timeout(crate::test_support::PATIENCE)
                .expect("release launch");
            match process {
                Some(session) => Ok(term::Launch {
                    session,
                    agent_session_id: None,
                    notice: None,
                }),
                None => Err(anyhow::anyhow!("delayed launch failure")),
            }
        };
        let running = tokio::spawn({
            let state = state.clone();
            async move {
                if switching {
                    switch_session_account_with(
                        &state,
                        "worker",
                        Some(Some("claude-a".to_string())),
                        None,
                        None,
                        LaunchEdit::default(),
                        || Ok(HashMap::new()),
                        launch,
                    )
                    .await
                    .map(drop)
                } else {
                    let claim = state.begin_launch("worker")?;
                    relaunch_session_with(
                        &state,
                        &claim,
                        "worker",
                        Some("obsolete instruction"),
                        true,
                        None,
                        None,
                        launch,
                    )
                    .await
                }
            }
        });
        tokio::time::timeout(crate::test_support::PATIENCE, started)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status_of(&state, "worker"), Some(Idle));
        assert!(state.live_session("worker").is_none());
        handle(
            &state,
            None,
            RequestBody::ArchiveSession {
                session: "hub".to_string(),
                force_remove: Vec::new(),
                keep: Vec::new(),
                keep_process_ids: Vec::new(),
            },
        )
        .await
        .unwrap();
        let ended_at = state.session_record("worker").unwrap().ended_at;
        release.send(()).unwrap();
        let err = running
            .await
            .unwrap()
            .expect_err("archive wins over the relaunch");
        if succeeds {
            assert_eq!(code_of(&err), error_code::SESSION_ARCHIVED);
        } else {
            assert!(err.to_string().contains("delayed launch failure"));
        }
        if let Some(live) = &stand_in {
            wait_until(|| live.poll_exit()).await;
            wait_until(|| state.live_session("worker").is_none()).await;
        }
        let after = state.session_record("worker").unwrap();
        assert_eq!(after.status, Archived);
        if !succeeds {
            assert_eq!(
                after.ended_at, ended_at,
                "rollback preserves the newer archive time"
            );
        }
        assert_eq!(
            after.account_id, None,
            "a failed switch restores the original account"
        );
        assert_eq!(after.config_dir, None);
        assert_eq!(status_of(&state, "hub"), Some(Archived));
    }

    /// A Claude Code resume takes the instruction as its launch prompt, shaped as a written message
    /// is, and queues nothing: written into the session it would meet the "Resume this
    /// conversation?" dialog. Another agent has it queued, as before.
    #[tokio::test]
    async fn a_claude_resume_takes_its_instruction_as_the_launch_prompt() {
        use crate::test_support::{stand_in_launch, Launched, StandIn};
        use std::sync::Mutex;
        let (state, _dir) = console_with_project_and_accounts("coordinator-resume-prompt");
        for (id, agent, instruction, prompt, queued) in [
            (
                "claude",
                Agent::Claude,
                "\u{1}/clear it",
                Some(" /clear it"),
                0,
            ),
            ("dashed", Agent::Claude, "- item", Some(" - item"), 0),
            ("blank", Agent::Claude, "\u{1} ", None, 0),
            ("codex", Agent::Codex, "/clear it", None, 1),
        ] {
            state
                .store
                .insert_session(&Session {
                    agent_session_id: Some(format!("conversation-{id}")),
                    has_conversation: true,
                    ..project_session(id, agent, SessionStatus::Interrupted)
                })
                .unwrap();
            let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
            let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));
            let claim = state.begin_launch(id).unwrap();
            relaunch_session_with(
                &state,
                &claim,
                id,
                Some(instruction),
                true,
                None,
                None,
                stand_in_launch(&started, &launches, || {}),
            )
            .await
            .unwrap();
            let launches = launches.lock().unwrap();
            assert_eq!(launches[0].2.as_deref(), prompt, "{id}");
            assert_eq!(state.queued_count(id), queued, "{id}");
        }
    }

    #[tokio::test]
    async fn an_archive_during_resume_ends_a_late_process_and_survives_a_late_failure() {
        archive_during_relaunch(false, true).await;
        archive_during_relaunch(false, false).await;
    }

    #[tokio::test]
    async fn an_archive_during_account_switch_ends_a_late_process_and_survives_a_late_failure() {
        archive_during_relaunch(true, true).await;
        archive_during_relaunch(true, false).await;
    }

    /// A reopen whose launch fails leaves the session exactly as it was archived: the rollback
    /// puts its status back, and the time it was archived at is not re-stamped, so its place in
    /// the archive and the time its row reports both stay where they were.
    #[tokio::test]
    async fn a_reopen_that_fails_to_launch_keeps_the_time_it_was_archived_at() {
        let (state, dir) = console_with_project_and_accounts("coordinator-reopen-launch-refused");
        state
            .store
            .insert_session(&Session {
                ended_at: Some(5),
                ..project_session("s", Agent::Claude, SessionStatus::Archived)
            })
            .unwrap();

        let err = resume_session_with(&state, "s", None, None, stub_shell(&dir))
            .await
            .expect_err("the project's directory does not exist");
        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);

        let after = state.store.get_session("s").unwrap().unwrap();
        assert_eq!(after.status, SessionStatus::Archived);
        assert_eq!(after.ended_at, Some(5));
    }

    /// A reopen that changes the model and effort refuses a value the agent does not offer before
    /// anything is launched or recorded, and puts the session's own back when the launch fails.
    #[tokio::test]
    async fn a_reopen_with_another_model_is_checked_and_put_back_when_it_fails() {
        let (state, dir) = console_with_project_and_accounts("coordinator-reopen-launch-edit");
        state
            .store
            .insert_session(&Session {
                ended_at: Some(5),
                model: Some(Some("sonnet".to_string())),
                ..project_session("s", Agent::Claude, SessionStatus::Archived)
            })
            .unwrap();
        let edit = |model: &str, effort: Option<&str>| LaunchEdit {
            model: Some(Some(model.to_string())),
            effort: effort.map(|effort| Some(effort.to_string())),
        };

        let err = resume_session_with_launch(
            &state,
            "s",
            None,
            None,
            edit("gpt-9", None),
            stub_shell(&dir),
            term::launch,
        )
        .await
        .expect_err("model not offered");
        assert_eq!(code_of(&err), error_code::INVALID_LAUNCH_SETTING);
        let err = resume_session_with_launch(
            &state,
            "s",
            None,
            None,
            edit("opus", Some("extreme")),
            stub_shell(&dir),
            term::launch,
        )
        .await
        .expect_err("effort not offered");
        assert_eq!(code_of(&err), error_code::INVALID_LAUNCH_SETTING);
        let after = state.store.get_session("s").unwrap().unwrap();
        assert_eq!(after.model, Some(Some("sonnet".to_string())));

        // Offered, so recorded for the launch, which fails: the project's directory does not exist.
        let err = resume_session_with_launch(
            &state,
            "s",
            None,
            None,
            edit("opus", Some("high")),
            stub_shell(&dir),
            term::launch,
        )
        .await
        .expect_err("the project's directory does not exist");
        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
        let after = state.store.get_session("s").unwrap().unwrap();
        assert_eq!(after.status, SessionStatus::Archived);
        assert_eq!(
            (after.model, after.effort),
            (Some(Some("sonnet".to_string())), None)
        );
    }

    const AGENT_ID: &str = "11111111-2222-3333-4444-555555555555";

    /// An archived session of `agent` with a conversation, pinned to `account`, archived at 5.
    fn archived_with_conversation(id: &str, agent: Agent, account: &Path) -> Session {
        Session {
            ended_at: Some(5),
            has_conversation: true,
            agent_session_id: Some(AGENT_ID.to_string()),
            config_dir: Some(account.to_string_lossy().into_owned()),
            ..project_session(id, agent, SessionStatus::Archived)
        }
    }

    fn assert_still_archived_at_5(state: &Arc<AppState>, id: &str) {
        let after = state.store.get_session(id).unwrap().unwrap();
        assert_eq!(after.status, SessionStatus::Archived);
        assert_eq!(after.ended_at, Some(5));
    }

    /// A reopen of a session whose conversation the agent no longer has is refused before any
    /// launch, leaving the session archived with its archive time.
    #[tokio::test]
    async fn a_reopen_is_refused_when_the_conversation_record_is_gone() {
        let (state, dir) = console_with_project_and_accounts("coordinator-reopen-unresumable");
        let account = dir.join("claude-a");
        std::fs::create_dir_all(account.join("projects")).unwrap();
        state
            .store
            .insert_session(&archived_with_conversation("s", Agent::Claude, &account))
            .unwrap();

        let err = resume_session_with(&state, "s", None, None, stub_shell(&dir))
            .await
            .expect_err("no transcript");
        assert_eq!(code_of(&err), error_code::CONVERSATION_UNRESUMABLE);
        assert_still_archived_at_5(&state, "s");

        // The record's presence lets the reopen go as far as the launch, which this fixture's
        // missing project directory then refuses.
        let transcript = account.join(format!("projects/work/{AGENT_ID}.jsonl"));
        std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        std::fs::write(&transcript, "{}\n").unwrap();
        let err = resume_session_with(&state, "s", None, None, stub_shell(&dir))
            .await
            .expect_err("no cwd");
        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
    }

    /// The check made before a request to resume is put to the user raises what the reopen would
    /// raise for a conversation the agent no longer has, changes nothing, and otherwise names the
    /// archived owners that would come back with the session.
    #[tokio::test]
    async fn the_check_before_asking_to_resume_refuses_a_conversation_that_is_gone() {
        let (state, dir) = console_with_project_and_accounts("coordinator-check-can-resume");
        let account = dir.join("claude-a");
        std::fs::create_dir_all(account.join("projects")).unwrap();
        let mut owner = archived_with_conversation("owner", Agent::Claude, &account);
        owner.agent_session_id = Some("owner-conversation".to_string());
        state.store.insert_session(&owner).unwrap();
        let mut held = archived_with_conversation("s", Agent::Claude, &account);
        held.bound_to = Some("owner".to_string());
        state.store.insert_session(&held).unwrap();

        let err = check_can_resume_with(&state, "s", stub_shell(&dir))
            .await
            .expect_err("no transcript");
        assert_eq!(code_of(&err), error_code::CONVERSATION_UNRESUMABLE);
        assert_still_archived_at_5(&state, "s");

        let transcript = account.join(format!("projects/work/{AGENT_ID}.jsonl"));
        std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        std::fs::write(&transcript, "{}\n").unwrap();
        let owners = check_can_resume_with(&state, "s", stub_shell(&dir))
            .await
            .unwrap();
        assert_eq!(owners, ["owner"]);
    }

    /// Only proven absence refuses: a Grok session folder missing locally may be restored by Grok.
    #[tokio::test]
    async fn a_reopen_of_a_record_that_cannot_be_proven_gone_still_launches() {
        let (state, dir) = console_with_project_and_accounts("coordinator-reopen-unknown");
        let account = dir.join("grok-a");
        std::fs::create_dir_all(account.join("sessions")).unwrap();
        state
            .store
            .insert_session(&archived_with_conversation("s", Agent::Grok, &account))
            .unwrap();

        let err = resume_session_with(&state, "s", None, None, stub_shell(&dir))
            .await
            .expect_err("no cwd");
        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
    }

    /// An archived console session `hub` with a bound `worker`. `hub`'s workdir is never created
    /// (the fixture's console points at one that does not exist), so a reopen that reaches `hub`'s
    /// launch stops at the missing directory instead of starting the real `claude`.
    fn insert_hub_and_worker(state: &Arc<AppState>, hub_account: &Path, worker_account: &Path) {
        for session in [
            Session {
                ended_at: Some(5),
                has_conversation: true,
                agent_session_id: Some("22222222-2222-3333-4444-555555555555".to_string()),
                config_dir: Some(hub_account.to_string_lossy().into_owned()),
                ..console_session("hub", SessionStatus::Archived)
            },
            Session {
                bound_to: Some("hub".to_string()),
                ..archived_with_conversation("worker", Agent::Claude, worker_account)
            },
        ] {
            state.store.insert_session(&session).unwrap();
        }
    }

    /// The session to reopen is checked before any owner is brought back, so a session whose
    /// record is gone leaves the whole group archived.
    #[tokio::test]
    async fn a_reopen_checks_the_session_before_reopening_any_owner() {
        let (state, dir) =
            console_with_project_and_accounts("coordinator-reopen-target-unresumable");
        let hub_account = dir.join("claude-hub");
        let record = hub_account.join("projects/work/22222222-2222-3333-4444-555555555555.jsonl");
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        std::fs::write(&record, "{}\n").unwrap();
        let worker_account = dir.join("claude-worker");
        std::fs::create_dir_all(worker_account.join("projects")).unwrap();
        insert_hub_and_worker(&state, &hub_account, &worker_account);

        let err = resume_session_with(&state, "worker", None, None, stub_shell(&dir))
            .await
            .expect_err("the session's transcript is gone");
        assert_eq!(code_of(&err), error_code::CONVERSATION_UNRESUMABLE);
        assert_still_archived_at_5(&state, "hub");
        assert_still_archived_at_5(&state, "worker");
    }

    /// An owner whose record is gone is forgotten only when the reopen goes on: with the session
    /// itself refused too, the owner keeps its conversation.
    #[tokio::test]
    async fn a_refused_reopen_leaves_an_unresumable_owner_with_its_conversation() {
        let (state, dir) = console_with_project_and_accounts("coordinator-reopen-both-unresumable");
        let (hub_account, worker_account) = (dir.join("claude-hub"), dir.join("claude-worker"));
        std::fs::create_dir_all(hub_account.join("projects")).unwrap();
        std::fs::create_dir_all(worker_account.join("projects")).unwrap();
        insert_hub_and_worker(&state, &hub_account, &worker_account);

        let err = resume_session_with(&state, "worker", None, None, stub_shell(&dir))
            .await
            .expect_err("neither transcript exists");
        assert_eq!(code_of(&err), error_code::CONVERSATION_UNRESUMABLE);
        let hub = state.store.get_session("hub").unwrap().unwrap();
        assert!(hub.has_conversation);
        assert_eq!(
            hub.agent_session_id.as_deref(),
            Some("22222222-2222-3333-4444-555555555555")
        );
        assert_still_archived_at_5(&state, "hub");
        assert_still_archived_at_5(&state, "worker");
    }

    /// An owner whose record is gone is relaunched into a fresh conversation, and the reopen goes
    /// on instead of being refused. The launch here stops at the owner's missing workdir, after
    /// the conversation was forgotten.
    #[tokio::test]
    async fn a_reopen_relaunches_an_owner_whose_record_is_gone_into_a_fresh_conversation() {
        let (state, dir) =
            console_with_project_and_accounts("coordinator-reopen-owner-unresumable");
        let hub_account = dir.join("claude-hub");
        std::fs::create_dir_all(hub_account.join("projects")).unwrap();
        let worker_account = dir.join("claude-worker");
        let record = worker_account.join(format!("projects/work/{AGENT_ID}.jsonl"));
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        std::fs::write(&record, "{}\n").unwrap();
        insert_hub_and_worker(&state, &hub_account, &worker_account);

        let err = resume_session_with(&state, "worker", None, None, stub_shell(&dir))
            .await
            .expect_err("the owner's workdir does not exist");
        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
        assert_still_archived_at_5(&state, "hub");
        let hub = state.store.get_session("hub").unwrap().unwrap();
        assert!(!hub.has_conversation);
        assert_eq!(hub.agent_session_id, None);
        assert_still_archived_at_5(&state, "worker");
    }

    /// A target whose `codex unarchive` fails is refused before any owner is relaunched, so the
    /// archived owner above it stays archived too.
    #[tokio::test]
    async fn a_reopen_whose_target_cannot_be_unarchived_leaves_the_owner_archived() {
        let (state, dir) = console_with_project_and_accounts("coordinator-reopen-target-unarchive");
        let hub_account = dir.join("claude-hub");
        let hub_record =
            hub_account.join("projects/work/22222222-2222-3333-4444-555555555555.jsonl");
        std::fs::create_dir_all(hub_record.parent().unwrap()).unwrap();
        std::fs::write(&hub_record, "{}\n").unwrap();
        let account = dir.join("codex-a");
        let archived = account.join(format!(
            "archived_sessions/rollout-2026-10-01T00-00-00-{AGENT_ID}.jsonl.zst"
        ));
        std::fs::create_dir_all(archived.parent().unwrap()).unwrap();
        std::fs::write(&archived, "x").unwrap();
        insert_hub_and_worker(&state, &hub_account, &dir.join("claude-worker"));
        state.store.delete_session_if_archived("worker").unwrap();
        state
            .store
            .insert_session(&Session {
                bound_to: Some("hub".to_string()),
                ..archived_with_conversation("worker", Agent::Codex, &account)
            })
            .unwrap();

        let err = resume_session_with(&state, "worker", None, None, fake_codex(&dir, false))
            .await
            .expect_err("unarchive fails");
        assert_eq!(code_of(&err), error_code::CODEX_UNARCHIVE_FAILED);
        assert_still_archived_at_5(&state, "hub");
        assert_still_archived_at_5(&state, "worker");
        assert!(archived.exists());
    }

    /// The leftovers of the conversation an owner gives up go to the Trash, wherever the
    /// accounts hold a copy of it; a copy the owner's own account no longer has does not make the
    /// owner resumable.
    #[tokio::test]
    async fn a_reopen_trashes_the_leftovers_of_the_conversation_an_owner_gives_up() {
        let (state, dir) = console_with_project_and_accounts("coordinator-reopen-owner-leftover");
        let hub_account = dir.join("claude-hub");
        std::fs::create_dir_all(hub_account.join("projects")).unwrap();
        let leftover = dir
            .join("claude-a")
            .join("projects/work/22222222-2222-3333-4444-555555555555.jsonl");
        std::fs::create_dir_all(leftover.parent().unwrap()).unwrap();
        std::fs::write(&leftover, "{}\n").unwrap();
        let worker_account = dir.join("claude-worker");
        let record = worker_account.join(format!("projects/work/{AGENT_ID}.jsonl"));
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        std::fs::write(&record, "{}\n").unwrap();
        insert_hub_and_worker(&state, &hub_account, &worker_account);

        let err = resume_session_with(&state, "worker", None, None, stub_shell(&dir))
            .await
            .expect_err("the owner's workdir does not exist");
        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
        assert!(!leftover.exists(), "the Trash stand-in removed it");
        assert!(record.exists(), "the session's own record is untouched");
    }

    /// A pinned account directory that cannot be read proves nothing, so the reopen goes on to the
    /// launch, which refuses it for its own reason.
    #[tokio::test]
    async fn a_reopen_with_an_unreadable_account_directory_is_left_to_the_launch() {
        let (state, dir) = console_with_project_and_accounts("coordinator-reopen-account-gone");
        state
            .store
            .insert_session(&archived_with_conversation(
                "s",
                Agent::Claude,
                &dir.join("gone"),
            ))
            .unwrap();

        let err = resume_session_with(&state, "s", None, None, stub_shell(&dir))
            .await
            .expect_err("no cwd");
        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
        assert_still_archived_at_5(&state, "s");
    }

    /// A login-shell snapshot that runs no shell: only a `HOME`, below `dir`, for the default
    /// accounts to be found under.
    fn stub_shell(dir: &Path) -> impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static {
        let env = HashMap::from([(
            "HOME".to_string(),
            dir.join("home").to_string_lossy().into_owned(),
        )]);
        move || Ok(env)
    }

    /// A `codex` that reports the home it ran under and moves the archived rollout back, or fails.
    fn fake_codex(dir: &Path, succeeds: bool) -> impl Fn() -> Result<HashMap<String, String>> {
        let body = if succeeds {
            format!(
                "echo \"$CODEX_HOME $*\" > '{log}'\nmkdir -p \"$CODEX_HOME/sessions/2026/10/01\"\nmv \"$CODEX_HOME\"/archived_sessions/* \"$CODEX_HOME/sessions/2026/10/01/\"\n",
                log = dir.join("unarchive.log").display()
            )
        } else {
            "echo 'thread is not archived' >&2\nexit 1\n".to_string()
        };
        let env = crate::test_support::fake_agent(dir, "codex", &body);
        move || Ok(env.clone())
    }

    /// A Codex rollout only in `archived_sessions/` is brought back under the session's own
    /// account before the resume; a failed `codex unarchive` is its own coded error and the
    /// session stays as it was.
    #[tokio::test]
    async fn a_reopen_unarchives_a_codex_rollout_under_the_sessions_own_account() {
        let (state, dir) = console_with_project_and_accounts("coordinator-reopen-unarchive");
        let account = dir.join("codex-a");
        let rollout = format!("rollout-2026-10-01T00-00-00-{AGENT_ID}.jsonl.zst");
        let archived = account.join("archived_sessions").join(&rollout);
        std::fs::create_dir_all(archived.parent().unwrap()).unwrap();
        std::fs::write(&archived, "x").unwrap();
        state
            .store
            .insert_session(&archived_with_conversation("s", Agent::Codex, &account))
            .unwrap();

        let err = resume_session_with(&state, "s", None, None, fake_codex(&dir, false))
            .await
            .expect_err("unarchive fails");
        assert_eq!(code_of(&err), error_code::CODEX_UNARCHIVE_FAILED);
        let coded = err.downcast_ref::<CodedError>().unwrap();
        assert_eq!(
            coded.params.get("detail").map(String::as_str),
            Some("thread is not archived")
        );
        assert_still_archived_at_5(&state, "s");
        assert!(archived.exists());

        // The unarchive done, the reopen goes on to the launch, which this fixture refuses.
        let err = resume_session_with(&state, "s", None, None, fake_codex(&dir, true))
            .await
            .expect_err("no cwd");
        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
        assert!(!archived.exists());
        assert!(account.join("sessions/2026/10/01").join(&rollout).exists());
        let log = std::fs::read_to_string(dir.join("unarchive.log")).unwrap();
        assert_eq!(
            log.trim(),
            format!("{} unarchive {AGENT_ID}", account.display())
        );
        assert_still_archived_at_5(&state, "s");
    }

    /// A project session bound to the console session `owner`, titled by its id so a refusal that
    /// names it can be read back.
    fn bound_session(id: &str, status: SessionStatus, owner: &str) -> Session {
        Session {
            title: id.to_string(),
            bound_to: Some(owner.to_string()),
            ..project_session(id, Agent::Claude, status)
        }
    }

    fn status_of(state: &Arc<AppState>, id: &str) -> Option<SessionStatus> {
        state.store.get_session(id).unwrap().map(|s| s.status)
    }

    /// The line is the process, not the status: any bound session with one refuses the archive,
    /// working, awaiting instructions or waiting for the user alike, and so does one being
    /// launched right now. The refusal counts them and names them, and nothing was archived.
    #[tokio::test]
    async fn archiving_a_console_session_is_refused_while_a_bound_process_runs() {
        use SessionStatus::{Idle, Interrupted, WaitingUser, Working};
        let (state, _dir) = console_with_project_and_accounts("coordinator-archive-refused");
        state
            .store
            .insert_session(&console_session("hub", Idle))
            .unwrap();
        state
            .store
            .insert_session(&bound_session("dormant", Interrupted, "hub"))
            .unwrap();

        // (what runs, the statuses of the bound sessions that have a process)
        let cases: [(&str, &[SessionStatus]); 4] = [
            ("working", &[Working]),
            ("awaiting instructions", &[Idle]),
            ("waiting for the user", &[WaitingUser]),
            ("several", &[Working, Idle]),
        ];
        for (name, statuses) in cases {
            let mut live = Vec::new();
            let ids: Vec<String> = (0..statuses.len()).map(|n| format!("run-{n}")).collect();
            for (id, status) in ids.iter().zip(statuses) {
                state
                    .store
                    .insert_session(&Session {
                        title: format!("Task “{id}”, notes"),
                        ..bound_session(id, *status, "hub")
                    })
                    .unwrap();
                let fake = idle_stand_in(id);
                state.register_live(fake.clone());
                live.push(fake);
            }

            let err = archive_session(&state, "hub").expect_err(name);
            assert_eq!(code_of(&err), error_code::SESSION_HAS_RUNNING_SESSIONS);
            let coded = err.downcast_ref::<CodedError>().unwrap();
            assert_eq!(coded.params["count"], statuses.len().to_string(), "{name}");
            let mut shown_ids: Vec<String> =
                serde_json::from_str(&coded.params["session_ids"]).unwrap();
            shown_ids.sort();
            assert_eq!(shown_ids, ids, "{name}");
            for id in &ids {
                assert!(
                    coded.params["sessions"].contains(&format!("Task “{id}”, notes")),
                    "{name}"
                );
                assert!(coded.message.contains(id.as_str()), "{name}");
            }
            assert!(!coded.params["sessions"].contains("dormant"), "{name}");
            assert_eq!(status_of(&state, "hub"), Some(Idle), "{name}");
            assert_eq!(status_of(&state, "dormant"), Some(Interrupted), "{name}");

            drop(live); // ends the stand-in processes
            for id in &ids {
                state.store.delete_session(id).unwrap();
            }
        }

        // A bound session being launched has no registered process yet, and still counts.
        let claim = state.begin_launch("dormant").unwrap();
        let err = archive_session(&state, "hub").expect_err("launching");
        assert_eq!(code_of(&err), error_code::SESSION_HAS_RUNNING_SESSIONS);
        assert_eq!(status_of(&state, "hub"), Some(Idle));
        drop(claim);
    }

    /// The user's confirmed control request ends both levels, including manually bound
    /// sessions and launches in progress. Agent-driven archives keep their separate safeguard.
    #[tokio::test]
    async fn user_archive_ends_running_descendants_and_preserves_other_sessions() {
        use SessionStatus::{Archived, Idle, Interrupted, WaitingUser, Working};
        let (state, _dir) = console_with_project_and_accounts("coordinator-user-archive-cascade");
        for session in [
            console_session("hub", Idle),
            lead_session("lead", Working, "hub"),
            Session {
                origin: Origin::User,
                ..bound_session("manual", WaitingUser, "hub")
            },
            bound_session("nested", Idle, "lead"),
            bound_session("launching", Interrupted, "lead"),
            Session {
                ended_at: Some(5),
                ..bound_session("old", Archived, "hub")
            },
            console_session("other-hub", Idle),
            bound_session("elsewhere", Working, "other-hub"),
            project_session("unbound", Agent::Claude, Working),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        let processes: Vec<_> = ["hub", "lead", "manual", "nested", "elsewhere", "unbound"]
            .into_iter()
            .map(|id| {
                let live = idle_stand_in(id);
                state.register_live(live.clone());
                live
            })
            .collect();
        let launch = state.begin_launch("launching").unwrap();

        // Preflight includes descendants before ending any process.
        state.store.set_session_locked("nested", true).unwrap();
        let request = || RequestBody::ArchiveSession {
            session: "hub".to_string(),
            force_remove: Vec::new(),
            keep: Vec::new(),
            keep_process_ids: Vec::new(),
        };
        let err = handle(&state, None, request()).await.unwrap_err();
        assert_eq!(code_of(&err), error_code::SESSION_LOCKED);
        for id in ["hub", "lead", "manual", "nested", "launching"] {
            assert_ne!(status_of(&state, id), Some(Archived), "{id}");
        }
        state.store.set_session_locked("nested", false).unwrap();
        let mut events = state.subscribe();
        handle(&state, None, request()).await.unwrap();
        let mut archived = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let Event::SessionUpserted { session } = event {
                archived.push(session.id);
            }
        }
        assert_eq!(archived.last().map(String::as_str), Some("hub"));
        let position = |id: &str| archived.iter().position(|session| session == id).unwrap();
        assert!(position("nested") < position("lead"));
        assert!(position("launching") < position("lead"));
        for process in &processes[..4] {
            wait_until(|| process.poll_exit()).await;
        }
        for process in &processes[4..] {
            assert!(!process.poll_exit(), "unrelated process was ended");
        }

        for id in ["hub", "lead", "manual", "nested", "launching"] {
            assert_eq!(status_of(&state, id), Some(Archived), "{id}");
        }
        for id in ["elsewhere", "unbound"] {
            assert_eq!(status_of(&state, id), Some(Working), "{id}");
        }
        assert_eq!(status_of(&state, "other-hub"), Some(Idle));
        assert_eq!(state.session_record("old").unwrap().ended_at, Some(5));
        drop(launch);
        drop(processes);
    }

    /// With no bound process running, archiving a console session archives every bound session
    /// that is not archived yet, and only those: not an unbound session, not one bound to another
    /// console session, and an archived one keeps its own ended time.
    #[tokio::test]
    async fn archiving_a_console_session_archives_its_interrupted_bound_sessions() {
        use SessionStatus::{Archived, Idle, Interrupted};
        let (state, _dir) = console_with_project_and_accounts("coordinator-archive-cascade");
        for session in [
            console_session("hub", Idle),
            console_session("other-hub", Idle),
            bound_session("a", Interrupted, "hub"),
            bound_session("b", Interrupted, "hub"),
            Session {
                ended_at: Some(5),
                ..bound_session("old", Archived, "hub")
            },
            bound_session("elsewhere", Interrupted, "other-hub"),
            project_session("unbound", Agent::Claude, Interrupted),
        ] {
            state.store.insert_session(&session).unwrap();
        }

        archive_session(&state, "hub").unwrap();

        let statuses: Vec<(String, SessionStatus)> = session_ids(&state)
            .into_iter()
            .map(|id| {
                let status = status_of(&state, &id).unwrap();
                (id, status)
            })
            .collect();
        assert_eq!(
            statuses,
            [
                ("a".to_string(), Archived),
                ("b".to_string(), Archived),
                ("elsewhere".to_string(), Interrupted),
                ("hub".to_string(), Archived),
                ("old".to_string(), Archived),
                ("other-hub".to_string(), Idle),
                ("unbound".to_string(), Interrupted),
            ]
        );
        assert_eq!(
            state.store.get_session("old").unwrap().unwrap().ended_at,
            Some(5)
        );
    }

    /// A project session that started sessions is an owner like a console session: archiving it is
    /// refused while one of them has a process, and otherwise takes the interrupted ones along.
    #[tokio::test]
    async fn archiving_a_project_session_follows_the_rule_for_the_sessions_bound_to_it() {
        use SessionStatus::{Archived, Idle, Interrupted};
        let (state, _dir) = console_with_project_and_accounts("coordinator-archive-project-owner");
        for session in [
            project_session("starter", Agent::Claude, Idle),
            bound_session("running", Idle, "starter"),
            bound_session("dormant", Interrupted, "starter"),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        let running = idle_stand_in("running");
        state.register_live(running.clone());

        let err = archive_session(&state, "starter").expect_err("a bound session is running");
        assert_eq!(code_of(&err), error_code::SESSION_HAS_RUNNING_SESSIONS);
        assert_eq!(status_of(&state, "starter"), Some(Idle));
        assert_eq!(status_of(&state, "dormant"), Some(Interrupted));

        drop(running);
        state.store.delete_session("running").unwrap();
        archive_session(&state, "starter").unwrap();
        assert_eq!(status_of(&state, "starter"), Some(Archived));
        assert_eq!(status_of(&state, "dormant"), Some(Archived));
    }

    /// Archiving a project session reaches no other session: the automatic archive of a `done`
    /// report (see `reporting.rs`) and a `delete_project` that stops sessions both end here, and
    /// neither takes the console session the session is bound to.
    #[tokio::test]
    async fn archiving_a_bound_project_session_leaves_its_console_session_alone() {
        let (state, _dir) = console_with_project_and_accounts("coordinator-archive-bound-alone");
        state
            .store
            .insert_session(&console_session("hub", SessionStatus::Idle))
            .unwrap();
        state
            .store
            .insert_session(&bound_session("worker", SessionStatus::Idle, "hub"))
            .unwrap();

        archive_session(&state, "worker").unwrap();

        assert_eq!(status_of(&state, "worker"), Some(SessionStatus::Archived));
        assert_eq!(status_of(&state, "hub"), Some(SessionStatus::Idle));
    }

    /// A lead session in `status`: bound to the console session `owner` after it was opened.
    fn lead_session(id: &str, status: SessionStatus, owner: &str) -> Session {
        Session {
            lead: true,
            ..bound_session(id, status, owner)
        }
    }

    /// Which owners a relaunch brings back first, topmost first: the archived owner a dormant
    /// session is bound to and, when that is an archived lead session, its archived console
    /// session above it. The walk stops at the first owner that is not archived — only
    /// interrupted, running or gone — and nothing is brought back for an unbound session, for one
    /// that is not dormant, or for a console session, so reopening an owner reopens nothing bound
    /// to it.
    #[test]
    fn a_reopen_brings_back_the_archived_owners_above_it_topmost_first() {
        use SessionStatus::{Archived, Idle, Interrupted};
        let (state, _dir) = console_with_project_and_accounts("coordinator-archived-owners");
        for session in [
            console_session("archived-hub", Archived),
            console_session("interrupted-hub", Interrupted),
            console_session("running-hub", Idle),
            lead_session("archived-lead", Archived, "archived-hub"),
            lead_session("interrupted-lead", Interrupted, "archived-hub"),
            lead_session("lead-under-running-hub", Archived, "running-hub"),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        // (the session's status, what it is bound to, the owners it brings back)
        let cases: [(&str, SessionStatus, Option<&str>, &[&str]); 10] = [
            (
                "archived",
                Archived,
                Some("archived-hub"),
                &["archived-hub"],
            ),
            (
                "interrupted",
                Interrupted,
                Some("archived-hub"),
                &["archived-hub"],
            ),
            ("owner interrupted", Archived, Some("interrupted-hub"), &[]),
            ("owner running", Archived, Some("running-hub"), &[]),
            ("owner gone", Archived, Some("no-such-hub"), &[]),
            ("unbound", Archived, None, &[]),
            ("not dormant", Idle, Some("archived-hub"), &[]),
            (
                "both levels archived",
                Archived,
                Some("archived-lead"),
                &["archived-hub", "archived-lead"],
            ),
            ("lead interrupted", Archived, Some("interrupted-lead"), &[]),
            (
                "lead's owner running",
                Archived,
                Some("lead-under-running-hub"),
                &["lead-under-running-hub"],
            ),
        ];
        for (name, status, owner, expected) in cases {
            let session = Session {
                bound_to: owner.map(str::to_string),
                ..project_session("s", Agent::Claude, status)
            };
            state.store.insert_session(&session).unwrap();
            assert_eq!(archived_owners(&state, "s").unwrap(), expected, "{name}");
            state.store.delete_session("s").unwrap();
        }
        assert!(archived_owners(&state, "archived-hub").unwrap().is_empty());
    }

    /// An owner is left to the reopen while it is archived or a launch of it is in flight (the
    /// claim then refuses the reopen); one that is idle, running or interrupted and not in flight
    /// is skipped.
    #[test]
    fn a_reopen_skips_an_owner_that_is_neither_archived_nor_being_launched() {
        use SessionStatus::{Archived, Idle, Interrupted};
        let (state, _dir) = console_with_project_and_accounts("coordinator-needs-reopening");
        for session in [
            console_session("archived", Archived),
            console_session("idle", Idle),
            console_session("interrupted", Interrupted),
            console_session("launching", Idle),
            console_session("running", Idle),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        let _claim = state.begin_launch("launching").unwrap();
        // Brought back by a sibling reopen, whose claim is already released.
        let running = idle_stand_in("running");
        state.register_live(running.clone());

        let needs = |id: &str| needs_reopening(&state, id).unwrap();
        assert!(needs("archived"));
        assert!(needs("launching"));
        assert!(!needs("idle"));
        assert!(!needs("running"));
        assert!(!needs("interrupted"));
    }

    /// The topmost owner is relaunched first, and the reopen fails as a whole when it cannot be:
    /// nothing below it is attempted (the ended times are untouched) and all of them stay
    /// archived. The console's working directory is missing here and the project's is not, so the
    /// failure can only be the console session's.
    #[tokio::test]
    async fn a_reopen_fails_as_a_whole_when_its_topmost_owner_cannot_be_reopened() {
        let (state, dir) = console_with_project_and_accounts("coordinator-reopen-owner-fails");
        std::fs::create_dir_all(dir.join("no-such-project")).unwrap();
        for session in [
            console_session("hub", SessionStatus::Archived),
            Session {
                ended_at: Some(5),
                ..lead_session("lead", SessionStatus::Archived, "hub")
            },
            Session {
                ended_at: Some(5),
                ..bound_session("worker", SessionStatus::Archived, "lead")
            },
        ] {
            state.store.insert_session(&session).unwrap();
        }

        let err = resume_session_with(&state, "worker", None, None, stub_shell(&dir))
            .await
            .expect_err("the console session cannot launch");

        assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
        assert_eq!(status_of(&state, "hub"), Some(SessionStatus::Archived));
        for id in ["lead", "worker"] {
            let session = state.store.get_session(id).unwrap().unwrap();
            assert_eq!(
                (session.status, session.ended_at),
                (SessionStatus::Archived, Some(5)),
                "{id}"
            );
        }
        assert!(["hub", "lead", "worker"]
            .iter()
            .all(|id| !state.has_process(id)));
    }

    #[tokio::test]
    async fn session_lock_changes_cannot_cross_a_removal_claim() {
        let (state, _dir) = console_with_project_and_accounts("coordinator-lock-removal-race");
        state
            .store
            .insert_session(&console_session("session", SessionStatus::Interrupted))
            .unwrap();
        set_session_locked(&state, "session", true).unwrap();
        let removal = state.claim_session_removal();
        let nested = state.claim_session_removal();
        let task_state = state.clone();
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            for locked in [true, false] {
                assert_eq!(
                    code_of(&set_session_locked(&task_state, "session", locked).unwrap_err()),
                    error_code::SESSION_LOCK_BUSY
                );
            }
        })
        .await
        .unwrap();
        drop(nested);
        assert_eq!(
            code_of(&set_session_locked(&state, "session", false).unwrap_err()),
            error_code::SESSION_LOCK_BUSY
        );
        assert_eq!(
            code_of(&archive_session(&state, "session").unwrap_err()),
            error_code::SESSION_LOCKED
        );
        drop(removal);
        assert!(state.session_record("session").unwrap().locked);
        set_session_locked(&state, "session", false).unwrap();
        archive_session(&state, "session").unwrap();
        assert_eq!(status_of(&state, "session"), Some(SessionStatus::Archived));
    }

    #[test]
    fn locking_an_archived_session_is_refused_but_unlocking_one_is_not() {
        use SessionStatus::{Archived, Interrupted};
        let (state, _dir) = console_with_project_and_accounts("coordinator-lock-archived");
        for (id, status) in [("live", Interrupted), ("old", Archived)] {
            state
                .store
                .insert_session(&console_session(id, status))
                .unwrap();
        }

        assert_eq!(
            code_of(&set_session_locked(&state, "old", true).unwrap_err()),
            error_code::SESSION_ARCHIVED
        );
        assert!(!state.session_record("old").unwrap().locked);

        state.store.set_session_locked("old", true).unwrap();
        assert!(!set_session_locked(&state, "old", false).unwrap().locked);

        assert!(set_session_locked(&state, "live", true).unwrap().locked);
    }

    #[test]
    fn locked_sessions_refuse_archive_and_delete_cascades_without_partial_changes() {
        use SessionStatus::{Archived, Interrupted};
        let (state, _dir) = console_with_project_and_accounts("coordinator-locked-cascades");
        for session in [
            console_session("hub", Interrupted),
            lead_session("lead", Interrupted, "hub"),
            bound_session("worker", Interrupted, "lead"),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        set_session_locked(&state, "worker", true).unwrap();
        for id in ["worker", "lead", "hub"] {
            assert_eq!(
                code_of(&archive_session(&state, id).unwrap_err()),
                error_code::SESSION_LOCKED
            );
            assert_eq!(status_of(&state, id), Some(Interrupted));
        }
        set_session_locked(&state, "worker", false).unwrap();
        archive_session(&state, "hub").unwrap();
        // Straight to the store: the lock a session archived before locking was refused still carries.
        state.store.set_session_locked("worker", true).unwrap();
        assert_eq!(
            code_of(&delete_session(&state, "hub").unwrap_err()),
            error_code::SESSION_LOCKED
        );
        assert_eq!(
            code_of(&delete_archived_sessions(&state, "console-1", None, None).unwrap_err()),
            error_code::SESSION_LOCKED
        );
        for id in ["hub", "lead", "worker"] {
            assert_eq!(status_of(&state, id), Some(Archived));
        }
        set_session_locked(&state, "worker", false).unwrap();
        assert_eq!(delete_session(&state, "hub").unwrap().len(), 3);
    }

    #[test]
    fn a_locked_unused_session_cannot_be_deleted() {
        let (state, _dir) = console_with_project_and_accounts("coordinator-locked-unused");
        state
            .store
            .insert_session(&console_session("unused", SessionStatus::Idle))
            .unwrap();
        set_session_locked(&state, "unused", true).unwrap();
        assert_eq!(
            code_of(&delete_session(&state, "unused").unwrap_err()),
            error_code::SESSION_LOCKED
        );
        assert_eq!(
            code_of(&delete_unused_session(&state, "unused").unwrap_err()),
            error_code::SESSION_LOCKED
        );
    }

    /// Deleting an archived console session takes the archived sessions bound to it, at both
    /// levels, and no others; an archived lead session takes its own archived sessions and leaves
    /// its console session; one bound session on its own leaves the console session; a console
    /// session that is not archived is refused and takes nothing with it.
    #[tokio::test]
    async fn deleting_an_archived_console_session_deletes_its_archived_bound_sessions() {
        use SessionStatus::{Archived, Idle, Interrupted};
        let (state, _dir) = console_with_project_and_accounts("coordinator-delete-cascade");
        for session in [
            console_session("hub", Archived),
            bound_session("a", Archived, "hub"),
            lead_session("b", Archived, "hub"),
            bound_session("of-b", Archived, "b"),
            bound_session("still-here", Interrupted, "hub"),
            console_session("other-hub", Archived),
            bound_session("c", Archived, "other-hub"),
            project_session("unbound", Agent::Claude, Archived),
            console_session("live-hub", Idle),
            bound_session("d", Archived, "live-hub"),
            lead_session("lead", Archived, "live-hub"),
            bound_session("of-lead", Archived, "lead"),
        ] {
            state.store.insert_session(&session).unwrap();
        }

        delete_session(&state, "hub").unwrap();
        assert_eq!(
            session_ids(&state),
            [
                "c",
                "d",
                "lead",
                "live-hub",
                "of-lead",
                "other-hub",
                "still-here",
                "unbound"
            ]
        );

        delete_session(&state, "lead").unwrap();
        assert_eq!(
            session_ids(&state),
            ["c", "d", "live-hub", "other-hub", "still-here", "unbound"],
            "the lead session takes its own and leaves its console session"
        );

        delete_session(&state, "c").unwrap();
        assert_eq!(
            session_ids(&state),
            ["d", "live-hub", "other-hub", "still-here", "unbound"],
            "the bound session alone leaves its console session"
        );

        let err = delete_session(&state, "live-hub").expect_err("sessions bound to it");
        assert_eq!(code_of(&err), error_code::SESSION_NOT_DELETABLE);
        assert_eq!(
            session_ids(&state),
            ["d", "live-hub", "other-hub", "still-here", "unbound"]
        );
    }

    /// The agent's records of the sessions a delete removed go through the trash function, Claude's
    /// subagent directory with the transcript when there is one, unless a remaining session refers
    /// to the same agent session; a missing record never reaches it and a failing move is not an
    /// error.
    #[test]
    fn the_agent_records_of_deleted_sessions_are_trashed_unless_shared() {
        use SessionStatus::{Archived, Interrupted};
        let (state, dir) = console_with_project_and_accounts("coordinator-delete-trash-records");
        let account = dir.join("claude-account");
        let uuid = |name: &str| {
            let n = ["solo", "tail", "shared", "never-written"]
                .iter()
                .position(|each| *each == name)
                .unwrap();
            format!("00000000-0000-0000-0000-00000000000{n}")
        };
        let project_dir = account.join("projects").join("-work");
        std::fs::create_dir_all(project_dir.join(uuid("solo")).join("subagents")).unwrap();
        for name in ["solo", "tail", "shared"] {
            std::fs::write(project_dir.join(format!("{}.jsonl", uuid(name))), "{}\n").unwrap();
        }
        let native = |id: &str, native: &str, status| Session {
            agent_session_id: Some(uuid(native)),
            config_dir: Some(account.to_string_lossy().into_owned()),
            ..project_session(id, Agent::Claude, status)
        };
        for session in [
            native("solo", "solo", Archived),
            native("tail", "tail", Archived),
            native("copy-a", "shared", Archived),
            native("copy-b", "shared", Interrupted),
            native("unrecorded", "never-written", Archived),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        let trashed = std::sync::Mutex::new(Vec::new());
        let trash = |path: &Path| {
            trashed.lock().unwrap().push(path.to_path_buf());
            anyhow::bail!("the Trash refused it")
        };
        let locator = || {
            agent_records::Locator::with_shell(HashMap::from([(
                "HOME".to_string(),
                dir.join("home").to_string_lossy().into_owned(),
            )]))
        };
        let deleted = delete_session(&state, "solo").unwrap();
        trash_agent_records_of(&state, &deleted, locator(), trash);
        assert_eq!(
            *trashed.lock().unwrap(),
            [
                project_dir.join(format!("{}.jsonl", uuid("solo"))),
                project_dir.join(uuid("solo"))
            ]
        );
        trashed.lock().unwrap().clear();
        let deleted =
            delete_archived_sessions(&state, "console-1", Some("project-1"), None).unwrap();
        assert_eq!(deleted.len(), 3);
        trash_agent_records_of(&state, &deleted, locator(), trash);
        assert_eq!(
            *trashed.lock().unwrap(),
            [project_dir.join(format!("{}.jsonl", uuid("tail")))],
            "the shared copy, the unwritten record and the missing subagent directory are left"
        );
    }

    /// A Codex thread that had a turn reverted goes to the Trash whole: its first rollout and every
    /// later segment, plain or compressed.
    #[test]
    fn a_reverted_codex_threads_whole_chain_is_trashed() {
        let (state, dir) = console_with_project_and_accounts("coordinator-delete-trash-chain");
        let account = dir.join("codex-account");
        let thread = "019a0000-0000-7000-8000-000000000001";
        let mut expected = Vec::new();
        for compressed in [false, true] {
            expected.extend(crate::test_support::codex_chain(&account, thread, compressed).all());
        }
        state
            .store
            .insert_session(&Session {
                agent_session_id: Some(thread.to_owned()),
                config_dir: Some(account.to_string_lossy().into_owned()),
                ..project_session("reverted", Agent::Codex, SessionStatus::Archived)
            })
            .unwrap();
        let deleted = delete_session(&state, "reverted").unwrap();
        let trashed = std::sync::Mutex::new(Vec::new());
        trash_agent_records_of(
            &state,
            &deleted,
            agent_records::Locator::with_shell(HashMap::from([(
                "HOME".to_owned(),
                dir.join("home").to_string_lossy().into_owned(),
            )])),
            |path| {
                trashed.lock().unwrap().push(path.to_path_buf());
                Ok(())
            },
        );
        let mut trashed = trashed.into_inner().unwrap();
        trashed.sort();
        expected.sort();
        assert_eq!(trashed, expected);
    }

    #[tokio::test]
    async fn guarded_deletion_accepts_an_unused_codex_session_without_a_native_id() {
        let (state, _dir) = console_with_project_and_accounts("coordinator-delete-unused-codex");
        state
            .store
            .insert_session(&project_session(
                "unused",
                Agent::Codex,
                SessionStatus::Idle,
            ))
            .unwrap();
        let mut events = state.subscribe();
        handle(
            &state,
            None,
            RequestBody::DeleteSession {
                session: "unused".to_owned(),
                only_if_unused: true,
                trash_agent_records: false,
                force_remove: Vec::new(),
                keep: Vec::new(),
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            events.try_recv().unwrap(),
            Event::SessionDeleted { session } if session == "unused"
        ));
        assert!(state.store.get_session("unused").unwrap().is_none());
    }

    #[tokio::test]
    async fn guarded_deletion_accepts_and_trashes_unused_grok_native_records() {
        let (state, dir) = console_with_project_and_accounts("coordinator-delete-unused-grok");
        let account = dir.join("account");
        let native_id = "11111111-2222-3333-4444-555555555555";
        let records = account.join("sessions/work").join(native_id);
        std::fs::create_dir_all(&records).unwrap();
        std::fs::write(records.join("usage.json"), "{}").unwrap();
        std::fs::write(records.join("messages.json"), "[]").unwrap();
        state
            .store
            .insert_session(&Session {
                agent_session_id: Some(native_id.to_owned()),
                config_dir: Some(account.to_string_lossy().into_owned()),
                ..project_session("unused", Agent::Grok, SessionStatus::Idle)
            })
            .unwrap();
        let deleted = delete_unused_session(&state, "unused").unwrap();
        assert!(state.store.get_session("unused").unwrap().is_none());
        let trashed = dir.join("trash");
        trash_agent_records_of(
            &state,
            &deleted,
            agent_records::Locator::with_shell(HashMap::from([(
                "HOME".to_owned(),
                dir.join("home").to_string_lossy().into_owned(),
            )])),
            |path| {
                assert_eq!(path, records);
                std::fs::rename(path, &trashed)?;
                Ok(())
            },
        );
        assert!(!records.exists());
        assert!(trashed.join("messages.json").exists());
    }

    #[tokio::test]
    async fn unused_deletion_returns_the_native_identity_at_the_atomic_delete() {
        let (state, dir) = console_with_project_and_accounts("coordinator-delete-unused-identity");
        let initial = project_session("unused", Agent::Codex, SessionStatus::Idle);
        state.store.insert_session(&initial).unwrap();
        let stale = state.session_record("unused").unwrap();
        assert!(stale.agent_session_id.is_none());
        let native_id = "11111111-2222-3333-4444-555555555555";
        let account = dir.join("codex-account");
        let rollout = account
            .join("sessions/2026/10/17")
            .join(format!("rollout-2026-10-17T01-02-03-{native_id}.jsonl"));
        std::fs::create_dir_all(rollout.parent().unwrap()).unwrap();
        std::fs::write(&rollout, "{}\n").unwrap();
        let discovered = Session {
            agent_session_id: Some(native_id.to_owned()),
            config_dir: Some(account.to_string_lossy().into_owned()),
            ..initial
        };
        state.save_session(&discovered).unwrap();
        state
            .set_session_account("unused", None, discovered.config_dir.as_deref())
            .unwrap();
        let Deletion::Deleted { session, .. } = state.delete_if_unused("unused").unwrap() else {
            panic!("native-id discovery must leave an unused session deletable");
        };
        assert_eq!(session.agent_session_id, discovered.agent_session_id);
        assert_eq!(session.config_dir, discovered.config_dir);
        let trashed = dir.join("trashed-rollout.jsonl");
        trash_agent_records_of(
            &state,
            &[*session],
            agent_records::Locator::with_shell(HashMap::from([(
                "HOME".to_owned(),
                dir.join("home").to_string_lossy().into_owned(),
            )])),
            |path| {
                assert_eq!(path, rollout);
                std::fs::rename(path, &trashed)?;
                Ok(())
            },
        );
        assert!(!rollout.exists());
        assert!(trashed.exists());
    }

    #[tokio::test]
    async fn guarded_deletion_refuses_a_first_turn_or_archived_owner_instead_of_cascading() {
        let (state, _dir) = console_with_project_and_accounts("coordinator-delete-unused-race");
        let session = project_session("first-turn", Agent::Codex, SessionStatus::Idle);
        state.store.insert_session(&session).unwrap();
        let mut started = session.clone();
        started.has_conversation = true;
        started.status = SessionStatus::Archived;
        state.save_session(&started).unwrap();
        state
            .store
            .insert_session(&console_session("owner", SessionStatus::Archived))
            .unwrap();
        state
            .store
            .insert_session(&Session {
                origin: Origin::Console,
                ..bound_session("child", SessionStatus::Archived, "owner")
            })
            .unwrap();
        for id in ["first-turn", "owner", "child"] {
            let err = handle(
                &state,
                None,
                RequestBody::DeleteSession {
                    session: id.to_owned(),
                    only_if_unused: true,
                    trash_agent_records: false,
                    force_remove: Vec::new(),
                    keep: Vec::new(),
                },
            )
            .await
            .unwrap_err();
            assert_eq!(code_of(&err), error_code::SESSION_NOT_DELETABLE, "{id}");
        }
        assert_eq!(session_ids(&state), ["child", "first-turn", "owner"]);
    }

    /// A session that is not archived is deleted only while it is unused: no turn yet, with none
    /// bound to it, and bound to no owner or opened by the user. Deleting one stops its process;
    /// any other is refused and keeps both its record and its process.
    #[tokio::test]
    async fn deleting_a_session_that_is_not_archived_needs_it_to_be_unused() {
        use SessionStatus::{Idle, Interrupted};
        let (state, _dir) = console_with_project_and_accounts("coordinator-delete-unused");
        for session in [
            project_session("fresh", Agent::Claude, Idle),
            project_session("never-used", Agent::Claude, Interrupted),
            Session {
                has_conversation: true,
                ..project_session("used", Agent::Claude, Idle)
            },
            console_session("hub", Idle),
            bound_session("opened-by-user", Idle, "hub"),
            Session {
                origin: Origin::Console,
                ..bound_session("started-by-agent", Idle, "hub")
            },
        ] {
            state.store.insert_session(&session).unwrap();
        }
        let fresh = idle_stand_in("fresh");
        state.register_live(fresh.clone());
        let used = idle_stand_in("used");
        state.register_live(used.clone());
        let mut events = state.subscribe();

        delete_session(&state, "fresh").unwrap();
        assert!(matches!(
            events.try_recv().unwrap(),
            Event::SessionDeleted { session } if session == "fresh"
        ));
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while !fresh.poll_exit() {
            assert!(Instant::now() < deadline, "the process was never stopped");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let claim = state.begin_launch("never-used").unwrap();
        let err = delete_session(&state, "never-used").expect_err("starting");
        assert_eq!(code_of(&err), error_code::SESSION_NOT_DELETABLE);
        drop(claim);
        delete_session(&state, "never-used").unwrap();
        delete_unused_session(&state, "opened-by-user").unwrap();

        for refused in ["used", "hub", "started-by-agent"] {
            let err = delete_session(&state, refused).expect_err(refused);
            assert_eq!(
                code_of(&err),
                error_code::SESSION_NOT_DELETABLE,
                "{refused}"
            );
        }
        assert_eq!(session_ids(&state), ["hub", "started-by-agent", "used"]);
        assert!(!used.poll_exit());
    }

    /// Deleting every archived console session of a console takes their archived bound sessions
    /// too, at both levels; deleting a project's archived sessions takes the bound ones among them
    /// and leaves the console sessions; deleting a console session's archive takes its archived
    /// bound sessions, a lead session's own included, and nothing else, and a project together
    /// with a console session is refused.
    #[tokio::test]
    async fn deleting_archived_sessions_in_bulk_follows_the_binding_for_console_sessions_only() {
        use SessionStatus::Archived;
        let seed = |name: &str| {
            let (state, dir) = console_with_project_and_accounts(&format!("coordinator-{name}"));
            for session in [
                console_session("hub", Archived),
                bound_session("a", Archived, "hub"),
                lead_session("lead", Archived, "hub"),
                bound_session("of-lead", Archived, "lead"),
                project_session("unbound", Agent::Claude, Archived),
            ] {
                state.store.insert_session(&session).unwrap();
            }
            (state, dir)
        };

        let (state, _dir) = seed("bulk-console-scope");
        delete_archived_sessions(&state, "console-1", None, None).unwrap();
        assert_eq!(session_ids(&state), ["unbound"]);

        let (state, _dir) = seed("bulk-project-scope");
        delete_archived_sessions(&state, "console-1", Some("project-1"), None).unwrap();
        assert_eq!(session_ids(&state), ["hub"]);

        let (state, _dir) = seed("bulk-bound-scope");
        delete_archived_sessions(&state, "console-1", None, Some("hub")).unwrap();
        assert_eq!(session_ids(&state), ["hub", "unbound"]);
        for (project, owner, code) in [
            (
                Some("project-1"),
                Some("hub"),
                error_code::CONFLICTING_FIELDS,
            ),
            (None, Some("unbound"), error_code::UNKNOWN_SESSION),
            (None, Some("nobody"), error_code::UNKNOWN_SESSION),
        ] {
            let refused = delete_archived_sessions(&state, "console-1", project, owner)
                .expect_err("not a scope");
            assert_eq!(code_of(&refused), code);
        }
        assert_eq!(session_ids(&state), ["hub", "unbound"]);
    }

    /// `console_with_project_and_accounts` plus a second console and a second project of the
    /// first, with `sessions` recorded and the ones in `live` given a stand-in process.
    fn binding_fixture(
        name: &str,
        sessions: Vec<Session>,
        live: &[&str],
    ) -> (Arc<AppState>, ScratchDir, Vec<crate::test_support::StandIn>) {
        let (state, dir) = console_with_project_and_accounts(&format!("coordinator-{name}"));
        state
            .store
            .insert_console(&Console {
                id: "console-2".to_string(),
                ..console(Agent::Claude, &dir.join("no-such-workdir-2"))
            })
            .unwrap();
        let project_1 = state.store.get_project("project-1").unwrap().unwrap();
        state
            .store
            .insert_project(&Project {
                id: "project-2".to_string(),
                ..project_1
            })
            .unwrap();
        for session in &sessions {
            state.store.insert_session(session).unwrap();
        }
        let stand_ins = live
            .iter()
            .map(|id| {
                let stand_in = idle_stand_in(id);
                state.register_live(stand_in.clone());
                stand_in
            })
            .collect();
        (state, dir, stand_ins)
    }

    /// An unbound project session binds itself to a running console session of its console and
    /// becomes a lead session keeping its own sessions, or, with none of its own, to a running
    /// project session of its project that can own sessions and becomes an ordinary bound one.
    /// Each refusal names its reason and changes nothing.
    #[tokio::test]
    async fn a_session_binds_itself_only_where_orchestration_stays_two_levels_deep() {
        use SessionStatus::{Idle, Interrupted, Starting};
        let (state, _dir, _live) = binding_fixture(
            "bind",
            vec![
                console_session("hub", Idle),
                console_session("sleeping-hub", Interrupted),
                console_session("starting-hub", Starting),
                // Reads running, but has no process behind it.
                console_session("ghost-hub", Idle),
                Session {
                    console_id: "console-2".to_string(),
                    ..console_session("foreign-hub", Idle)
                },
                project_session("starter", Agent::Claude, Idle),
                project_session("other-starter", Agent::Claude, Idle),
                project_session("loner", Agent::Claude, Idle),
                Session {
                    project_id: Some("project-2".to_string()),
                    ..project_session("elsewhere", Agent::Claude, Idle)
                },
                bound_session("mine", Idle, "starter"),
                bound_session("bound", Idle, "hub"),
            ],
            &[
                "hub",
                "starting-hub",
                "foreign-hub",
                "other-starter",
                "elsewhere",
                "mine",
            ],
        );

        for (session, owner, reason) in [
            ("bound", "hub", "already bound"),
            ("hub", "starter", "console session cannot be bound"),
            (
                "starter",
                "other-starter",
                "has sessions of its own (`mine`)",
            ),
            ("loner", "mine", "cannot own sessions"),
            ("loner", "elsewhere", "another project"),
            ("loner", "foreign-hub", "another console"),
            ("loner", "sleeping-hub", "not running"),
            ("loner", "starting-hub", "not running"),
            ("loner", "ghost-hub", "not running"),
            ("loner", "loner", "its own owner"),
        ] {
            let err = bind_session(&state, session, owner).expect_err(reason);
            assert!(err.to_string().contains(reason), "{session} {owner}: {err}");
        }
        let loner = state.store.get_session("loner").unwrap().unwrap();
        assert_eq!((loner.bound_to, loner.lead), (None, false));

        let (lead, _) = bind_session(&state, "starter", "hub").unwrap();
        assert_eq!((lead.bound_to.as_deref(), lead.lead), (Some("hub"), true));
        assert!(!lead.is_worker());
        let mine = state.store.get_session("mine").unwrap().unwrap();
        assert_eq!(mine.bound_to.as_deref(), Some("starter"));

        let (joined, _) = bind_session(&state, "loner", "other-starter").unwrap();
        assert_eq!(
            (joined.bound_to.as_deref(), joined.lead),
            (Some("other-starter"), false)
        );
        assert!(joined.is_worker());
        let again = bind_session(&state, "loner", "hub").expect_err("bound already");
        assert!(again.to_string().contains("already bound"), "{again}");
    }

    /// An owner hands a running session of its own to another running owner under the same rules:
    /// a lead session keeps being one under another console session, goes to a project session only
    /// once it has no sessions of its own, and then can own none. Each refusal names its reason
    /// and changes nothing.
    #[tokio::test]
    async fn an_owner_hands_a_session_over_only_where_orchestration_stays_two_levels_deep() {
        use SessionStatus::{Idle, Interrupted, Starting};
        let (state, _dir, _live) = binding_fixture(
            "transfer",
            vec![
                console_session("hub", Idle),
                console_session("hub-2", Idle),
                console_session("sleeping-hub", Interrupted),
                console_session("starting-hub", Starting),
                Session {
                    console_id: "console-2".to_string(),
                    ..console_session("foreign-hub", Idle)
                },
                lead_session("lead", Idle, "hub"),
                bound_session("child", Idle, "lead"),
                lead_session("spare-lead", Idle, "hub"),
                bound_session("plain", Idle, "hub"),
                bound_session("dormant", Interrupted, "hub"),
                bound_session("starting", Starting, "hub"),
                project_session("peer", Agent::Claude, Idle),
            ],
            &[
                "hub",
                "hub-2",
                "foreign-hub",
                "lead",
                "child",
                "spare-lead",
                "plain",
                "peer",
            ],
        );

        // A session being launched holds the launch claim, which refuses a binding change.
        let _claim = state.begin_launch("starting").unwrap();
        for (session, from, to, reason) in [
            ("lead", "hub", "peer", "has sessions of its own (`child`)"),
            ("plain", "hub", "child", "cannot own sessions"),
            ("plain", "hub", "foreign-hub", "another console"),
            ("plain", "hub", "sleeping-hub", "not running"),
            ("plain", "hub", "starting-hub", "not running"),
            ("plain", "hub", "hub", "already bound"),
            ("plain", "hub", "plain", "its own owner"),
            ("starting", "hub", "hub-2", "being started"),
            ("child", "hub", "hub-2", "not bound to `hub`"),
        ] {
            let before = state.store.get_session(session).unwrap().unwrap();
            let err = transfer_session(&state, session, from, to).expect_err(reason);
            assert!(err.to_string().contains(reason), "{session} to {to}: {err}");
            let after = state.store.get_session(session).unwrap().unwrap();
            assert_eq!((before.bound_to, before.lead), (after.bound_to, after.lead));
        }

        // A session that becomes a bound one loses the report panel's tools, so its pages are
        // withdrawn; a lead session keeps them.
        for (page, session) in [("l-1", "lead"), ("sl-1", "spare-lead")] {
            state
                .store
                .insert_page(&crate::test_support::test_page(page, session, 1))
                .unwrap();
        }
        let (lead, ..) = transfer_session(&state, "lead", "hub", "hub-2").unwrap();
        assert_eq!((lead.bound_to.as_deref(), lead.lead), (Some("hub-2"), true));
        assert_eq!(state.store.list_pages("lead").unwrap().len(), 1);
        let child = state.store.get_session("child").unwrap().unwrap();
        assert_eq!(child.bound_to.as_deref(), Some("lead"));

        let (plain, ..) = transfer_session(&state, "plain", "hub", "peer").unwrap();
        assert_eq!(
            (plain.bound_to.as_deref(), plain.lead),
            (Some("peer"), false)
        );
        let (spare, owner, _) = transfer_session(&state, "spare-lead", "hub", "peer").unwrap();
        assert_eq!(
            (spare.bound_to.as_deref(), spare.lead),
            (Some("peer"), false)
        );
        assert!(spare.is_worker());
        assert_eq!(owner.id, "peer");
        assert!(state.store.list_pages("spare-lead").unwrap().is_empty());
    }

    /// Asserts `err` is the coded refusal `code`, naming `session` (and `owner`, when given).
    fn assert_busy_refusal(err: &anyhow::Error, code: &str, session: &str, owner: Option<&str>) {
        assert_eq!(code_of(err), code);
        let coded = err.downcast_ref::<CodedError>().expect("a coded refusal");
        assert_eq!(
            coded.params.get("session").map(String::as_str),
            Some(session)
        );
        assert_eq!(coded.params.get("owner").map(String::as_str), owner);
    }

    /// A session, or an owner, under a handoff or switch claim is refused a binding change that
    /// involves it, and nothing changes; once the claim is released the same change goes through.
    #[tokio::test]
    async fn a_claimed_session_or_owner_is_not_bound_to_or_from() {
        use SessionStatus::Idle;
        let (state, _dir, _live) = binding_fixture(
            "binding-claimed",
            vec![
                console_session("hub", Idle),
                console_session("hub-2", Idle),
                project_session("loner", Agent::Claude, Idle),
                bound_session("plain", Idle, "hub"),
            ],
            &["hub", "hub-2", "loner", "plain"],
        );

        let claim = state.begin_switch("loner").unwrap();
        let err = bind_session(&state, "loner", "hub").expect_err("session claimed");
        assert_busy_refusal(&err, error_code::SESSION_CHANGING, "loner", None);
        drop(claim);

        let claim = state.begin_switch("hub").unwrap();
        let err = bind_session(&state, "loner", "hub").expect_err("owner claimed");
        assert_busy_refusal(
            &err,
            error_code::SESSION_OWNER_CHANGING,
            "loner",
            Some("hub"),
        );
        assert_eq!(
            state.store.get_session("loner").unwrap().unwrap().bound_to,
            None
        );
        let err = transfer_session(&state, "plain", "hub", "hub-2").expect_err("old owner claimed");
        assert_busy_refusal(
            &err,
            error_code::SESSION_OWNER_CHANGING,
            "plain",
            Some("hub"),
        );
        drop(claim);

        let claim = state.begin_switch("hub-2").unwrap();
        let err = transfer_session(&state, "plain", "hub", "hub-2").expect_err("new owner claimed");
        assert_busy_refusal(
            &err,
            error_code::SESSION_OWNER_CHANGING,
            "plain",
            Some("hub-2"),
        );
        assert_eq!(
            state
                .store
                .get_session("plain")
                .unwrap()
                .unwrap()
                .bound_to
                .as_deref(),
            Some("hub")
        );
        drop(claim);

        let (loner, _) = bind_session(&state, "loner", "hub").unwrap();
        assert_eq!(loner.bound_to.as_deref(), Some("hub"));
        let (plain, ..) = transfer_session(&state, "plain", "hub", "hub-2").unwrap();
        assert_eq!(plain.bound_to.as_deref(), Some("hub-2"));
    }

    /// A bound session, running or not, is released to an ordinary unbound project session: a
    /// lead session keeps the sessions bound to it, and one the user releases needs no owner
    /// named. Each refusal names its reason and changes nothing.
    #[tokio::test]
    async fn a_session_is_released_into_an_unbound_one_that_keeps_its_own_sessions() {
        use SessionStatus::{Idle, Interrupted};
        let (state, _dir, _live) = binding_fixture(
            "release",
            vec![
                console_session("hub", Idle),
                lead_session("lead", Idle, "hub"),
                bound_session("child", Idle, "lead"),
                bound_session("plain", Idle, "hub"),
                bound_session("dormant", Interrupted, "hub"),
                bound_session("by-user", Idle, "hub"),
                project_session("loner", Agent::Claude, Idle),
            ],
            &["hub", "lead", "child", "plain", "by-user"],
        );

        for (session, owner, code) in [
            ("hub", Some("hub"), error_code::CONSOLE_SESSION_NEVER_BOUND),
            ("loner", Some("hub"), error_code::SESSION_NOT_BOUND),
            ("loner", None, error_code::SESSION_NOT_BOUND),
            (
                "plain",
                Some("lead"),
                error_code::SESSION_NOT_BOUND_TO_OWNER,
            ),
            ("child", Some("hub"), error_code::SESSION_NOT_BOUND_TO_OWNER),
        ] {
            let before = state.store.get_session(session).unwrap().unwrap();
            let err = release_session(&state, session, owner, None).expect_err(code);
            assert_eq!(code_of(&err), code, "{session}: {err}");
            let after = state.store.get_session(session).unwrap().unwrap();
            assert_eq!((before.bound_to, before.lead), (after.bound_to, after.lead));
        }

        let lead = release_session(&state, "lead", Some("hub"), None)
            .unwrap()
            .session;
        assert!(!lead.is_worker());
        assert_eq!((lead.bound_to, lead.lead), (None, false));
        let child = state.store.get_session("child").unwrap().unwrap();
        assert_eq!(child.bound_to.as_deref(), Some("lead"));

        let plain = release_session(&state, "plain", Some("hub"), None)
            .unwrap()
            .session;
        assert_eq!((plain.bound_to, plain.lead), (None, false));
        let dormant = release_session(&state, "dormant", Some("hub"), None)
            .unwrap()
            .session;
        assert_eq!(dormant.bound_to, None);
        let by_user = release_session(&state, "by-user", None, None).unwrap();
        assert_eq!(by_user.former_owner, "hub");
        let by_user = by_user.session;
        assert_eq!(by_user.bound_to, None);
        let err = release_session(&state, "by-user", None, None).expect_err("already released");
        assert_eq!(code_of(&err), error_code::SESSION_NOT_BOUND, "{err}");

        // Unbound now, it can be bound again like any unbound project session.
        let (rebound, _) = bind_session(&state, "plain", "hub").unwrap();
        assert_eq!(rebound.bound_to.as_deref(), Some("hub"));
    }

    /// Relaunches the dormant session `id` with a stand-in launch, giving the launch prompt, the
    /// messages queued for it and the flag's count afterwards, or the launch's failure.
    async fn relaunch_stand_in(
        state: &Arc<AppState>,
        id: &str,
        fail: bool,
    ) -> Result<(Option<String>, Vec<String>)> {
        use crate::test_support::{stand_in_launch, Launched, StandIn};
        use std::sync::Mutex;
        let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
        let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));
        let claim = state.begin_launch(id).unwrap();
        let launch = stand_in_launch(&started, &launches, || {});
        relaunch_session_with(state, &claim, id, None, true, None, None, move |request| {
            if fail {
                anyhow::bail!("the agent could not start");
            }
            launch(request)
        })
        .await?;
        let prompt = launches.lock().unwrap()[0].2.clone();
        Ok((prompt, state.take_queued(id)))
    }

    fn dormant_with_conversation(id: &str, agent: Agent, owner: &str) -> Session {
        Session {
            agent,
            agent_session_id: Some(format!("conversation-{id}")),
            has_conversation: true,
            ..bound_session(id, SessionStatus::Interrupted, owner)
        }
    }

    /// A dormant session whose owner changed is told when it next starts: the notice is built at
    /// launch from the binding as it stands, ahead of the launch's own instruction. A release and
    /// a transfer leave one, a later change replaces an earlier pending notice, and once the
    /// launch has delivered it nothing is left.
    #[tokio::test]
    async fn a_dormant_session_is_told_of_its_new_owner_when_it_next_starts() {
        use SessionStatus::Idle;
        let (state, _dir, _live) = binding_fixture(
            "owner-notice",
            vec![
                console_session("hub", Idle),
                console_session("hub-2", Idle),
                dormant_with_conversation("released", Agent::Claude, "hub"),
                dormant_with_conversation("handed", Agent::Claude, "hub"),
                dormant_with_conversation("replaced", Agent::Claude, "hub"),
            ],
            &["hub", "hub-2"],
        );

        // A release leaves the notice owed, decided with the binding change.
        let Released {
            session: released,
            running,
            ..
        } = release_session(&state, "released", Some("hub"), None).unwrap();
        assert!(!running);
        assert!(mcp::tell_released(&state, &released, running, None)
            .await
            .unwrap()
            .is_none());
        assert_eq!(state.store.owner_notice_pending("released").unwrap(), 1);
        let (prompt, _) = relaunch_stand_in(&state, "released", false).await.unwrap();
        assert!(prompt.unwrap().contains("you have no parent session now"));
        assert_eq!(state.store.owner_notice_pending("released").unwrap(), 0);

        // A transfer, which a session with no process takes.
        let args = serde_json::json!({ "session": "handed", "to": "hub-2", "note": "carry on" });
        let result = mcp::exec::call(&state, "hub", "transfer_session", args.as_object().unwrap())
            .await
            .unwrap();
        assert!(
            result["note"]
                .as_str()
                .unwrap()
                .contains("will be told when it next starts"),
            "{result}"
        );
        let (prompt, _) = relaunch_stand_in(&state, "handed", false).await.unwrap();
        let prompt = prompt.unwrap();
        assert!(
            prompt.contains("your parent session has changed"),
            "{prompt}"
        );
        assert!(prompt.contains("`hub-2`"), "{prompt}");
        assert_eq!(state.store.owner_notice_pending("handed").unwrap(), 0);

        // A later change replaces the pending notice: it states the owner as it then stands.
        release_session(&state, "replaced", Some("hub"), None).unwrap();
        state
            .store
            .set_binding("replaced", None, "hub-2", true, false, false)
            .unwrap()
            .unwrap();
        mcp::running_or_leave(&state, &state.session_record("replaced").unwrap()).unwrap();
        let (prompt, _) = relaunch_stand_in(&state, "replaced", false).await.unwrap();
        let prompt = prompt.unwrap();
        assert!(prompt.contains("`hub-2`"), "{prompt}");
        assert!(
            !prompt.contains("you have no parent session now"),
            "{prompt}"
        );
        assert_eq!(state.store.owner_notice_pending("replaced").unwrap(), 0);
    }

    /// A launch that fails delivers nothing and keeps the notice owed, and a launch that is not a
    /// Claude Code resume delivers it through the queue once the agent takes input.
    #[tokio::test]
    async fn a_failed_launch_keeps_the_notice_and_a_queued_launch_delivers_it() {
        use SessionStatus::Idle;
        let (state, _dir, _live) = binding_fixture(
            "owner-notice-launch",
            vec![
                console_session("hub", Idle),
                dormant_with_conversation("claude", Agent::Claude, "hub"),
                dormant_with_conversation("codex", Agent::Codex, "hub"),
            ],
            &["hub"],
        );
        for id in ["claude", "codex"] {
            release_session(&state, id, None, None).unwrap();
        }

        relaunch_stand_in(&state, "claude", true)
            .await
            .expect_err("the launch fails");
        assert_eq!(state.store.owner_notice_pending("claude").unwrap(), 1);
        assert!(state.take_queued("claude").is_empty());
        assert_eq!(
            state.store.get_session("claude").unwrap().unwrap().status,
            SessionStatus::Interrupted
        );
        let (prompt, _) = relaunch_stand_in(&state, "claude", false).await.unwrap();
        assert!(prompt.unwrap().contains("you have no parent session now"));

        let (prompt, queued) = relaunch_stand_in(&state, "codex", false).await.unwrap();
        assert!(prompt.is_none());
        assert_eq!(queued.len(), 1);
        assert!(
            queued[0].contains("you have no parent session now"),
            "{queued:?}"
        );
        assert_eq!(state.store.owner_notice_pending("codex").unwrap(), 0);
    }

    /// Information shared with an interrupted session stays in the store through a launch that
    /// fails, and through a successful one until its message has been written into the session:
    /// a queue lost before then (the process ends, the daemon exits) leaves the rows for the next
    /// resume, which queues each message once, in the order the information arrived. Archiving or
    /// deleting the session drops what is kept.
    #[tokio::test]
    async fn information_kept_for_an_interrupted_session_is_deleted_only_once_it_is_written() {
        use SessionStatus::Idle;
        let interrupt = |state: &Arc<AppState>, id: &str| {
            let mut session = state.session_record(id).unwrap();
            session.status = SessionStatus::Interrupted;
            state.store.update_session(&session).unwrap();
        };
        let (state, _dir, _live) = binding_fixture(
            "kept-info",
            vec![
                console_session("hub", Idle),
                dormant_with_conversation("codex", Agent::Codex, "hub"),
                dormant_with_conversation("archived-later", Agent::Codex, "hub"),
                dormant_with_conversation("deleted-later", Agent::Codex, "hub"),
            ],
            &["hub"],
        );
        let share = |target: &str, text: &str| {
            let (delivery, _) = crate::sharing::deliver_info(&state, "hub", target, text).unwrap();
            assert_eq!(delivery, crate::reporting::Delivery::Kept);
        };
        share("codex", "first news");
        share("codex", "second news");
        assert!(state.take_queued("codex").is_empty());

        relaunch_stand_in(&state, "codex", true)
            .await
            .expect_err("the launch fails");
        assert_eq!(state.store.kept_messages("codex").unwrap().len(), 2);
        assert_eq!(state.queued_count("codex"), 0);

        // A launch that succeeds queues them, and keeps the rows until they are written.
        let (_, queued) = relaunch_stand_in(&state, "codex", false).await.unwrap();
        assert_eq!(queued.len(), 2, "{queued:?}");
        assert!(queued[0].contains("first news"), "{queued:?}");
        assert!(queued[1].contains("second news"), "{queued:?}");
        assert_eq!(state.store.kept_messages("codex").unwrap().len(), 2);

        // The queue is lost before it drained, as when the daemon exits or the process ends: the
        // next resume queues each message once.
        state.end_process_for_test("codex");
        interrupt(&state, "codex");
        assert_eq!(state.store.kept_messages("codex").unwrap().len(), 2);
        let (_, queued) = relaunch_stand_in(&state, "codex", false).await.unwrap();
        assert_eq!(queued.len(), 2, "{queued:?}");

        // Written into the session, the rows go.
        state.end_process_for_test("codex");
        interrupt(&state, "codex");
        let started = Arc::new(std::sync::Mutex::new(Vec::new()));
        let launches = Arc::new(std::sync::Mutex::new(Vec::new()));
        let claim = state.begin_launch("codex").unwrap();
        relaunch_session_with(
            &state,
            &claim,
            "codex",
            None,
            true,
            None,
            None,
            crate::test_support::stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();
        drop(claim);
        assert_eq!(state.store.kept_messages("codex").unwrap().len(), 2);
        let live = state.live_session("codex").expect("running");
        state.flush_outbox("codex", SessionStatus::Idle);
        let seen = crate::test_support::output_containing(&live, "second news");
        assert!(seen.contains("first news"), "{seen}");
        assert!(state.store.kept_messages("codex").unwrap().is_empty());
        assert_eq!(state.queued_count("codex"), 0);

        // A share that meets a launch under way keeps nothing: the launch may already have read
        // what was kept.
        let claim = state.begin_launch("archived-later").unwrap();
        assert!(state
            .keep_message_if_not_running("archived-later", "too late")
            .unwrap()
            .is_none());
        drop(claim);
        assert!(state
            .store
            .kept_messages("archived-later")
            .unwrap()
            .is_empty());

        share("archived-later", "stale news");
        share("deleted-later", "stale news");
        archive_session(&state, "archived-later").unwrap();
        assert!(state
            .store
            .kept_messages("archived-later")
            .unwrap()
            .is_empty());
        state.discard_session("deleted-later").unwrap();
        assert!(state
            .store
            .kept_messages("deleted-later")
            .unwrap()
            .is_empty());
    }

    /// An owner that is interrupted, with no process running.
    fn interrupted_owner(id: &str) -> Session {
        Session {
            title: id.to_string(),
            agent_session_id: Some(format!("conversation-{id}")),
            has_conversation: true,
            ..project_session(id, Agent::Claude, SessionStatus::Interrupted)
        }
    }

    fn report(
        summary: &str,
        status: crate::reporting::ReportStatus,
    ) -> crate::reporting::Report<'_> {
        crate::reporting::Report {
            summary,
            status,
            open_items: &[],
            synthesised: false,
        }
    }

    /// A report to an owner that is interrupted is kept for it, and a `done` one archives the
    /// reporter as a delivered one would; the owner's resume writes it in, dated, in the order it
    /// came among the information kept for it.
    #[tokio::test]
    async fn a_report_to_an_interrupted_owner_is_kept_for_its_resume_in_order() {
        use crate::reporting::{deliver_report, ReportStatus};
        let (state, _dir, _live) = binding_fixture(
            "kept-report",
            vec![
                interrupted_owner("starter"),
                bound_session("worker", SessionStatus::Idle, "starter"),
                bound_session("asker", SessionStatus::Idle, "starter"),
                console_session("hub", SessionStatus::Idle),
            ],
            &["hub"],
        );
        let share = |text: &str| {
            crate::sharing::deliver_info(&state, "hub", "starter", text).unwrap();
        };

        share("first news");
        let note =
            deliver_report(&state, "worker", report("all done", ReportStatus::Done)).expect("kept");
        assert!(
            note.contains("kept for the project session `starter`"),
            "{note}"
        );
        assert!(note.contains("This session is now archived"), "{note}");
        assert_eq!(
            state.session_record("worker").unwrap().status,
            SessionStatus::Archived
        );
        let note = deliver_report(
            &state,
            "asker",
            report("which cache?", ReportStatus::NeedsDecision),
        )
        .expect("kept");
        assert!(!note.contains("archived"), "{note}");
        assert_eq!(
            state.session_record("asker").unwrap().status,
            SessionStatus::Idle
        );
        share("second news");

        let (_, queued) = relaunch_stand_in(&state, "starter", false).await.unwrap();
        assert_eq!(queued.len(), 4, "{queued:?}");
        assert!(queued[0].contains("first news"), "{queued:?}");
        assert!(
            queued[1].starts_with("Report from session worker"),
            "{queued:?}"
        );
        assert!(queued[1].contains("all done"), "{queued:?}");
        assert!(
            queued[1].contains("Kept for you while you were interrupted; it dates from "),
            "{queued:?}"
        );
        assert!(queued[2].contains("which cache?"), "{queued:?}");
        assert!(queued[3].contains("second news"), "{queued:?}");
    }

    /// A report to an archived owner is refused, keeps nothing and leaves the reporter as it was,
    /// and the refusal names the owner rather than reading as the reporter's own state; a reporter
    /// archived with its owner is told that too.
    #[test]
    fn a_report_to_an_archived_owner_is_refused_naming_the_owner() {
        use crate::reporting::{deliver_report, ReportStatus};
        let (state, _dir, _live) = binding_fixture(
            "report-archived-owner",
            vec![
                Session {
                    status: SessionStatus::Archived,
                    ..interrupted_owner("starter")
                },
                bound_session("left-out", SessionStatus::Idle, "starter"),
                bound_session("archived-with-it", SessionStatus::Archived, "starter"),
            ],
            &[],
        );

        let err = deliver_report(&state, "left-out", report("done", ReportStatus::Done))
            .expect_err("refused");
        assert_eq!(code_of(&err), error_code::SESSION_ARCHIVED, "{err}");
        assert!(
            err.to_string()
                .contains("your owner, the project session `starter`, is archived"),
            "{err}"
        );
        assert!(err.to_string().contains("not delivered"), "{err}");
        assert_eq!(
            state.session_record("left-out").unwrap().status,
            SessionStatus::Idle
        );

        let err = deliver_report(
            &state,
            "archived-with-it",
            report("done", ReportStatus::Done),
        )
        .expect_err("refused");
        assert!(
            err.to_string().contains("your owner `starter` is archived"),
            "{err}"
        );
        assert!(state.store.kept_messages("starter").unwrap().is_empty());
    }

    /// A notice Octoboard writes to an owner that is interrupted is kept for its resume and
    /// delivered dated; a kept notice that a session waits for the user gives way to anything
    /// later about that session, a later wait included. What is kept for an owner archived before
    /// its resume is dropped.
    #[tokio::test]
    async fn a_notice_to_an_interrupted_owner_is_kept_and_an_out_of_date_wait_dropped() {
        use crate::reporting::{tell_owner_it_ended, tell_owner_it_waits};
        let (state, _dir, _live) = binding_fixture(
            "kept-notice",
            vec![
                interrupted_owner("starter"),
                bound_session("worker", SessionStatus::WaitingUser, "starter"),
                bound_session("blocked", SessionStatus::WaitingUser, "starter"),
                interrupted_owner("archived-later"),
                bound_session("its-worker", SessionStatus::Interrupted, "archived-later"),
            ],
            &[],
        );

        tell_owner_it_waits(&state, "worker");
        state.turn_started("worker", Some("go on"));
        tell_owner_it_waits(&state, "worker");
        tell_owner_it_waits(&state, "blocked");
        assert_eq!(state.store.kept_messages("starter").unwrap().len(), 2);
        let worker = state.session_record("worker").unwrap();
        tell_owner_it_ended(&state, &worker);

        let (_, queued) = relaunch_stand_in(&state, "starter", false).await.unwrap();
        assert_eq!(queued.len(), 2, "{queued:?}");
        assert!(queued[0].contains("`blocked`"), "{queued:?}");
        assert!(queued[0].contains("is waiting for the user"), "{queued:?}");
        assert!(queued[1].contains("`worker`"), "{queued:?}");
        assert!(queued[1].contains("ended on its own"), "{queued:?}");
        for notice in &queued {
            assert!(
                notice.contains("Kept for you while you were interrupted"),
                "{notice}"
            );
        }

        let its_worker = state.session_record("its-worker").unwrap();
        tell_owner_it_ended(&state, &its_worker);
        assert_eq!(
            state.store.kept_messages("archived-later").unwrap().len(),
            1
        );
        archive_session(&state, "archived-later").unwrap();
        assert!(state
            .store
            .kept_messages("archived-later")
            .unwrap()
            .is_empty());
    }

    /// A kept notice that a session waits for the user, which nothing later superseded, is not
    /// written into the owner at its resume once the session no longer waits.
    #[tokio::test]
    async fn a_kept_wait_that_is_over_is_not_delivered_at_the_resume() {
        use crate::reporting::tell_owner_it_waits;
        let (state, _dir, _live) = binding_fixture(
            "kept-wait-over",
            vec![
                interrupted_owner("starter"),
                bound_session("over", SessionStatus::WaitingUser, "starter"),
                bound_session("still", SessionStatus::WaitingUser, "starter"),
            ],
            &[],
        );
        tell_owner_it_waits(&state, "over");
        tell_owner_it_waits(&state, "still");
        assert_eq!(state.store.kept_messages("starter").unwrap().len(), 2);
        let mut over = state.session_record("over").unwrap();
        over.status = SessionStatus::Idle;
        state.store.update_session(&over).unwrap();

        let (_, queued) = relaunch_stand_in(&state, "starter", false).await.unwrap();

        assert_eq!(queued.len(), 1, "{queued:?}");
        assert!(queued[0].contains("`still`"), "{queued:?}");
        assert_eq!(state.store.kept_messages("starter").unwrap().len(), 1);
    }

    /// The parting note of an owner that releases a session that is not running is kept with the
    /// owed notice and told at the next launch, while the release is still the latest change of
    /// owner: a later transfer or bind drops it, and a blank note is not kept.
    #[tokio::test]
    async fn a_dormant_release_keeps_the_parting_note_for_the_next_launch() {
        use SessionStatus::Idle;
        let (state, _dir, _live) = binding_fixture(
            "release-note",
            vec![
                console_session("hub", Idle),
                console_session("hub-2", Idle),
                dormant_with_conversation("noted", Agent::Claude, "hub"),
                dormant_with_conversation("blank", Agent::Claude, "hub"),
                dormant_with_conversation("moved", Agent::Claude, "hub"),
                dormant_with_conversation("rebound", Agent::Claude, "hub"),
            ],
            &["hub", "hub-2"],
        );

        let Released {
            session: noted,
            running,
            ..
        } = release_session(&state, "noted", Some("hub"), Some("Pick up at step 3.")).unwrap();
        assert!(!running);
        assert!(
            mcp::tell_released(&state, &noted, running, Some("Pick up at step 3."))
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            state.store.owner_notice_note("noted").unwrap().as_deref(),
            Some("Pick up at step 3.")
        );
        let (prompt, _) = relaunch_stand_in(&state, "noted", false).await.unwrap();
        let prompt = prompt.unwrap();
        assert!(
            prompt.contains("you have no parent session now"),
            "{prompt}"
        );
        assert!(
            prompt.contains("Your former parent session left this note:"),
            "{prompt}"
        );
        assert!(prompt.contains("> Pick up at step 3."), "{prompt}");
        assert_eq!(state.store.owner_notice_pending("noted").unwrap(), 0);
        assert_eq!(state.store.owner_notice_note("noted").unwrap(), None);

        // The tool drops a blank note before the release.
        let args = serde_json::json!({ "session": "blank", "note": "   " });
        mcp::exec::call(&state, "hub", "release_session", args.as_object().unwrap())
            .await
            .unwrap();
        assert_eq!(state.store.owner_notice_note("blank").unwrap(), None);
        let (prompt, _) = relaunch_stand_in(&state, "blank", false).await.unwrap();
        let prompt = prompt.unwrap();
        assert!(
            prompt.contains("you have no parent session now"),
            "{prompt}"
        );
        assert!(!prompt.contains("former parent session left"), "{prompt}");

        // A transfer or a bind after the release makes the note stale, so it is dropped.
        release_session(&state, "moved", Some("hub"), Some("Pick up at step 3.")).unwrap();
        let (moved, _) = bind_session(&state, "moved", "hub-2").unwrap();
        assert_eq!(moved.bound_to.as_deref(), Some("hub-2"));
        assert_eq!(state.store.owner_notice_note("moved").unwrap(), None);
        release_session(&state, "rebound", Some("hub"), Some("Pick up at step 3.")).unwrap();
        state
            .store
            .set_binding("rebound", None, "hub-2", true, false, true)
            .unwrap()
            .unwrap();
        assert_eq!(state.store.owner_notice_note("rebound").unwrap(), None);
        for id in ["moved", "rebound"] {
            let (prompt, _) = relaunch_stand_in(&state, id, false).await.unwrap();
            let prompt = prompt.unwrap();
            assert!(prompt.contains("`hub-2`"), "{prompt}");
            assert!(!prompt.contains("Pick up at step 3."), "{prompt}");
        }
    }

    /// A release whose message cannot be written to a session that was running, its process gone
    /// meanwhile, leaves the notice owed with the parting note, and the note still goes at launch.
    #[tokio::test]
    async fn a_failed_write_after_a_release_keeps_the_note_owed() {
        use SessionStatus::Idle;
        let (state, _dir, _live) = binding_fixture(
            "release-note-failed-write",
            vec![
                console_session("hub", Idle),
                dormant_with_conversation("gone", Agent::Claude, "hub"),
            ],
            &["hub"],
        );

        // Judged running, as at the release, but no process is there to write to.
        let released = release_session(&state, "gone", Some("hub"), Some("Mind step 3."))
            .unwrap()
            .session;
        state.store.clear_owner_notice("gone", 1).unwrap();
        assert!(
            mcp::tell_released(&state, &released, true, Some("Mind step 3."))
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(state.store.owner_notice_pending("gone").unwrap(), 1);
        let (prompt, _) = relaunch_stand_in(&state, "gone", false).await.unwrap();
        let prompt = prompt.unwrap();
        assert!(prompt.contains("> Mind step 3."), "{prompt}");
    }

    /// A release by the user tells the owner the session was released from, when the owner is
    /// running: a dormant one is not told, and a later attempt by it to drive the session is refused
    /// as not its own. The released session is told as for any release.
    #[tokio::test]
    async fn a_release_by_the_user_tells_a_running_owner() {
        use crate::test_support::{heard_after_marker, output_containing, running};
        use SessionStatus::{Idle, Interrupted};
        let (state, _dir, _live) = binding_fixture(
            "release-by-user",
            vec![
                console_session("hub", Idle),
                console_session("hub-2", Interrupted),
                Session {
                    title: "Retry audit".to_string(),
                    ..bound_session("plain", Idle, "hub")
                },
                bound_session("orphan", Idle, "hub-2"),
            ],
            &[],
        );
        let hub = running(&state, "hub");
        let _plain = running(&state, "plain");
        let _orphan = running(&state, "orphan");
        let release = |session: &str| {
            handle(
                &state,
                None,
                RequestBody::ReleaseSession {
                    session: session.to_string(),
                },
            )
        };

        release("plain").await.unwrap();
        let told = output_containing(&hub, "released");
        assert!(told.contains("`plain`"), "{told}");
        assert!(told.contains("Retry audit"), "{told}");
        assert!(
            told.contains("no longer instruct, drive or archive it"),
            "{told}"
        );
        assert!(told.contains("left with the user"), "{told}");
        let send_message = mcp::qualified_tool_name(Agent::Claude, "send_message");
        assert!(told.contains(&format!("`{send_message}`")), "{told}");
        assert_eq!(
            state.store.get_session("plain").unwrap().unwrap().bound_to,
            None
        );

        // Nothing is owed to a dormant owner, and the release goes through.
        release("orphan").await.unwrap();
        assert_eq!(
            state.store.get_session("orphan").unwrap().unwrap().bound_to,
            None
        );
        assert_eq!(state.store.owner_notice_pending("hub-2").unwrap(), 0);
        assert!(state.take_queued("hub-2").is_empty());
        assert!(!heard_after_marker(&state, &hub).contains("`orphan`"));
    }

    /// A session, or its owner, under a handoff or switch claim is not released, and nothing
    /// changes; once the claim is released the same call goes through.
    #[tokio::test]
    async fn a_claimed_session_or_owner_is_not_released() {
        use SessionStatus::Idle;
        let (state, _dir, _live) = binding_fixture(
            "release-claimed",
            vec![
                console_session("hub", Idle),
                bound_session("plain", Idle, "hub"),
            ],
            &["hub", "plain"],
        );

        let claim = state.begin_switch("plain").unwrap();
        let err = release_session(&state, "plain", Some("hub"), None).expect_err("session claimed");
        assert_busy_refusal(&err, error_code::SESSION_CHANGING, "plain", None);
        drop(claim);

        let claim = state.begin_switch("hub").unwrap();
        let err = release_session(&state, "plain", None, None).expect_err("owner claimed");
        assert_busy_refusal(
            &err,
            error_code::SESSION_OWNER_CHANGING,
            "plain",
            Some("hub"),
        );
        assert_eq!(
            state
                .store
                .get_session("plain")
                .unwrap()
                .unwrap()
                .bound_to
                .as_deref(),
            Some("hub")
        );
        drop(claim);

        let released = release_session(&state, "plain", None, None)
            .unwrap()
            .session;
        assert_eq!(released.bound_to, None);
    }

    /// Every launch of a session — opening, resuming, reopening, a switch of its account — tells it
    /// who it reports to through `owner_of`, and a lead session is told nobody: it gets an unbound
    /// project session's role description and tools, as at its first launch, while a session
    /// bound to it is told the lead session owns it.
    #[test]
    fn a_lead_session_launches_as_an_unbound_project_session() {
        let (state, _dir) = console_with_project_and_accounts("coordinator-lead-launch");
        let lead = lead_session("lead", SessionStatus::Interrupted, "hub");
        let of_lead = bound_session("of-lead", SessionStatus::Interrupted, "lead");
        for session in [
            console_session("hub", SessionStatus::Idle),
            lead.clone(),
            of_lead.clone(),
        ] {
            state.store.insert_session(&session).unwrap();
        }

        assert_eq!(owner_of(&state, &lead).unwrap(), None);
        assert_eq!(
            owner_of(&state, &of_lead).unwrap(),
            Some(mcp::Owner::Project {
                session_id: "lead".to_string()
            })
        );
    }

    /// Archiving a console session reaches through its lead session to the lead session's own
    /// sessions: refused, naming it, while one of those has a process although the lead session
    /// itself has none, and otherwise archiving both levels.
    #[tokio::test]
    async fn archiving_a_console_session_covers_its_lead_sessions_own_sessions() {
        use SessionStatus::{Archived, Idle, Interrupted, Working};
        let (state, _dir) = console_with_project_and_accounts("coordinator-archive-two-levels");
        for session in [
            console_session("hub", Idle),
            lead_session("lead", Interrupted, "hub"),
            bound_session("running", Working, "lead"),
            bound_session("dormant", Interrupted, "lead"),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        let running = idle_stand_in("running");
        state.register_live(running.clone());

        let err = archive_session(&state, "hub").expect_err("a session two levels down runs");
        assert_eq!(code_of(&err), error_code::SESSION_HAS_RUNNING_SESSIONS);
        let coded = err.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.params["count"], "1");
        assert!(coded.params["sessions"].contains("running"));
        for (id, status) in [
            ("hub", Idle),
            ("lead", Interrupted),
            ("dormant", Interrupted),
        ] {
            assert_eq!(status_of(&state, id), Some(status), "{id}");
        }

        drop(running);
        state.store.delete_session("running").unwrap();
        archive_session(&state, "hub").unwrap();
        for id in ["hub", "lead", "dormant"] {
            assert_eq!(status_of(&state, id), Some(Archived), "{id}");
        }
    }

    /// Removing a project that holds a running lead session and its running sessions stops them
    /// all, the lead session's own first, and leaves the console session it is bound to running.
    #[tokio::test]
    async fn removing_a_project_with_a_lead_session_leaves_its_console_session_alone() {
        use SessionStatus::{Idle, Working};
        let (state, _dir) = console_with_project_and_accounts("coordinator-delete-project-lead");
        for session in [
            console_session("hub", Idle),
            lead_session("lead", Idle, "hub"),
            bound_session("worker", Working, "lead"),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        let live: Vec<_> = ["hub", "lead", "worker"]
            .into_iter()
            .map(|id| {
                let stand_in = idle_stand_in(id);
                state.register_live(stand_in.clone());
                stand_in
            })
            .collect();

        handle(
            &state,
            None,
            RequestBody::DeleteProject {
                project: "project-1".to_string(),
                stop_sessions: true,
                trash_directory: false,
                force_remove: Vec::new(),
                inspection: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(session_ids(&state), ["hub"]);
        assert_eq!(status_of(&state, "hub"), Some(Idle));
        assert!(!live[0].poll_exit(), "the console session keeps running");
    }

    /// Two console sessions in one console each keep their own page history: a submission is
    /// written into the console session that pushed the page and no other, and "the newest page"
    /// is the newest of that console session, so a later page from the other one does not make an
    /// earlier page read-only.
    #[tokio::test]
    async fn a_page_submission_reaches_the_console_session_that_pushed_it() {
        let (state, dir) =
            crate::test_support::app_state("coordinator-submit-page-routes-by-owner");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        let mut live = HashMap::new();
        for id in ["owner-a", "owner-b"] {
            state
                .store
                .insert_session(&bare_session(id, Role::Console))
                .unwrap();
            let session = idle_stand_in(id);
            state.register_live(session.clone());
            crate::session::spawn_reader_thread(session.clone(), 8 * 1024);
            live.insert(id, session);
        }
        for page in [
            crate::test_support::test_page("a-old", "owner-a", 1),
            crate::test_support::test_page("a-new", "owner-a", 3),
            crate::test_support::test_page("b-new", "owner-b", 4),
        ] {
            state.store.insert_page(&page).unwrap();
        }

        // A project session that faces the user can have pages too; only a session that does not
        // exist is refused.
        state
            .store
            .insert_session(&bare_session("worker", Role::Project))
            .unwrap();
        state
            .store
            .insert_page(&crate::test_support::test_page("w-1", "worker", 5))
            .unwrap();
        let listing = |session: &str| {
            handle(
                &state,
                None,
                RequestBody::ListPages {
                    console_session: session.to_string(),
                },
            )
        };
        assert!(matches!(
            listing("owner-a").await.unwrap(),
            Some(Event::PageList { ref pages, .. }) if pages.len() == 2
        ));
        assert!(matches!(
            listing("worker").await.unwrap(),
            Some(Event::PageList { ref pages, .. }) if pages.len() == 1
        ));
        let refused = listing("nobody").await.expect_err("no such session");
        assert_eq!(code_of(&refused), error_code::UNKNOWN_SESSION);

        let refused = submit_page(&state, "a-old", serde_json::json!({}))
            .await
            .expect_err("a history page of its console session");
        assert_eq!(code_of(&refused), error_code::PAGE_NOT_CURRENT);

        // The newest page of a session that has since become a bound one is refused as no page at
        // all.
        let mut bound = bare_session("bound-worker", Role::Project);
        bound.bound_to = Some("owner-a".to_string());
        state.store.insert_session(&bound).unwrap();
        state
            .store
            .insert_page(&crate::test_support::test_page("bw-1", "bound-worker", 6))
            .unwrap();
        let refused = submit_page(&state, "bw-1", serde_json::json!({}))
            .await
            .expect_err("a page of a session that has no page tools");
        assert_eq!(code_of(&refused), error_code::UNKNOWN_PAGE);

        submit_page(&state, "a-new", serde_json::json!({}))
            .await
            .expect("owner-a's newest page, though owner-b has a later one");

        let output_of = |id: &str| {
            let deadline = Instant::now() + Duration::from_millis(500);
            loop {
                let output = live[id].recent_output(8 * 1024);
                if !output.is_empty() || Instant::now() >= deadline {
                    return String::from_utf8_lossy(&output).into_owned();
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        assert!(output_of("owner-a").contains("a-new"));
        // Read once, after owner-a's delivery has landed. A mistaken write to owner-b would most
        // likely have arrived by now; this cannot rule out one that is still on its way.
        assert!(!live["owner-b"]
            .recent_output(8 * 1024)
            .windows(5)
            .any(|window| window == b"a-new"));
    }

    // -- opening a session without waiting for its agent --------------------------------------

    /// What opens a console session of the console `state_for_opening` sets up.
    fn open_request() -> OpenRequest {
        OpenRequest {
            console_id: "console-1".to_string(),
            project_id: None,
            agent: None,
            account: None,
            model: None,
            effort: None,
            task: None,
            title: None,
            origin: Origin::User,
            bound_to: None,
            resume_agent_session_id: None,
            session_id: None,
            terminal_appearance: None,
        }
    }

    fn state_for_opening(name: &str) -> (Arc<AppState>, ScratchDir) {
        let (state, dir) = crate::test_support::app_state(name);
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        (state, dir)
    }

    /// A launch that waits for `release` and then hands over `live` as the process it started.
    fn launch_after(
        release: std::sync::mpsc::Receiver<()>,
        live: Arc<LiveSession>,
    ) -> impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static {
        move |_| {
            release.recv().expect("released");
            Ok(term::Launch {
                session: live,
                agent_session_id: None,
                notice: None,
            })
        }
    }

    /// [`open_detached_with`], with the launch started as soon as the session is returned.
    async fn open_started(
        state: &Arc<AppState>,
        request: OpenRequest,
        launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
    ) -> Result<Session> {
        let (session, followup) = open_detached_with(state, request, launch).await?;
        drop(followup);
        Ok(session)
    }

    async fn wait_until(mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while !done() {
            assert!(Instant::now() < deadline, "timed out");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// The reply, and the broadcast, come while the agent is still being launched, with the session
    /// `starting`; the launch settling then moves it to the status a fresh session has.
    #[tokio::test]
    async fn a_session_is_shown_as_starting_while_its_agent_is_still_being_launched() {
        let (state, _dir) = state_for_opening("coordinator-open-detached");
        let mut events = state.subscribe();
        // Not known until the session is: the stand-in is made once the launch is asked for it.
        let (release, released) = std::sync::mpsc::channel();
        let (live_tx, live_rx) = std::sync::mpsc::channel::<Arc<LiveSession>>();
        let opened = open_started(&state, open_request(), move |request| {
            launch_after(released, live_rx.recv().expect("a process"))(request)
        })
        .await
        .expect("opened");

        assert_eq!(opened.status, SessionStatus::Starting);
        let stored = state.store.get_session(&opened.id).unwrap().unwrap();
        assert_eq!(stored.status, SessionStatus::Starting);
        assert!(state.is_launching(&opened.id));
        assert!(state.live_session(&opened.id).is_none());
        let published = loop {
            match events.try_recv().expect("the session was published") {
                Event::SessionUpserted { session } => break session,
                _ => continue,
            }
        };
        assert_eq!(published.status, SessionStatus::Starting);

        let stand_in = crate::test_support::idle_stand_in(&opened.id);
        live_tx.send((*stand_in).clone()).unwrap();
        release.send(()).unwrap();
        wait_until(|| !state.is_launching(&opened.id)).await;
        assert_eq!(
            state.store.get_session(&opened.id).unwrap().unwrap().status,
            SessionStatus::Idle
        );
        assert!(state.live_session(&opened.id).is_some());
    }

    /// A launch that fails leaves nothing behind: the session is deleted again, and the user is told
    /// why by a notice that names the session by its title, which no longer exists to be looked up.
    /// The failure's own code and params travel in the notice, for a client to word the reason.
    #[tokio::test]
    async fn a_launch_that_fails_deletes_the_session_and_says_why() {
        let (state, _dir) = state_for_opening("coordinator-open-detached-fails");
        let mut events = state.subscribe();
        let opened = open_started(&state, open_request(), |_| {
            Err(CodedError::raised(
                error_code::BINARY_NOT_FOUND,
                "`claude` was not found".to_string(),
                &[("binary", "claude")],
            ))
        })
        .await
        .expect("replied before the launch");

        wait_until(|| state.store.get_session(&opened.id).unwrap().is_none()).await;
        assert!(!state.is_launching(&opened.id));
        let (mut deleted, mut notice) = (false, None);
        while let Ok(event) = events.try_recv() {
            match event {
                Event::SessionDeleted { session } if session == opened.id => deleted = true,
                Event::SessionNotice {
                    session,
                    code,
                    params,
                    ..
                } if session == opened.id => notice = Some((code, params)),
                _ => {}
            }
        }
        assert!(deleted, "clients drop the row they were shown");
        let (code, params) = notice.expect("a notice");
        assert_eq!(code, notice_code::LAUNCH_FAILED);
        assert_eq!(params["reason"], "`claude` was not found");
        assert_eq!(params["reason_code"], error_code::BINARY_NOT_FOUND);
        assert_eq!(params["binary"], "claude");
        assert_eq!(params["title"], opened.title);
    }

    /// The caller that waits for the launch gets its refusal as the error, as it always did, and no
    /// notice besides: the error is how it is told.
    #[tokio::test]
    async fn a_session_opened_by_a_caller_that_waits_reports_a_failed_launch_as_its_error() {
        let (state, _dir) = state_for_opening("coordinator-open-waits");
        let mut events = state.subscribe();
        let starting = begin_open(&state, open_request()).await.expect("recorded");
        let id = starting.session.id.clone();
        let err = finish_open(&state, starting, |_| Err(anyhow::anyhow!("no binary")))
            .await
            .map_err(OpenError::into_error)
            .expect_err("refused");
        assert_eq!(format!("{err}"), "no binary");
        assert!(state.store.get_session(&id).unwrap().is_none());
        while let Ok(event) = events.try_recv() {
            assert!(!matches!(event, Event::SessionNotice { .. }));
        }
    }

    /// Archiving a session that is still starting stands: when the process comes up it is ended
    /// rather than left running under a session in the archive.
    #[tokio::test]
    async fn a_session_archived_while_starting_does_not_keep_the_process_it_launched() {
        let (state, _dir) = state_for_opening("coordinator-open-archived-meanwhile");
        let (release, released) = std::sync::mpsc::channel();
        let (live_tx, live_rx) = std::sync::mpsc::channel::<Arc<LiveSession>>();
        let (started_tx, started) = std::sync::mpsc::channel();
        let opened = open_started(&state, open_request(), move |request| {
            started_tx.send(()).unwrap();
            launch_after(released, live_rx.recv().expect("a process"))(request)
        })
        .await
        .expect("opened");
        wait_until(|| started.try_recv().is_ok()).await;

        archive_session(&state, &opened.id).expect("archived while starting");
        let stand_in = crate::test_support::idle_stand_in(&opened.id);
        live_tx.send((*stand_in).clone()).unwrap();
        release.send(()).unwrap();

        wait_until(|| !state.is_launching(&opened.id)).await;
        wait_until(|| stand_in.poll_exit()).await;
        wait_until(|| state.live_session(&opened.id).is_none()).await;
        assert_eq!(
            state.store.get_session(&opened.id).unwrap().unwrap().status,
            SessionStatus::Archived
        );
    }

    /// Archived before the launch has begun, a session is not launched at all.
    #[tokio::test]
    async fn a_session_archived_before_its_launch_begins_is_never_launched() {
        let (state, _dir) = state_for_opening("coordinator-open-archived-before");
        let launched = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let opened = open_started(&state, open_request(), {
            let launched = launched.clone();
            move |_| {
                launched.store(true, std::sync::atomic::Ordering::SeqCst);
                Err(anyhow::anyhow!("launched"))
            }
        })
        .await
        .expect("opened");
        archive_session(&state, &opened.id).expect("archived while starting");

        wait_until(|| !state.is_launching(&opened.id)).await;
        assert_eq!(
            state.store.get_session(&opened.id).unwrap().unwrap().status,
            SessionStatus::Archived
        );
        assert!(!launched.load(std::sync::atomic::Ordering::SeqCst));
        assert!(state.live_session(&opened.id).is_none());
    }

    /// A session opened to continue an outside conversation is launched with it, after the reply.
    #[tokio::test]
    async fn a_conversation_to_resume_reaches_the_launch_after_the_reply() {
        const ID: &str = "11111111-2222-3333-4444-555555555555";
        let (state, dir) = console_with_project_and_accounts("coordinator-resume-id-launched");
        let record = dir
            .join("claude-a/projects/slug")
            .join(format!("{ID}.jsonl"));
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        std::fs::write(&record, "").unwrap();
        let (asked_tx, asked) = std::sync::mpsc::channel();
        let opened = open_started(
            &state,
            OpenRequest {
                project_id: Some("project-1".to_string()),
                account: Some(Some("claude-a".to_string())),
                resume_agent_session_id: Some(ID.to_string()),
                ..open_request()
            },
            move |request| {
                asked_tx.send(request.resume_agent_session_id).unwrap();
                Err(anyhow::anyhow!("no agent in the tests"))
            },
        )
        .await
        .expect("opened");

        assert_eq!(opened.status, SessionStatus::Starting);
        assert_eq!(opened.agent_session_id.as_deref(), Some(ID));
        assert!(opened.has_conversation);
        let mut resumed = None;
        wait_until(|| {
            resumed = asked.try_recv().ok();
            resumed.is_some()
        })
        .await;
        assert_eq!(resumed.flatten().as_deref(), Some(ID));
    }

    /// A session that is starting cannot be deleted or relaunched out from under its launch.
    #[tokio::test]
    async fn a_starting_session_is_not_deleted_or_resumed() {
        let (state, dir) = state_for_opening("coordinator-open-starting-refusals");
        let (release, released) = std::sync::mpsc::channel();
        let (live_tx, live_rx) = std::sync::mpsc::channel::<Arc<LiveSession>>();
        let opened = open_started(&state, open_request(), move |request| {
            launch_after(released, live_rx.recv().expect("a process"))(request)
        })
        .await
        .expect("opened");

        let err = delete_session(&state, &opened.id).expect_err("not while starting");
        assert_eq!(code_of(&err), error_code::SESSION_NOT_DELETABLE);
        let err = resume_session_with(&state, &opened.id, None, None, stub_shell(&dir))
            .await
            .expect_err("already being started");
        assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);

        let stand_in = crate::test_support::idle_stand_in(&opened.id);
        live_tx.send((*stand_in).clone()).unwrap();
        release.send(()).unwrap();
        wait_until(|| !state.is_launching(&opened.id)).await;
    }

    /// The launch is the reply's follow-up: nothing is launched while it is held, so a launch that
    /// fails at once cannot delete the session on a client before the reply has told it of it.
    #[tokio::test]
    async fn the_launch_waits_for_the_reply_to_be_queued() {
        let (state, _dir) = state_for_opening("coordinator-open-after-reply");
        let launched = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (opened, followup) = open_detached_with(&state, open_request(), {
            let launched = launched.clone();
            move |_| {
                launched.store(true, std::sync::atomic::Ordering::SeqCst);
                Err(anyhow::anyhow!("no binary"))
            }
        })
        .await
        .expect("opened");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!launched.load(std::sync::atomic::Ordering::SeqCst));
        assert!(state.store.get_session(&opened.id).unwrap().is_some());

        drop(followup);
        wait_until(|| state.store.get_session(&opened.id).unwrap().is_none()).await;
        assert!(launched.load(std::sync::atomic::Ordering::SeqCst));
    }

    /// A session still starting is refused as an owner: its launch may fail, which deletes it
    /// whatever is bound to it.
    #[tokio::test]
    async fn a_starting_session_is_refused_as_an_owner() {
        let (state, _dir) = state_for_opening("coordinator-open-starting-owner");
        let (owner, followup) = open_detached_with(&state, open_request(), |_| {
            Err(anyhow::anyhow!("never launched"))
        })
        .await
        .expect("opened");
        let err = resolve_bound_to(
            &state,
            "console-1",
            Some("project-1"),
            Role::Project,
            Some(owner.id.clone()),
        )
        .expect_err("starting");
        assert_eq!(code_of(&err), error_code::UNKNOWN_SESSION);
        drop(followup);
        wait_until(|| !state.is_launching(&owner.id)).await;
    }

    #[test]
    fn a_group_follows_the_bindings_two_levels_and_a_bound_role_owns_none() {
        let sessions = [
            Session {
                bound_to: Some("hub".to_string()),
                ..bare_session("member", Role::Project)
            },
            Session {
                bound_to: Some("member".to_string()),
                ..bare_session("stray", Role::Project)
            },
            Session {
                bound_to: Some("hub".to_string()),
                lead: true,
                ..bare_session("lead", Role::Project)
            },
            Session {
                bound_to: Some("lead".to_string()),
                ..bare_session("led", Role::Project)
            },
            bare_session("hub", Role::Console),
        ];
        let ids = |group: Vec<&Session>| -> Vec<String> {
            group.iter().map(|session| session.id.clone()).collect()
        };
        assert_eq!(
            ids(group_in(&sessions, &sessions[4])),
            ["hub", "member", "lead", "led"]
        );
        assert_eq!(ids(group_in(&sessions, &sessions[0])), ["member"]);
        assert_eq!(ids(group_in(&sessions, &sessions[2])), ["lead", "led"]);
    }

    /// What the switched session's process started is carried over to its relaunch on the new
    /// account and keeps running, listed under the session; the relaunched process stops it
    /// when it ends.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_switch_carries_what_the_process_started_over_to_the_relaunch() {
        use crate::test_support::{stand_in_launch, Launched, StandIn};
        let (state, dir) = console_with_project_and_accounts("coordinator-switch-carry-over");
        std::fs::create_dir_all(dir.join("claude-a")).unwrap();
        state
            .store
            .insert_session(&project_session("s", Agent::Claude, SessionStatus::Idle))
            .unwrap();
        let (first, child) = crate::test_support::running_leaving_one(&state, &dir, "s");
        let _guard = crate::test_support::KillOnDrop(vec![child]);
        let started = Arc::new(std::sync::Mutex::new(Vec::<StandIn>::new()));
        let launches = Arc::new(std::sync::Mutex::new(Vec::<Launched>::new()));

        switch_session_account_with(
            &state,
            "s",
            Some(Some("claude-a".to_string())),
            None,
            None,
            LaunchEdit::default(),
            || Ok(HashMap::new()),
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();

        let relaunched = state.live_session("s").expect("relaunched");
        assert_ne!(relaunched.launch_id, first.launch_id);
        state.process_ledger.tick_now();
        assert!(
            crate::test_support::is_running(child),
            "stopped with the process switched away from"
        );
        let trees = state
            .process_ledger
            .trees_of_sessions(&["s".to_string()], Duration::ZERO);
        assert_eq!(trees.len(), 1, "{trees:?}");
        assert!(trees[0].id.starts_with(&format!("{child}:")));

        stop_process(&state, relaunched).await.unwrap();
        assert!(
            crate::test_support::stops_running(&state, child).await,
            "kept running past the relaunch"
        );
    }

    /// A switch that fails once it has ended the session's process — its relaunch does not start,
    /// or starts and ends at once — leaves the session interrupted, and what its process had
    /// started running and kept for the user among its project's processes.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_switch_that_fails_keeps_what_the_process_started_under_its_project() {
        for comes_up_and_dies in [false, true] {
            let (state, dir) = console_with_project_and_accounts("coordinator-switch-carry-failed");
            std::fs::create_dir_all(dir.join("claude-a")).unwrap();
            state
                .store
                .insert_session(&project_session("s", Agent::Claude, SessionStatus::Idle))
                .unwrap();
            let (_first, child) = crate::test_support::running_leaving_one(&state, &dir, "s");
            let _guard = crate::test_support::KillOnDrop(vec![child]);
            let started = Arc::new(std::sync::Mutex::new(
                Vec::<crate::test_support::StandIn>::new(),
            ));
            let kept_alive = started.clone();
            let launch = move |request: term::LaunchRequest| {
                if !comes_up_and_dies {
                    anyhow::bail!("the agent could not be started");
                }
                let live = crate::test_support::tagged_live_session(
                    &request.session_id,
                    request.agent,
                    "exit 1",
                    &request.process_tag,
                );
                let process = (*live).clone();
                kept_alive.lock().unwrap().push(live);
                Ok(term::Launch {
                    session: process,
                    agent_session_id: None,
                    notice: None,
                })
            };

            switch_session_account_with(
                &state,
                "s",
                Some(Some("claude-a".to_string())),
                None,
                None,
                LaunchEdit::default(),
                || Ok(HashMap::new()),
                launch,
            )
            .await
            .expect_err("the relaunch failed");

            assert_eq!(status_of(&state, "s"), Some(SessionStatus::Interrupted));
            state.process_ledger.tick_now();
            assert!(
                crate::test_support::is_running(child),
                "stopped with a failed switch ({comes_up_and_dies})"
            );
            let listed = state.process_ledger.trees_of_project(&[], "project-1", &[]);
            assert_eq!(listed.len(), 1, "{listed:?}");
            assert!(listed[0].id.starts_with(&format!("{child}:")));
            assert!(listed[0].kept && listed[0].session_id.is_none());
        }
    }

    /// A process that had exited by itself before the switch is not the switch's doing, so when
    /// the switch fails, what it had started is stopped as for any agent that exits on its own,
    /// not kept under the project.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_switch_that_fails_stops_what_a_process_that_had_exited_by_itself_started() {
        let (state, dir) = console_with_project_and_accounts("coordinator-switch-exited-failed");
        std::fs::create_dir_all(dir.join("claude-a")).unwrap();
        state
            .store
            .insert_session(&project_session("s", Agent::Claude, SessionStatus::Idle))
            .unwrap();
        let (pid_file, go_file) = (dir.join("s-child"), dir.join("go"));
        let live = fake_live_session(
            "s",
            Agent::Claude,
            80,
            24,
            &format!(
                "/usr/bin/perl -MPOSIX -e 'POSIX::setsid(); {} exec \"/bin/sleep\", \"300\";' &\n\
                 while [ ! -e {go} ]; do sleep 0.05; done\n\
                 exit 0",
                crate::test_support::perl_write(&pid_file, "$$"),
                go = go_file.display()
            ),
        );
        state.register_live(live.clone());
        let child = crate::test_support::wait_for_pid(&pid_file);
        let _guard = crate::test_support::KillOnDrop(vec![child]);
        // The agent's shell is still there for this read, which attributes what it started.
        state.process_ledger.scan_before_ending();
        std::fs::write(&go_file, "").unwrap();
        let exited = live.clone();
        assert!(
            crate::test_support::within(crate::test_support::PATIENCE, move || exited.poll_exit())
                .await,
            "the process exits by itself"
        );
        // Still registered, as in the moment before the exit watcher deregisters it.
        let watched = live.clone();
        let watching = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            watching.watch_exit(watched);
        });

        switch_session_account_with(
            &state,
            "s",
            Some(Some("claude-a".to_string())),
            None,
            None,
            LaunchEdit::default(),
            || Ok(HashMap::new()),
            |_| Err(anyhow::anyhow!("the agent could not be started")),
        )
        .await
        .expect_err("the relaunch failed");

        assert!(
            crate::test_support::stops_running(&state, child).await,
            "kept past a switch that did not end the process"
        );
        assert!(state
            .process_ledger
            .trees_of_project(&[], "project-1", &[])
            .is_empty());
    }

    /// A session archived while its switch was under way keeps nothing either: what its process
    /// had started is stopped, as an archive stops it.
    #[tokio::test(flavor = "multi_thread")]
    async fn what_a_switch_held_for_a_session_archived_meanwhile_is_stopped() {
        let (state, dir) = console_with_project_and_accounts("coordinator-switch-archived");
        state
            .store
            .insert_session(&project_session(
                "s",
                Agent::Claude,
                SessionStatus::Archived,
            ))
            .unwrap();
        let (first, child) = crate::test_support::running_leaving_one(&state, &dir, "s");
        let _guard = crate::test_support::KillOnDrop(vec![child]);
        let ledger = &state.process_ledger;
        let held = ledger.hold_for_launch("s");
        ledger.hand_over(&first.launch_id, "s");
        ledger.end_launch(&first.launch_id);
        assert!(crate::test_support::is_running(child));

        keep_what_a_failed_restart_carried(&state, "s", held);

        assert!(
            crate::test_support::stops_running(&state, child).await,
            "kept for an archived session"
        );
        assert!(ledger.trees_of_project(&[], "project-1", &[]).is_empty());
    }

    /// A console session keeps nothing: what its process had started is stopped when its switch
    /// fails, as an interrupted session's is.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_console_session_whose_switch_fails_stops_what_its_process_started() {
        let (state, dir) = console_with_project_and_accounts("coordinator-switch-console-failed");
        std::fs::create_dir_all(dir.join("claude-a")).unwrap();
        state
            .store
            .insert_session(&console_session("hub", SessionStatus::Idle))
            .unwrap();
        let (_first, child) = crate::test_support::running_leaving_one(&state, &dir, "hub");
        let _guard = crate::test_support::KillOnDrop(vec![child]);

        switch_session_account_with(
            &state,
            "hub",
            Some(Some("claude-a".to_string())),
            None,
            None,
            LaunchEdit::default(),
            || Ok(HashMap::new()),
            |_| Err(anyhow::anyhow!("the agent could not be started")),
        )
        .await
        .expect_err("the relaunch failed");

        assert!(
            crate::test_support::stops_running(&state, child).await,
            "kept running past the failed switch"
        );
    }

    /// What the process of a session whose model is changed started is carried over to the
    /// relaunch and keeps running, listed under the session.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_model_change_carries_what_the_process_started_over_to_the_relaunch() {
        use crate::test_support::{stand_in_launch, Launched, StandIn};
        let (state, dir) = console_with_project_and_accounts("coordinator-model-carry-over");
        state
            .store
            .insert_session(&project_session("s", Agent::Claude, SessionStatus::Idle))
            .unwrap();
        let (first, child) = crate::test_support::running_leaving_one(&state, &dir, "s");
        let _guard = crate::test_support::KillOnDrop(vec![child]);
        let started = Arc::new(std::sync::Mutex::new(Vec::<StandIn>::new()));
        let launches = Arc::new(std::sync::Mutex::new(Vec::<Launched>::new()));

        set_session_model_with(
            &state,
            "s",
            LaunchEdit {
                model: Some(Some("opus".to_string())),
                effort: None,
            },
            ModelAsker::User,
            None,
            shell_env(&dir),
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();

        let relaunched = state.live_session("s").expect("relaunched");
        assert_ne!(relaunched.launch_id, first.launch_id);
        state.process_ledger.tick_now();
        assert!(
            crate::test_support::is_running(child),
            "stopped with the process restarted"
        );
        let trees = state
            .process_ledger
            .trees_of_sessions(&["s".to_string()], Duration::ZERO);
        assert_eq!(trees.len(), 1, "{trees:?}");
        assert!(trees[0].id.starts_with(&format!("{child}:")));

        stop_process(&state, relaunched).await.unwrap();
        assert!(
            crate::test_support::stops_running(&state, child).await,
            "kept running past the relaunch"
        );
    }

    /// A model change that fails once it has ended the session's process leaves the session
    /// interrupted, and what its process had started running and kept among its project's
    /// processes.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_model_change_that_fails_keeps_what_the_process_started_under_its_project() {
        let (state, dir) = console_with_project_and_accounts("coordinator-model-carry-failed");
        state
            .store
            .insert_session(&project_session("s", Agent::Claude, SessionStatus::Idle))
            .unwrap();
        let (_first, child) = crate::test_support::running_leaving_one(&state, &dir, "s");
        let _guard = crate::test_support::KillOnDrop(vec![child]);

        set_session_model_with(
            &state,
            "s",
            LaunchEdit {
                model: Some(Some("opus".to_string())),
                effort: None,
            },
            ModelAsker::User,
            None,
            shell_env(&dir),
            |_| Err(anyhow::anyhow!("the agent could not be started")),
        )
        .await
        .expect_err("the relaunch failed");

        assert_eq!(status_of(&state, "s"), Some(SessionStatus::Interrupted));
        state.process_ledger.tick_now();
        assert!(
            crate::test_support::is_running(child),
            "stopped with a failed model change"
        );
        let listed = state.process_ledger.trees_of_project(&[], "project-1", &[]);
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert!(listed[0].id.starts_with(&format!("{child}:")));
        assert!(listed[0].kept && listed[0].session_id.is_none());
    }

    /// A console session keeps nothing: what its process had started is stopped when its model
    /// change fails.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_console_session_whose_model_change_fails_stops_what_its_process_started() {
        let (state, dir) = console_with_project_and_accounts("coordinator-model-console-failed");
        state
            .store
            .insert_session(&console_session("hub", SessionStatus::Idle))
            .unwrap();
        let (_first, child) = crate::test_support::running_leaving_one(&state, &dir, "hub");
        let _guard = crate::test_support::KillOnDrop(vec![child]);

        set_session_model_with(
            &state,
            "hub",
            LaunchEdit {
                model: Some(Some("opus".to_string())),
                effort: None,
            },
            ModelAsker::User,
            None,
            shell_env(&dir),
            |_| Err(anyhow::anyhow!("the agent could not be started")),
        )
        .await
        .expect_err("the relaunch failed");

        assert!(
            crate::test_support::stops_running(&state, child).await,
            "kept running past the failed model change"
        );
    }
}
