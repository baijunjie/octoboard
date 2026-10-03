//! Codex adapter. Everything is injected through repeated `-c` overrides, which are
//! per-invocation and persisted nowhere — so the complete set has to be re-passed on every
//! `codex resume` too.
//!
//! `CODEX_HOME` must not be used as a config-directory override: it makes Codex ignore the user's
//! entire global setup (`~/.codex/AGENTS.md`, model choice, approval policy, skills, plugins), not
//! just add to it, which is the same objection as modifying their configuration.
//!
//! Two conditions are launch prerequisites rather than polish:
//!
//! - **Project trust.** Without it the folder-trust modal appears. The table must be passed whole
//!   (`-c 'projects={"<path>"={trust_level="trusted"}}'`) because `-c` splits its key on `.` and
//!   silently ignores a dotted path containing a quoted segment, and the path has to be the
//!   canonical one — `/private/tmp/…`, not `/tmp/…`.
//! - **Hook trust.** Codex gates any new or changed hook behind a persisted trust hash. Without it
//!   an interactive session raises a blocking review modal and no hook runs, and a headless one
//!   hangs indefinitely with no output at all. `--dangerously-bypass-hook-trust` is the verified
//!   way through; despite the name it weakens neither the sandbox nor the approval policy, it only
//!   skips the review of hooks Octoboard generated itself.
//!   TODO(milestone 04): its cost is two warning lines per launch. The warning-free alternative is
//!   seeding `hooks.state` with the trust hashes, which needs them captured once per Codex version
//!   and shipped with the adapter; the hash preimage could not be derived, so they have to be read
//!   out of a trusted session.
//!
//! Codex cannot pre-allocate a session id, and in the interactive TUI the thread is created lazily
//! on the first prompt submission — so a session the user opens without a task has no
//! `agent_session_id` until they type something. The id then arrives on the `SessionStart` hook
//! payload, and thread id and session id are the same value.

use serde_json::json;

use super::{AgentAdapter, LaunchPlan, LaunchSpec, HOOK_TIMEOUT_SECS};

/// The events Octoboard's session states are derived from. `Interrupt` is included because it is
/// mutually exclusive with `Stop` and is the only signal of a cancelled turn any of the three
/// agents gives. `SubagentStart` / `SubagentStop` are deliberately not injected: they are noise
/// here, and their payloads' ids cannot be joined back to the parent turn.
const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Interrupt",
    "Stop",
];

// TODO(milestone 02): read the user's `approvals_reviewer` setting before promising a raised hand.
// With `auto_review` on, Codex resolves an approval request itself: the `PermissionRequest` hook
// fires, no modal is ever shown, and the tool proceeds — so Octoboard would raise a hand nobody
// needs to answer. Belongs with the raised hand itself, which is milestone 02's.
pub struct CodexAdapter;

impl AgentAdapter for CodexAdapter {
    fn binary(&self) -> &'static str {
        "codex"
    }

    fn preallocates_session_id(&self) -> bool {
        false
    }

    fn plan(&self, spec: &LaunchSpec<'_>) -> anyhow::Result<LaunchPlan> {
        let mut args = Vec::new();

        // `resume` is a subcommand, so it comes before the overrides.
        if let Some(id) = spec.resume_agent_session_id {
            args.push("resume".to_string());
            args.push(id.to_string());
        }

        let canonical_cwd = std::fs::canonicalize(spec.cwd)
            .unwrap_or_else(|_| spec.cwd.to_path_buf())
            .to_string_lossy()
            .into_owned();
        args.push("-c".to_string());
        args.push(format!(
            "projects={{{}={{trust_level=\"trusted\"}}}}",
            toml_string(&canonical_cwd)
        ));

        let command = spec.hook_script.to_string_lossy().into_owned();
        for event in HOOK_EVENTS {
            args.push("-c".to_string());
            args.push(format!(
                "hooks.{event}=[{{hooks=[{{type=\"command\",command={},timeout={},async=true}}]}}]",
                toml_string(&command),
                HOOK_TIMEOUT_SECS
            ));
        }
        args.push("--dangerously-bypass-hook-trust".to_string());

        if spec.resume_agent_session_id.is_none() {
            if let Some(task) = spec.task {
                args.push(task.to_string());
            }
        }

        Ok(LaunchPlan {
            args,
            env: Vec::new(),
        })
    }
}

/// A TOML basic string. JSON string escaping is a subset of TOML's, so serde_json produces a valid
/// one and spares us a second escaping implementation.
fn toml_string(value: &str) -> String {
    json!(value).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::tests::spec_fixture;

    fn overrides(args: &[String]) -> Vec<String> {
        args.windows(2)
            .filter(|pair| pair[0] == "-c")
            .map(|pair| pair[1].clone())
            .collect()
    }

    #[test]
    fn passes_the_project_trust_table_whole_and_canonicalised() {
        let fixture = spec_fixture();
        let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
        let canonical = std::fs::canonicalize(&fixture.cwd).expect("canonical path");
        let expected = format!(
            "projects={{{:?}={{trust_level=\"trusted\"}}}}",
            canonical.to_string_lossy()
        );
        // The whole table in one override: `-c` splits its key on `.`, so a dotted path with a
        // quoted segment is accepted and then silently ignored.
        assert!(
            overrides(&plan.args).contains(&expected),
            "expected {expected} among {:?}",
            overrides(&plan.args)
        );
    }

    #[test]
    fn injects_one_hook_override_per_event_and_gets_past_the_trust_gate() {
        let fixture = spec_fixture();
        let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
        let overrides = overrides(&plan.args);
        for event in HOOK_EVENTS {
            assert_eq!(
                overrides
                    .iter()
                    .filter(|override_| override_.starts_with(&format!("hooks.{event}=")))
                    .count(),
                1,
                "exactly one override for {event}"
            );
        }
        for override_ in overrides.iter().filter(|o| o.starts_with("hooks.")) {
            assert!(override_.contains("async=true"));
            assert!(override_.contains(&format!("timeout={HOOK_TIMEOUT_SECS}")));
        }
        // Without clearing the hook trust gate an interactive session raises a blocking review
        // modal and a headless one hangs indefinitely.
        assert!(plan
            .args
            .iter()
            .any(|arg| arg == "--dangerously-bypass-hook-trust"));
    }

    #[test]
    fn a_resume_names_the_subcommand_first_and_still_re_injects() {
        let fixture = spec_fixture();
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some("thread-id");
        let plan = CodexAdapter.plan(&spec).expect("plan");
        assert_eq!(plan.args[0], "resume");
        assert_eq!(plan.args[1], "thread-id");
        // Every `-c` override is per-invocation and persisted nowhere, so a resume that omits them
        // loses the hooks.
        assert!(!overrides(&plan.args).is_empty());
    }

    #[test]
    fn never_pre_allocates_a_session_id() {
        let fixture = spec_fixture();
        let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
        assert!(!CodexAdapter.preallocates_session_id());
        assert!(!plan.args.iter().any(|arg| arg.contains("session-id")));
    }
}
