//! `add_project`: registering a project from a directory or a clone.

use std::sync::Arc;

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use crate::coordinator;
use crate::mcp::args::{
    optional_agent, optional_bool, optional_string, optional_string_list, present_text,
    required_str,
};
use crate::mcp::settings::{
    describe_project, git_workflow_argument, launch_argument, project_accounts_argument,
};
use crate::project_git::remote_url;
use crate::protocol::{error_code, CodedError, ProjectSource, Session};
use crate::state::AppState;

pub(super) async fn add_project(
    state: &Arc<AppState>,
    console_session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let source = match required_str(arguments, "source")? {
        "local" => ProjectSource::Local,
        "parent" => ProjectSource::Parent,
        "git" => ProjectSource::Git,
        other => bail!("`{other}` is not one of `local`, `parent` or `git`"),
    };
    let remote_url = optional_string(arguments, "remote_url");
    if source == ProjectSource::Git {
        check_clone_url(remote_url.as_deref())?;
    }
    let mut settings = coordinator::projects::NewProjectSettings {
        pinned: optional_bool(arguments, "pinned")?.unwrap_or(false),
        git_workflow: git_workflow_argument(arguments)?,
        ..Default::default()
    };
    for (agent, account) in project_accounts_argument(state, arguments)? {
        *settings.accounts.get_mut(agent) = account;
    }
    for (agent, change) in launch_argument(arguments, "launch")? {
        let launch = settings.launch.get_mut(agent);
        launch.model = change.model.flatten();
        launch.effort = change.effort.flatten();
    }
    let added = coordinator::projects::add_project(
        state,
        coordinator::projects::AddProjectRequest {
            console_id: console_session.console_id.clone(),
            source,
            path: optional_string(arguments, "path"),
            remote_url,
            name: optional_string(arguments, "name"),
            default_agent: optional_agent(arguments, "default_agent")?,
            settings,
            detect_default_agent: true,
            check_launch: true,
            check_text_limits: true,
            tags: optional_string_list(arguments, "tags")?,
            description: present_text(arguments, "description")?.flatten(),
        },
    )
    .await?;
    let projects = added
        .iter()
        .map(|project| describe_project(state, project))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({ "projects": projects }))
}

/// Refuses a repository URL the console session may not clone from: one of a shape the import of a
/// configuration document refuses too, so a tool call is no way to reach a path of this machine or
/// run a helper transport, and one with a credential in it, which would be stored with the project
/// and shown in every listing. The user's own git credentials are what a clone uses.
fn check_clone_url(url: Option<&str>) -> Result<()> {
    let Some(url) = url else { return Ok(()) };
    let shown = remote_url::without_credentials(url);
    if remote_url::has_credentials(url) {
        return Err(CodedError::raised(
            error_code::INVALID_REMOTE_URL,
            format!(
                "`{shown}` carries credentials, which are not accepted in a URL: give it without, \
                 and the user's own git credentials are used"
            ),
            &[("url", &shown)],
        ));
    }
    if !remote_url::is_clonable_url(url) {
        return Err(CodedError::raised(
            error_code::INVALID_REMOTE_URL,
            format!("`{shown}` is not an http(s), ssh or git repository URL that can be cloned"),
            &[("url", &shown)],
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::mcp::exec::call;
    use crate::mcp::test_support::{arguments, refused, settings_state};
    use serde_json::json;

    /// A clone URL with a credential in it is refused without echoing the credential, so is one
    /// that is not a remote at all; a local association takes its settings and returns them.
    #[tokio::test]
    async fn add_project_refuses_a_credentialed_url_and_takes_its_settings() {
        let (state, dir) = settings_state("add-project");
        // Should the refusal ever fail, a clone lands in the scratch directory, not the user's.
        let clones = dir.join("clones");
        state
            .store
            .set_default_clone_dir(Some(&clones.to_string_lossy()))
            .unwrap();
        for (url, expected) in [
            (
                "https://user:token@example.com/r.git",
                "carries credentials",
            ),
            ("https://token@example.com/r.git", "carries credentials"),
            ("/etc/passwd", "can be cloned"),
        ] {
            let args = json!({ "source": "git", "remote_url": url });
            let message = refused(&state, "a", "add_project", args).await;
            assert!(message.contains(expected), "{message}");
            assert!(
                !message.contains("token@") && !message.contains("user:"),
                "{message}"
            );
        }
        assert_eq!(state.store.list_projects().unwrap().len(), 2);

        let message = refused(
            &state,
            "a",
            "add_project",
            json!({ "source": "local", "path": "/tmp", "description": 3 }),
        )
        .await;
        assert!(message.contains("description"), "{message}");
        assert_eq!(state.store.list_projects().unwrap().len(), 2);

        let repo = dir.join("repo");
        std::fs::create_dir(&repo).unwrap();
        let added = call(
            &state,
            "a",
            "add_project",
            &arguments(json!({
                "source": "local",
                "path": repo.to_string_lossy(),
                "tags": ["one"],
                "description": "  A billing service.  ",
                "pinned": true,
                "accounts": { "codex": "acct-codex" },
                "launch": { "claude": { "effort": "low" } },
            })),
        )
        .await
        .unwrap();
        let project = &added["projects"][0];
        assert_eq!(project["pinned"], true);
        assert_eq!(project["tags"], json!(["one"]));
        assert_eq!(project["description"], "A billing service.");
        assert_eq!(project["accounts"]["codex"], "acct-codex");
        assert_eq!(project["launch"]["claude"]["effort"], "low");

        let bare = dir.join("bare");
        std::fs::create_dir(&bare).unwrap();
        let added = call(
            &state,
            "a",
            "add_project",
            &arguments(json!({
                "source": "local",
                "path": bare.to_string_lossy(),
                "description": null,
            })),
        )
        .await
        .unwrap();
        assert!(added["projects"][0]["description"].is_null());
    }

    /// `add_project` takes the project's own git workflow values as `update_project` does, except
    /// that an item left out or `null` follows the application-wide value; a parent source gives
    /// them to every project it adds, and a bad value is refused before anything is stored.
    #[tokio::test]
    async fn add_project_takes_the_projects_own_git_workflow_values() {
        let (state, dir) = settings_state("add-project-git-workflow");
        let before = state.store.list_projects().unwrap().len();
        for bad in [
            json!({ "delivery": "rebase" }),
            json!({ "development": "yes" }),
            json!({ "typo": true }),
        ] {
            let args = json!({ "source": "local", "path": "/tmp", "git_workflow": bad });
            let message = refused(&state, "a", "add_project", args).await;
            assert!(message.contains("git_workflow"), "{message}");
        }
        assert_eq!(state.store.list_projects().unwrap().len(), before);

        call(
            &state,
            "a",
            "update_settings",
            &arguments(json!({ "git_workflow": { "revert_gate": true } })),
        )
        .await
        .unwrap();
        let repo = dir.join("repo");
        std::fs::create_dir(&repo).unwrap();
        let added = call(
            &state,
            "a",
            "add_project",
            &arguments(json!({
                "source": "local",
                "path": repo.to_string_lossy(),
                "git_workflow": { "development": "in_place", "delivery": "merge", "revert_gate": null },
            })),
        )
        .await
        .unwrap();
        assert_eq!(
            added["projects"][0]["git_workflow"]["override"],
            json!({
                "development": "in_place",
                "squash_before_delivery": null,
                "rebase_before_merge": null,
                "delivery": "merge",
                "revert_gate": null,
            })
        );
        assert_eq!(
            added["projects"][0]["git_workflow"]["effective"]["revert_gate"],
            true
        );

        let parent = dir.join("parent");
        for name in ["one", "two"] {
            std::fs::create_dir_all(parent.join(name).join(".git")).unwrap();
        }
        let added = call(
            &state,
            "a",
            "add_project",
            &arguments(json!({
                "source": "parent",
                "path": parent.to_string_lossy(),
                "git_workflow": { "rebase_before_merge": false },
            })),
        )
        .await
        .unwrap();
        let projects = added["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 2);
        assert!(projects
            .iter()
            .all(|project| project["git_workflow"]["override"]["rebase_before_merge"] == false));

        let bare = dir.join("bare");
        std::fs::create_dir(&bare).unwrap();
        let added = call(
            &state,
            "a",
            "add_project",
            &arguments(json!({ "source": "local", "path": bare.to_string_lossy() })),
        )
        .await
        .unwrap();
        assert!(added["projects"][0]["git_workflow"]["override"]["delivery"].is_null());
    }
}
