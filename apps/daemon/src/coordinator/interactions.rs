//! The user's answer to the prompt a session's agent shows while it waits on them
//! (`answer_interaction`), pressed into the agent's terminal by `crate::session::interaction`.

use std::sync::Arc;

use anyhow::Result;

use crate::session::interaction::{press, AnswerFields};
use crate::state::AppState;

/// Presses `fields` as the answer to the prompt published for session `id` as `interaction`. Off
/// the runtime: the press blocks while it watches the screen respond to each key.
pub(super) async fn answer_interaction(
    state: &Arc<AppState>,
    id: &str,
    interaction: String,
    fields: AnswerFields,
) -> Result<()> {
    // An unknown session is refused as such; one with no process has nothing waiting.
    state.session_record(id)?;
    let Some(live) = state.live_session(id) else {
        return Err(press::not_waiting(id));
    };
    let state = state.clone();
    tokio::task::spawn_blocking(move || press::answer(&state, &live, &interaction, fields)).await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{
        error_code, AgentId, ChoiceEffect, InteractionPrompt, PendingInteraction, PermissionChoice,
        PermissionPrompt, SessionStatus,
    };
    use crate::session::interaction::{CallSeen, Published};
    use crate::state::test_support::insert_record;
    use crate::test_support::{app_state, code_of, fake_live_session};

    fn fields(choice: &str) -> AnswerFields {
        AnswerFields {
            choice: Some(choice.to_string()),
            text: None,
            answers: None,
            dismiss: false,
        }
    }

    /// An answer for a session nobody has is refused as an unknown session.
    #[tokio::test]
    async fn an_answer_for_an_unknown_session_is_refused_as_unknown() {
        let (state, _dir) = app_state("interactions-unknown");
        let refused = answer_interaction(&state, "nobody", "i".to_string(), fields("1"))
            .await
            .unwrap_err();
        assert_eq!(code_of(&refused), error_code::UNKNOWN_SESSION);
    }

    /// A session with no process has no prompt waiting.
    #[tokio::test]
    async fn an_answer_for_a_session_with_no_process_is_not_waiting() {
        let (state, dir) = app_state("interactions-no-process");
        insert_record(&state, &dir, "s", SessionStatus::WaitingUser);
        let refused = answer_interaction(&state, "s", "i".to_string(), fields("1"))
            .await
            .unwrap_err();
        assert_eq!(code_of(&refused), error_code::INTERACTION_NOT_WAITING);
    }

    /// An answer to the published prompt is pressed off the runtime and, once the prompt is gone,
    /// acknowledged with the hand down; one naming another id is refused as stale.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_published_prompt_is_answered_and_a_stale_id_is_not() {
        let (state, dir) = app_state("interactions-answer");
        insert_record(&state, &dir, "s", SessionStatus::WaitingUser);
        let mut record = state.session_record("s").unwrap();
        record.agent = AgentId::TEST;
        state.save_session(&record).unwrap();
        let live = fake_live_session(
            "s",
            AgentId::TEST,
            80,
            24,
            "printf 'PROMPT Bash ls\\r\\n'; IFS= read -r -n 1 key; \
             printf '\\033[2J\\033[HINPUT> after %s\\r\\n' \"$key\"; sleep 30",
        );
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        let call = CallSeen::new(
            Some("call-1".to_string()),
            "Bash",
            &serde_json::json!({"command": "ls"}),
        );
        live.interaction.call_started(call.clone());
        let choice = |id: &str| PermissionChoice {
            id: id.to_string(),
            label: id.to_string(),
            effect: ChoiceEffect::Allow,
            text: None,
        };
        live.interaction.publish(Some(Published {
            interaction: PendingInteraction {
                id: "i1".to_string(),
                call_id: Some("call-1".to_string()),
                prompt: InteractionPrompt::Permission(PermissionPrompt {
                    tool: "Bash".to_string(),
                    category: None,
                    title: None,
                    detail: None,
                    input: serde_json::json!({"command": "ls"}),
                    input_truncated: false,
                    choices: vec![choice("1")],
                }),
            },
            call,
            writes: live.input_writes(),
        }));
        // The prompt is published from a screen that shows it.
        assert!(crate::session::wait::wait_for(
            crate::test_support::PATIENCE,
            || live.screen_text().contains("PROMPT Bash ls")
        ));
        let stale = answer_interaction(&state, "s", "i0".to_string(), fields("1"))
            .await
            .unwrap_err();
        assert_eq!(code_of(&stale), error_code::INTERACTION_NOT_WAITING);
        answer_interaction(&state, "s", "i1".to_string(), fields("1"))
            .await
            .expect("answered");
        assert!(live.screen_text().contains("INPUT> after 1"));
        assert!(live.interaction.interaction().is_none());
        assert_eq!(
            state.session_record("s").unwrap().status,
            SessionStatus::Working
        );
    }
}
