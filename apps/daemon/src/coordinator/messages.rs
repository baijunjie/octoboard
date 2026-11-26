//! The user's own writes of a message into a session.

use std::sync::Arc;

use anyhow::Result;

use crate::messaging::delivery;
use crate::protocol::{error_code, CodedError, ProviderOperation};
use crate::session::term::{self, Shape};
use crate::state::AppState;

/// The most images one message takes.
pub(super) const MAX_IMAGES: usize = 10;

/// The user's own write into a session. Refused rather than queued while the session is waiting for
/// them: they are the one who has to answer the prompt that is blocking it, and they can be told so.
/// Not held back by a hold for the account's usage limit, which keeps only Octoboard's own writes.
/// Off the runtime, because the write blocks on the PTY and on the wait for the agent to send it.
///
/// With `images`, each the path of an image saved for this session (`save_pasted_image`), the text
/// may be empty; a provider that declares no `message_images` refuses them.
pub(super) async fn send_message(
    state: &Arc<AppState>,
    id: &str,
    text: String,
    images: Vec<String>,
) -> Result<()> {
    if images.is_empty() {
        delivery::write_users_message_off_runtime(state, id, text).await?;
        return Ok(());
    }
    let agent = state.session_record(id)?.agent;
    if !agent.descriptor().capabilities.message_images {
        return Err(CodedError::provider_unsupported(
            agent,
            ProviderOperation::MessageImages,
        ));
    }
    if images.len() > MAX_IMAGES {
        return Err(CodedError::raised(
            error_code::UNREADABLE_REQUEST,
            format!("a message takes at most {MAX_IMAGES} images"),
            &[("field", "images")],
        ));
    }
    let checked = images.clone();
    let checking = state.clone();
    let session = id.to_string();
    tokio::task::spawn_blocking(move || {
        crate::session::pasted_image::require_saved_for(&checking, &session, &checked)
    })
    .await??;
    delivery::write_users_shaped_off_runtime(state, id, text, Shape::Images(images)).await?;
    Ok(())
}

/// The user's slash command, written for the session's agent to run as one of its own: one line
/// beginning with `/`, for a provider that declares `slash_commands`, written without the guard a
/// message's leading slash gets and always through the terminal. Refused as a message is, with
/// `command_not_recognised` where the agent would run another command in its place, and with
/// `command_not_found` where its list says it has no such command.
pub(super) async fn send_command(state: &Arc<AppState>, id: &str, command: String) -> Result<()> {
    let command = checked_command(state, id, &command)?;
    delivery::write_users_shaped_off_runtime(state, id, command, Shape::Command).await?;
    Ok(())
}

/// `command` as [`send_command`] writes it into the session `id`, trimmed, once it is checked to
/// be one the session's agent can be given: refused as `send_command` refuses it before writing.
pub(super) fn checked_command(state: &Arc<AppState>, id: &str, command: &str) -> Result<String> {
    let agent = state.session_record(id)?.agent;
    if !agent.descriptor().capabilities.slash_commands {
        return Err(CodedError::provider_unsupported(
            agent,
            ProviderOperation::SlashCommands,
        ));
    }
    let command = term::stripped(command).trim().to_string();
    let named = command
        .strip_prefix('/')
        .is_some_and(|rest| !rest.is_empty() && !rest.starts_with(char::is_whitespace));
    if !named || command.contains('\n') {
        return Err(CodedError::raised(
            error_code::UNREADABLE_REQUEST,
            "a command is one line beginning with `/` and the command's name",
            &[("field", "command")],
        ));
    }
    // A completion offered for a lone `/word` that is not the word written is cleared from the
    // input box after it is seen. A provider with no measured way to clear the box (Ctrl-U, or a
    // key that empties it at the idle prompt) cannot take that back, so a command that may bring a
    // completion up is not written to it at all. That holds for a lone command that gets a space
    // typed after it too (`ScreenReader::writes_lone_command_with_trailing_space`): it is pasted
    // bare and its completion looked at first.
    let bare = !command.contains(char::is_whitespace);
    if bare && !crate::session::screen::clears_input_box(crate::adapter::provider(agent).screen()) {
        return Err(CodedError::provider_unsupported(
            agent,
            ProviderOperation::SlashCommands,
        ));
    }
    Ok(command)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::protocol::{AgentId, SessionStatus};
    use crate::state::test_support::insert_record;
    use crate::test_support::composer_agent::{agent, Agent};
    use crate::test_support::{app_state, code_of, param_of};

    /// A session `s` of the test provider, at `status`, with an agent of its own drawing its input
    /// box, completing a slash command as `complete` says.
    fn session(
        label: &str,
        status: SessionStatus,
        complete: &str,
    ) -> (Arc<AppState>, Agent, crate::test_support::ScratchDir) {
        let (state, dir) = app_state(label);
        insert_record(&state, &dir, "s", status);
        let mut record = state.session_record("s").unwrap();
        record.agent = AgentId::TEST;
        state.save_session(&record).unwrap();
        let agent = agent("s", "yes", "0", complete);
        state.register_live(agent.live.clone());
        (state, agent, dir)
    }

    /// A command is one line that begins with a slash and names a command.
    #[tokio::test]
    async fn a_command_that_is_not_one_line_beginning_with_a_slash_is_unreadable() {
        let (state, agent, _dir) = session("messages-command-shape", SessionStatus::Idle, "");
        for command in ["status", "/", "  ", "/a\n/b", "\u{200B}status", "/ status"] {
            let refused = send_command(&state, "s", command.to_string())
                .await
                .unwrap_err();
            assert_eq!(
                code_of(&refused),
                error_code::UNREADABLE_REQUEST,
                "{command:?}"
            );
            assert_eq!(param_of(&refused, "field"), "command");
        }
        assert!(agent.logged().is_empty());
    }

    /// Both are the user's own writes: refused while the session waits for them, with nothing
    /// written.
    #[tokio::test]
    async fn a_command_and_images_are_refused_while_the_session_waits_for_the_user() {
        let (state, agent, _dir) = session("messages-waiting", SessionStatus::WaitingUser, "");
        let refused = send_command(&state, "s", "/status".to_string())
            .await
            .unwrap_err();
        assert_eq!(code_of(&refused), error_code::SESSION_WAITING_FOR_USER);
        let image = saved_image(&state);
        let refused = send_message(&state, "s", String::new(), vec![image])
            .await
            .unwrap_err();
        assert_eq!(code_of(&refused), error_code::SESSION_WAITING_FOR_USER);
        assert!(agent.logged().is_empty());
    }

    /// An image saved for session `s`, as `save_pasted_image` leaves it.
    fn saved_image(state: &Arc<AppState>) -> String {
        crate::session::pasted_image::save_bytes(state, "s", b"\x89PNG\r\n\x1a\nrest of the image")
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    /// Images are a message without text, or with it: each saved image is pasted alone, then the
    /// text if there is any, and the message is sent.
    #[tokio::test]
    async fn a_message_of_saved_images_is_sent_with_or_without_text() {
        let (state, agent, _dir) = session("messages-images", SessionStatus::Idle, "");
        let image = saved_image(&state);
        send_message(&state, "s", String::new(), vec![image.clone()])
            .await
            .unwrap();
        assert_eq!(
            agent.logged(),
            [format!("PASTE {image}"), "SENT [Image #1]".to_string()]
        );
        send_message(&state, "s", "look".to_string(), vec![image.clone()])
            .await
            .unwrap();
        let logged = agent.logged();
        assert_eq!(
            logged[2..4],
            [format!("PASTE {image}"), "PASTE look".to_string()]
        );
        // The stand-in numbers its placeholders from the start of its run, so only the shape counts.
        assert!(
            logged[4].starts_with("SENT [Image #") && logged[4].ends_with("]look"),
            "{logged:?}"
        );
        assert_eq!(logged.len(), 5);
    }

    /// A message with images whose files are gone by the time it is written is refused with
    /// `message_not_written`, and nothing reaches the agent.
    #[tokio::test]
    async fn a_message_whose_images_are_gone_when_written_is_refused() {
        let (state, agent, _dir) = session("messages-images-gone", SessionStatus::Idle, "");
        let refused = delivery::write_users_shaped_off_runtime(
            &state,
            "s",
            "look".to_string(),
            Shape::Images(vec!["/no/such/directory/paste-1.png".to_string()]),
        )
        .await
        .unwrap_err();
        assert_eq!(code_of(&refused), error_code::MESSAGE_NOT_WRITTEN);
        assert!(agent.logged().is_empty());
        assert_eq!(state.outbox.len("s"), 0);
    }

    /// An image path that is not one saved for the session is unreadable, however it is spelt.
    #[tokio::test]
    async fn an_image_that_was_not_saved_for_the_session_is_unreadable() {
        let (state, _agent, _dir) = session("messages-foreign-image", SessionStatus::Idle, "");
        for path in ["/etc/passwd", "relative.png", "/tmp/pasted/../paste-1.png"] {
            let refused = send_message(&state, "s", String::new(), vec![path.to_string()])
                .await
                .unwrap_err();
            assert_eq!(code_of(&refused), error_code::UNREADABLE_REQUEST, "{path}");
            assert_eq!(param_of(&refused, "field"), "images");
        }
    }

    /// A command goes through the queue into the agent's terminal and is run by its Enter.
    #[tokio::test]
    async fn a_command_is_written_into_the_agent_and_run() {
        let (state, agent, _dir) = session("messages-command-run", SessionStatus::Idle, "");
        send_command(&state, "s", "/status".to_string())
            .await
            .unwrap();
        assert_eq!(agent.logged(), ["PASTE /status", "SENT /status"]);
    }

    /// A command the agent would run as another is refused to the sender with the command it
    /// offered, slash included, and its input box is left empty.
    #[tokio::test]
    async fn a_command_the_agent_would_run_as_another_is_refused_with_the_one_offered() {
        let (state, agent, _dir) =
            session("messages-command-other", SessionStatus::Idle, "probecmd");
        let mut events = state.subscribe();
        let refused = send_command(&state, "s", "/probe".to_string())
            .await
            .unwrap_err();
        // The session is told as well, whether or not a sender waits.
        let mut notice = None;
        while let Ok(event) = events.try_recv() {
            if let crate::protocol::Event::SessionNotice { code, params, .. } = event {
                if code == crate::protocol::notice_code::COMMAND_NOT_RECOGNISED {
                    notice = Some(params);
                }
            }
        }
        let params = notice.expect("a command_not_recognised notice");
        assert_eq!(params["offered"], "/probecmd");
        assert_eq!(code_of(&refused), error_code::COMMAND_NOT_RECOGNISED);
        assert_eq!(param_of(&refused, "command"), "/probe");
        assert_eq!(param_of(&refused, "offered"), "/probecmd");
        assert_eq!(agent.logged(), ["PASTE /probe", "CLEARED"]);
        assert_eq!(state.outbox.len("s"), 0);
    }

    /// A message takes at most ten images; more is unreadable, whatever they are.
    #[tokio::test]
    async fn more_than_ten_images_are_unreadable() {
        let (state, _agent, _dir) = session("messages-many-images", SessionStatus::Idle, "");
        let refused = send_message(&state, "s", String::new(), vec!["/a.png".to_string(); 11])
            .await
            .unwrap_err();
        assert_eq!(code_of(&refused), error_code::UNREADABLE_REQUEST);
        assert_eq!(param_of(&refused, "field"), "images");
    }
}
