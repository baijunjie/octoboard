//! The coordinator role: what each control-socket request does to the stored consoles, projects and
//! sessions, and which host-role work it triggers — including the launch flow every way of starting
//! a session goes through.
//!
//! This file dispatches each request; the submodules carry the requests out:
//!
//! - [`consoles`], [`projects`]: creating, changing and removing consoles and projects.
//! - [`open`], [`resume`], [`launch`]: opening a session, resuming one, and starting and stopping
//!   the agent process of a session, with the checks of its launch settings.
//! - [`bindings`]: which session reports to which, and the walks of a team.
//! - [`lifecycle`]: archiving and deleting sessions with what goes with them, and their title, pin,
//!   lock and colour.
//! - [`account_switch`], [`model_change`], [`account_sharing`]: moving a session to another
//!   account, or to another model or effort, and a change of an account's sharing, which ends and
//!   resumes its idle sessions.
//! - [`restart`]: what those three share when they restart a session's process.
//! - [`account_choice`]: which agent and account a session opens with, and the refusals of one that
//!   cannot open it.
//! - [`accounts`], [`settings`]: the accounts and the agent list, and the settings.
//! - [`messages`]: the user's own writes into a session.
//! - [`validate`]: the checks of the text fields requests carry.
//! - [`pages`], [`archive_cleanup`], [`exit_watch`]: not request handlers. `pages` withdraws a
//!   worker's pages from the report panel, and sweeps at daemon start the pages left on workers;
//!   `archive_cleanup` is the daemon task that deletes archived sessions whose conversation can no
//!   longer be resumed; `exit_watch` is the exit watcher every launch starts.
//!
//! Writing into a running session is `crate::messaging::delivery`'s, the notices an owner is sent
//! are `crate::messaging::owner_notices`', and the console session's reports, synthesis and
//! automatic archiving are `crate::messaging::reporting`'s, because the console session's tools and
//! the hooks (`crate::hooks::apply`) reach them without going through a control-socket request at
//! all.

pub mod account_choice;
pub mod account_sharing;
pub mod account_switch;
pub mod accounts;
pub mod archive_cleanup;
pub mod bindings;
pub mod consoles;
pub mod exit_watch;
pub mod launch;
pub mod lifecycle;
mod messages;
pub mod model_change;
pub mod open;
pub mod pages;
pub mod projects;
pub mod restart;
pub mod resume;
pub mod settings;
#[cfg(test)]
mod test_support;
pub mod validate;

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::accounts::quotas;
use crate::agents::launch_settings;
use crate::config_document::export;
use crate::config_document::import;
use crate::env_shell;
use crate::host_open;
use crate::hostfs;
use crate::project_git;
use crate::protocol::{Agent, CodedError, Event, ModelChangeOutcome, Origin, RequestBody};
use crate::records;
use crate::records::history;
use crate::session::trust;
use crate::state::{broadcast_settings, AppState};

use account_choice::{agent_is_available, checked_account_reference, refuse_if_agent_unavailable};
use account_switch::switch_session_account;
use accounts::{
    add_agent, create_account, delete_account_requested, refresh_account_quotas, remove_agent,
    reorder_agents, set_account_disabled, set_default_account_name, start_account_login,
    update_account_requested,
};
use bindings::release_requested;
use consoles::{
    check_new_console, check_new_console_launch, delete_console, store_new_console, update_console,
    ConsoleChanges, NewConsole,
};
use lifecycle::{
    archive_requested, delete_archived_requested, delete_requested, inspect_session_branches,
    inspect_session_records, rename_session, set_session_colour, set_session_locked,
    set_session_pinned,
};
use messages::{send_message, submit_page};
use model_change::{set_session_model, ModelChange};
use open::{open_session_detached, OpenRequest};
use projects::{
    add_project, delete_project, detect_project_agent, inspect_project_removal, refresh_git_status,
    update_project, AddProjectRequest, NewProjectSettings, ProjectChanges,
};
use resume::{locate_conversation, resume_session};
use settings::{update_settings, SettingsChanges, CLAUDE_MODE, CODEX_REVIEWER};
use validate::{absolute_path, required_text};

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

/// As `handle` (the tests' entry point) does, but for a caller that sends the reply itself: what a
/// request still has to do after its reply has been queued comes back as a [`Followup`], to drop
/// once the reply is on its way.
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
            accounts,
            launch,
            project_session_launch,
            icon,
            console_session_prompt,
        } => {
            let new = NewConsole {
                name,
                default_agent,
                project_session_agent,
                accounts,
                launch,
                project_session_launch,
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
            accounts,
            launch,
            project_session_launch,
            icon,
            console_session_prompt,
        } => {
            update_console(
                state,
                &id,
                ConsoleChanges {
                    name,
                    default_agent,
                    project_session_agent,
                    accounts: accounts
                        .into_iter()
                        .filter_map(|(agent, account)| Some((agent, account.into_present()?)))
                        .collect(),
                    launch: launch.into(),
                    project_launch: project_session_launch.into(),
                    icon,
                    console_session_prompt,
                },
            )
            .await?;
            Ok(None)
        }

        RequestBody::DeleteConsole { console: id } => {
            delete_console(state, id).await?;
            Ok(None)
        }

        RequestBody::AddProject {
            console_id,
            source,
            path,
            remote_url,
            name,
            default_agent,
            accounts,
            launch,
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
                        accounts: accounts.map(|_, account| account.unwrap_or_default()),
                        launch,
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
            accounts,
            launch,
            pinned,
            tags,
            description,
            git_workflow,
        } => {
            update_project(
                state,
                &id,
                ProjectChanges {
                    name,
                    default_agent,
                    accounts: accounts
                        .into_iter()
                        .filter_map(|(agent, account)| {
                            Some((agent, account.into_present()?.unwrap_or_default()))
                        })
                        .collect(),
                    launch: launch.into(),
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
            delete_project(
                state,
                id,
                stop_sessions,
                trash_directory,
                force_remove,
                inspection,
            )
            .await?;
            Ok(None)
        }

        RequestBody::InspectProjectRemoval { project } => {
            inspect_project_removal(state, request_id, project).await
        }

        RequestBody::ListDir { path } => {
            let expanded = hostfs::expand(&path);
            let entries = hostfs::listing::list_dir(&expanded)?;
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
                project_git::clone::probe_remote_agent(&remote_url, &shell_env)
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
            refresh_account_quotas(state, agent, account)?;
            Ok(None)
        }

        RequestBody::StartAccountLogin { agent, account } => {
            start_account_login(state, agent, account).await?;
            Ok(None)
        }

        RequestBody::UseCodexResetCredit { account } => {
            refuse_if_agent_unavailable(state, Agent::Codex)?;
            let account = checked_account_reference(state, Agent::Codex, account)?;
            let outcome = quotas::use_reset_credit(
                state,
                account,
                reset_credit_shell(),
                quotas::RESET_CREDIT_TIMEOUT,
            )
            .await?;
            Ok(Some(Event::ResetCreditUsed {
                id: request_id,
                outcome,
            }))
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
            let selection = export::Selection::from_request(
                console,
                consoles,
                include_settings,
                include_accounts,
                include_paths,
            )?;
            Ok(Some(Event::ConfigurationExported {
                id: request_id,
                document: export::build(&state.store, &selection)?,
            }))
        }

        RequestBody::InspectConfiguration { document } => {
            let inspection = import::inspect(state, &document).await?;
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
            let imported = import::apply(
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
            archive_requested(state, session, force_remove, keep, keep_process_ids).await?;
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
                inspect_session_records(&state, &sessions, records::Locator::default())
            })
            .await??;
            Ok(Some(Event::SessionRecordsInspected {
                id: request_id,
                presence,
            }))
        }

        RequestBody::ReadSessionHistory { session } => {
            let record = state.session_record(&session)?;
            let messages = history::of_session_blocking(state, record).await?;
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
        } => inspect_session_branches(state, request_id, sessions, processes).await,

        RequestBody::ListProcesses { target } => {
            let trees = crate::processes::requests::list(state, target).await?;
            Ok(Some(Event::ProcessesListed {
                id: request_id,
                trees,
            }))
        }

        RequestBody::StopProcesses { ids } => {
            let ids = crate::processes::requests::stop(state, ids).await?;
            Ok(Some(Event::ProcessesStopped {
                id: request_id,
                ids,
            }))
        }

        RequestBody::KeepProcesses { ids } => {
            let ids = crate::processes::requests::keep(state, ids).await?;
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
            delete_requested(
                state,
                session,
                trash_agent_records,
                only_if_unused,
                force_remove,
                keep,
            )
            .await?;
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
            delete_archived_requested(
                state,
                console,
                project,
                console_session,
                trash_agent_records,
                force_remove,
                keep,
            )
            .await?;
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
            let crate::session::interrupt::Interrupted {
                interrupted,
                status,
                note,
            } = crate::session::interrupt::interrupt_session(state, &session).await?;
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
            release_requested(state, id).await?;
            Ok(None)
        }

        RequestBody::ListPages { session } => {
            // A project session can have pages too; only a session that does not exist is unknown.
            state.session_record(&session)?;
            let pages = state.store.list_pages(&session)?;
            Ok(Some(Event::PageList {
                id: request_id,
                session_id: session,
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
            project_git::status::pull_project_now(state, project);
            Ok(None)
        }

        RequestBody::CreateShellTerminal {
            project,
            worktree,
            path,
        } => {
            let shell_terminal = crate::terminal::shell::create(
                state,
                project,
                worktree,
                path,
                crate::git::isolated::IsolatedGit::from_shell,
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
            crate::terminal::shell::rename(
                state,
                &shell_terminal,
                required_text("title", &title)?,
            )?;
            Ok(None)
        }

        RequestBody::CloseShellTerminal { shell_terminal } => {
            crate::terminal::shell::close(state, &shell_terminal).await?;
            Ok(None)
        }

        RequestBody::OpenProjectTerminal { project } => {
            let project = state
                .store
                .get_project(&project)?
                .ok_or_else(|| CodedError::unknown_project(&project))?;
            let path = project.path;
            tokio::task::spawn_blocking(move || {
                host_open::terminal::open_directory(Path::new(&path))
            })
            .await??;
            Ok(None)
        }

        RequestBody::OpenProjectDirectory { project } => {
            let project = state
                .store
                .get_project(&project)?
                .ok_or_else(|| CodedError::unknown_project(&project))?;
            let path = project.path;
            tokio::task::spawn_blocking(move || host_open::paths::open(Path::new(&path))).await??;
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
                    &crate::git::isolated::IsolatedGit::from_shell,
                )?;
                host_open::paths::open_path(&path, reveal)
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
            tokio::task::spawn_blocking(move || {
                project_git::push::push_current_branch(&state, &project)
            })
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
                project_git::switch::switch_branch(&state, &project, &branch)
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
            tokio::task::spawn_blocking(move || {
                project_git::restore::restore_project(&state, &project)
            })
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
            update_account_requested(state, account, name, config_dir).await?;
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
            account_sharing::set(state, account, share).await?;
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
            delete_account_requested(state, account, trash_directory).await?;
            Ok(None)
        }

        RequestBody::InspectAccountTrash { account } => Ok(Some(Event::AccountTrashInspected {
            id: request_id,
            outcome: crate::accounts::trash::inspect(state, &account).await?,
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

/// The shell environment a use of a reset credit runs Codex in. A test has none: a request that
/// got through to it would spend a credit of the machine's own Codex account.
#[cfg(not(test))]
fn reset_credit_shell() -> quotas::ShellSource {
    Arc::new(env_shell::launch_snapshot)
}

#[cfg(test)]
fn reset_credit_shell() -> quotas::ShellSource {
    Arc::new(|| anyhow::bail!("no shell environment under test"))
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

#[cfg(test)]
mod tests;
