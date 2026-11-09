//! The tool calls of a session's current turn as its hooks report them: the open ones, for telling
//! which call a permission prompt belongs to, and those waiting on the user, kept so that the end
//! of another call of the same turn does not lower the hand raised for them.

use std::hash::{Hash, Hasher};

use super::AppState;
use crate::hooks::{CallShape, ToolCallChange};

/// A session's tool calls, as far as its hooks have reported them.
#[derive(Default)]
pub(super) struct ToolCalls {
    /// The calls started and not ended yet, oldest first, for telling which one a permission
    /// prompt, which names no call, belongs to.
    open: Vec<OpenCall>,
    /// The calls waiting on the user, each by its id, or `None` for one whose id is not known.
    waiting: Vec<Option<String>>,
}

struct OpenCall {
    id: String,
    tool: Option<String>,
    /// A digest of the call's input, not the input itself, which can be a whole file's content.
    input: Option<u64>,
}

/// A digest of `input` that ignores the order of object keys, which Claude Code does not keep
/// from one event to the next for the same call's input.
fn digest(input: Option<&serde_json::Value>) -> Option<u64> {
    input.map(|input| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hash_value(input, &mut hasher);
        hasher.finish()
    })
}

fn hash_value(value: &serde_json::Value, hasher: &mut impl Hasher) {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_unstable_by_key(|(key, _)| key.as_str());
            hasher.write_u8(b'{');
            for (key, value) in entries {
                key.hash(hasher);
                hash_value(value, hasher);
            }
            hasher.write_u8(b'}');
        }
        Value::Array(items) => {
            hasher.write_u8(b'[');
            for item in items {
                hash_value(item, hasher);
            }
            hasher.write_u8(b']');
        }
        scalar => scalar.to_string().hash(hasher),
    }
}

impl ToolCalls {
    fn apply(&mut self, change: ToolCallChange<'_>) {
        match change {
            ToolCallChange::Started {
                id,
                call,
                waits_on_user,
            } => {
                if let Some(id) = id {
                    self.open.push(OpenCall {
                        id: id.to_string(),
                        tool: call.tool.map(str::to_string),
                        input: digest(call.input),
                    });
                }
                if waits_on_user {
                    self.wait(id.map(str::to_string));
                }
            }
            ToolCallChange::WaitsOnUser(call) => {
                let id = self.open_call(&call);
                self.wait(id);
            }
            ToolCallChange::Ended { id: Some(id) } => {
                self.open.retain(|call| call.id != id);
                let waited = self.waiting.len();
                self.waiting
                    .retain(|waiting| waiting.as_deref() != Some(id));
                // A call waiting under no known id may be this one, and is taken to be.
                if self.waiting.len() == waited {
                    self.waiting.retain(Option::is_some);
                }
            }
            // An end that names no call could be any of them.
            ToolCallChange::Ended { id: None } => self.waiting.clear(),
            ToolCallChange::AllEnded => {
                self.open.clear();
                self.waiting.clear();
            }
        }
    }

    fn wait(&mut self, id: Option<String>) {
        if !self.waiting.contains(&id) {
            self.waiting.push(id);
        }
    }

    /// The id of the open call `call` is, the latest started of those with its tool and input,
    /// or else of those with its tool; among each, one not yet waiting on the user before one that
    /// already is.
    fn open_call(&self, call: &CallShape<'_>) -> Option<String> {
        let input = digest(call.input);
        let same_tool = || {
            self.open
                .iter()
                .rev()
                .filter(|open| open.tool.as_deref() == call.tool)
        };
        let is_waiting = |open: &&OpenCall| {
            self.waiting
                .iter()
                .any(|w| w.as_deref() == Some(open.id.as_str()))
        };
        let pick = |matches: &dyn Fn(&&OpenCall) -> bool| {
            same_tool()
                .filter(matches)
                .find(|open| !is_waiting(open))
                .or_else(|| same_tool().find(matches))
        };
        pick(&|open| open.input == input)
            .or_else(|| pick(&|_| true))
            .map(|open| open.id.clone())
    }
}

impl AppState {
    /// Applies what a hook event says about session `id`'s tool calls, and says whether any of
    /// them still waits on the user.
    pub fn apply_tool_call_change(&self, id: &str, change: ToolCallChange<'_>) -> bool {
        let mut calls = self.tool_calls.lock().expect("tool call lock poisoned");
        let session = calls.entry(id.to_string()).or_default();
        session.apply(change);
        let waiting = !session.waiting.is_empty();
        if !waiting && session.open.is_empty() {
            calls.remove(id);
        }
        waiting
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::app_state;
    use serde_json::json;

    fn started<'a>(id: &'a str, tool: &'a str, input: &'a serde_json::Value) -> ToolCallChange<'a> {
        ToolCallChange::Started {
            id: Some(id),
            call: CallShape {
                tool: Some(tool),
                input: Some(input),
            },
            waits_on_user: false,
        }
    }

    /// A permission prompt belongs to the open call with its tool and input, not to the call
    /// started last.
    #[test]
    fn a_permission_prompt_is_matched_to_its_call_by_tool_and_input() {
        let (first, second) = (json!({"command": "make"}), json!({"command": "ls"}));
        let mut calls = ToolCalls::default();
        calls.apply(started("a", "Bash", &first));
        calls.apply(started("b", "Bash", &second));
        calls.apply(ToolCallChange::WaitsOnUser(CallShape {
            tool: Some("Bash"),
            input: Some(&first),
        }));
        calls.apply(ToolCallChange::Ended { id: Some("b") });
        assert_eq!(calls.waiting, vec![Some("a".to_string())]);
        calls.apply(ToolCallChange::Ended { id: Some("a") });
        assert!(calls.waiting.is_empty());
    }

    /// A prompt for no call that was seen starting is lowered by the next call to end.
    #[test]
    fn a_prompt_for_an_unknown_call_ends_with_any_call() {
        let input = json!({});
        let mut calls = ToolCalls::default();
        calls.apply(ToolCallChange::WaitsOnUser(CallShape {
            tool: Some("Write"),
            input: Some(&input),
        }));
        assert_eq!(calls.waiting, vec![None]);
        calls.apply(ToolCallChange::Ended { id: Some("x") });
        assert!(calls.waiting.is_empty());
    }

    fn question<'a>(id: &'a str, input: &'a serde_json::Value) -> ToolCallChange<'a> {
        ToolCallChange::Started {
            id: Some(id),
            call: CallShape {
                tool: Some("AskUserQuestion"),
                input: Some(input),
            },
            waits_on_user: true,
        }
    }

    fn prompt<'a>(tool: &'a str, input: &'a serde_json::Value) -> ToolCallChange<'a> {
        ToolCallChange::WaitsOnUser(CallShape {
            tool: Some(tool),
            input: Some(input),
        })
    }

    fn ended(id: &str) -> ToolCallChange<'_> {
        ToolCallChange::Ended { id: Some(id) }
    }

    /// The hand raised for a question stays up through the end of every other call, and comes
    /// down only with the question's own end.
    #[test]
    fn a_question_waits_until_its_own_call_ends() {
        let (state, _dir) = app_state("tool-calls-question");
        let (input, agent) = (json!({"questions": []}), json!({"prompt": "go"}));
        assert!(!state.apply_tool_call_change("s", started("a", "Agent", &agent)));
        assert!(state.apply_tool_call_change("s", question("q", &input)));
        assert!(state.apply_tool_call_change("s", ended("a")));
        assert!(state.apply_tool_call_change("s", ended("never-started")));
        assert!(!state.apply_tool_call_change("s", ended("q")));
    }

    /// A call that waits on nobody raises no hand, however it starts and ends.
    #[test]
    fn a_call_that_does_not_wait_raises_no_hand() {
        let (state, _dir) = app_state("tool-calls-plain");
        let input = json!({"command": "ls"});
        assert!(!state.apply_tool_call_change("s", started("a", "Bash", &input)));
        assert!(!state.apply_tool_call_change("s", ended("a")));
    }

    /// A question that arrives with no id is ended by any call that ends.
    #[test]
    fn a_question_without_an_id_ends_with_any_call() {
        let (state, _dir) = app_state("tool-calls-question-no-id");
        let change = ToolCallChange::Started {
            id: None,
            call: CallShape {
                tool: Some("AskUserQuestion"),
                input: None,
            },
            waits_on_user: true,
        };
        assert!(state.apply_tool_call_change("s", change));
        assert!(!state.apply_tool_call_change("s", ended("x")));
    }

    /// A permission prompt keeps the hand up through the end of the other calls of its message.
    #[test]
    fn a_permission_prompt_waits_through_the_end_of_another_call() {
        let (state, _dir) = app_state("tool-calls-prompt");
        let (make, agent) = (json!({"command": "make"}), json!({"prompt": "go"}));
        state.apply_tool_call_change("s", started("a", "Bash", &make));
        state.apply_tool_call_change("s", started("b", "Agent", &agent));
        assert!(state.apply_tool_call_change("s", prompt("Bash", &make)));
        assert!(state.apply_tool_call_change("s", ended("b")));
        assert!(!state.apply_tool_call_change("s", ended("a")));
    }

    /// A prompt whose input matches no open call of its tool belongs to the latest call of that
    /// tool.
    #[test]
    fn a_prompt_with_an_unmatched_input_belongs_to_the_latest_call_of_its_tool() {
        let (state, _dir) = app_state("tool-calls-prompt-tool-only");
        let (first, second, shown) = (
            json!({"command": "make"}),
            json!({"command": "ls"}),
            json!({"command": "rm"}),
        );
        state.apply_tool_call_change("s", started("a", "Bash", &first));
        state.apply_tool_call_change("s", started("b", "Bash", &second));
        assert!(state.apply_tool_call_change("s", prompt("Bash", &shown)));
        assert!(state.apply_tool_call_change("s", ended("a")));
        assert!(!state.apply_tool_call_change("s", ended("b")));
    }

    /// Of two open calls with the same tool and input, the prompt belongs to the later one.
    #[test]
    fn a_prompt_belongs_to_the_latest_of_identical_calls() {
        let (state, _dir) = app_state("tool-calls-prompt-identical");
        let input = json!({"command": "make"});
        state.apply_tool_call_change("s", started("a", "Bash", &input));
        state.apply_tool_call_change("s", started("b", "Bash", &input));
        assert!(state.apply_tool_call_change("s", prompt("Bash", &input)));
        assert!(state.apply_tool_call_change("s", ended("a")));
        assert!(!state.apply_tool_call_change("s", ended("b")));
    }

    /// A prompt of a tool none of the open calls uses is for a call that was not seen starting:
    /// the next call to end lowers the hand.
    #[test]
    fn a_prompt_of_another_tool_than_any_open_call_ends_with_the_next_call() {
        let (state, _dir) = app_state("tool-calls-prompt-other-tool");
        let (ls, file) = (json!({"command": "ls"}), json!({"path": "/x"}));
        state.apply_tool_call_change("s", started("a", "Bash", &ls));
        assert!(state.apply_tool_call_change("s", prompt("Write", &file)));
        assert!(!state.apply_tool_call_change("s", ended("a")));
    }

    /// A question's own permission prompt, which follows its start, is the same wait: one end
    /// lowers it.
    #[test]
    fn a_prompt_for_a_waiting_question_does_not_wait_twice() {
        let (state, _dir) = app_state("tool-calls-question-prompt");
        let input = json!({"questions": []});
        state.apply_tool_call_change("s", question("q", &input));
        assert!(state.apply_tool_call_change("s", prompt("AskUserQuestion", &input)));
        assert!(!state.apply_tool_call_change("s", ended("q")));
    }

    /// Two calls waiting at once keep the hand up until both have ended.
    #[test]
    fn the_hand_stays_up_until_every_waiting_call_has_ended() {
        let (state, _dir) = app_state("tool-calls-two-waiting");
        let (questions, make) = (json!({"questions": []}), json!({"command": "make"}));
        state.apply_tool_call_change("s", started("a", "Bash", &make));
        state.apply_tool_call_change("s", question("q", &questions));
        assert!(state.apply_tool_call_change("s", prompt("Bash", &make)));
        assert!(state.apply_tool_call_change("s", ended("q")));
        assert!(!state.apply_tool_call_change("s", ended("a")));
    }

    /// An end that names no call, and the end of the batch or the turn, lower a hand whatever
    /// call it was raised for.
    #[test]
    fn an_end_of_all_calls_or_of_an_unnamed_call_lowers_the_hand() {
        let (state, _dir) = app_state("tool-calls-all-ended");
        let input = json!({"questions": []});
        for end in [ToolCallChange::AllEnded, ToolCallChange::Ended { id: None }] {
            assert!(state.apply_tool_call_change("s", question("q", &input)));
            assert!(!state.apply_tool_call_change("s", end));
        }
    }

    /// After the turn's end a call that had been open is not matched by a later prompt.
    #[test]
    fn calls_open_before_the_turns_end_are_forgotten_with_it() {
        let (state, _dir) = app_state("tool-calls-forgotten-open");
        let input = json!({"command": "make"});
        state.apply_tool_call_change("s", started("a", "Bash", &input));
        state.apply_tool_call_change("s", ToolCallChange::AllEnded);
        // The prompt matches no open call, so it is lowered by any end, a's included.
        assert!(state.apply_tool_call_change("s", prompt("Bash", &input)));
        assert!(!state.apply_tool_call_change("s", ended("other")));
    }

    /// One session's waiting call says nothing of another session's calls.
    #[test]
    fn sessions_keep_their_calls_apart() {
        let (state, _dir) = app_state("tool-calls-sessions");
        let input = json!({"questions": []});
        assert!(state.apply_tool_call_change("one", question("q", &input)));
        assert!(!state.apply_tool_call_change("two", ended("a")));
        assert!(state.apply_tool_call_change("one", ended("a")));
        assert!(!state.apply_tool_call_change("one", ended("q")));
    }

    /// Claude Code does not keep the order of an input's keys from one event to the next, so a
    /// prompt whose input has the same content in another key order still belongs to its call.
    #[test]
    fn a_prompt_is_matched_to_its_call_whatever_the_order_of_the_input_keys() {
        let (state, _dir) = app_state("tool-calls-prompt-key-order");
        let started_input = json!({
            "command": "make",
            "options": {"cwd": "/x", "env": {"A": "1", "B": "2"}},
            "args": [{"k": 1, "j": 2}, "z"],
        });
        let shown = json!({
            "args": [{"j": 2, "k": 1}, "z"],
            "options": {"env": {"B": "2", "A": "1"}, "cwd": "/x"},
            "command": "make",
        });
        let other = json!({"command": "ls"});
        state.apply_tool_call_change("s", started("a", "Bash", &started_input));
        state.apply_tool_call_change("s", started("b", "Bash", &other));
        assert!(state.apply_tool_call_change("s", prompt("Bash", &shown)));
        // The prompt is a's, not that of b, the latest call of the tool.
        assert!(state.apply_tool_call_change("s", ended("b")));
        assert!(!state.apply_tool_call_change("s", ended("a")));
    }

    /// The order of an array's items is part of its content: a prompt whose array is in another
    /// order is not for the call that has the first order.
    #[test]
    fn a_prompt_is_not_matched_to_a_call_whose_array_input_is_in_another_order() {
        let (state, _dir) = app_state("tool-calls-prompt-array-order");
        let (first, second) = (json!({"paths": ["a", "b"]}), json!({"paths": ["c"]}));
        let shown = json!({"paths": ["b", "a"]});
        state.apply_tool_call_change("s", started("a", "Bash", &first));
        state.apply_tool_call_change("s", started("b", "Bash", &second));
        assert!(state.apply_tool_call_change("s", prompt("Bash", &shown)));
        // With no call of the same input, it falls back to b, the latest call of the tool.
        assert!(!state.apply_tool_call_change("s", ended("b")));
    }

    /// Of identical open calls, a second prompt goes to the call no prompt has been matched to,
    /// so the hand stays up until both calls have ended, in whichever order they end.
    #[test]
    fn identical_prompts_wait_on_identical_calls_each() {
        let input = json!({"command": "make"});
        for (first, second) in [("a", "b"), ("b", "a")] {
            let (state, _dir) = app_state("tool-calls-identical-prompts");
            state.apply_tool_call_change("s", started("a", "Bash", &input));
            state.apply_tool_call_change("s", started("b", "Bash", &input));
            assert!(state.apply_tool_call_change("s", prompt("Bash", &input)));
            assert!(state.apply_tool_call_change("s", prompt("Bash", &input)));
            assert!(state.apply_tool_call_change("s", ended(first)));
            assert!(!state.apply_tool_call_change("s", ended(second)));
        }
    }

    /// When a prompt matches no call by input, a call of its tool that is not waiting is
    /// preferred to one that already is.
    #[test]
    fn a_prompt_with_an_unmatched_input_prefers_a_call_of_its_tool_not_yet_waiting() {
        let (state, _dir) = app_state("tool-calls-tool-only-prompts");
        let (first, second) = (json!({"command": "make"}), json!({"command": "ls"}));
        let (shown_one, shown_two) = (json!({"command": "rm"}), json!({"command": "mv"}));
        state.apply_tool_call_change("s", started("a", "Bash", &first));
        state.apply_tool_call_change("s", started("b", "Bash", &second));
        assert!(state.apply_tool_call_change("s", prompt("Bash", &shown_one)));
        assert!(state.apply_tool_call_change("s", prompt("Bash", &shown_two)));
        assert!(state.apply_tool_call_change("s", ended("b")));
        assert!(!state.apply_tool_call_change("s", ended("a")));
    }

    /// A call that waits is taken for the one a prompt of the same tool and input is for only
    /// when no other such call is left: the match by input comes before the preference.
    #[test]
    fn a_prompt_matched_by_input_does_not_take_a_waiting_call_of_another_input() {
        let (state, _dir) = app_state("tool-calls-input-before-waiting");
        let (first, second) = (json!({"command": "make"}), json!({"command": "ls"}));
        state.apply_tool_call_change("s", started("a", "Bash", &first));
        state.apply_tool_call_change("s", started("b", "Bash", &second));
        // b waits; a's own prompt, with its own input, is still a's.
        assert!(state.apply_tool_call_change("s", prompt("Bash", &second)));
        assert!(state.apply_tool_call_change("s", prompt("Bash", &first)));
        assert!(state.apply_tool_call_change("s", ended("b")));
        assert!(!state.apply_tool_call_change("s", ended("a")));
    }

    /// A question's own prompt does not take another open call of its tool's name in its place,
    /// nor wait twice, when only the question is open.
    #[test]
    fn a_question_prompt_with_another_call_open_still_waits_once() {
        let (state, _dir) = app_state("tool-calls-question-prompt-other-open");
        let (input, make) = (json!({"questions": []}), json!({"command": "make"}));
        state.apply_tool_call_change("s", started("a", "Bash", &make));
        state.apply_tool_call_change("s", question("q", &input));
        assert!(state.apply_tool_call_change("s", prompt("AskUserQuestion", &input)));
        assert!(!state.apply_tool_call_change("s", ended("q")));
    }

    /// What is held for a session goes with the session's other bookkeeping.
    #[test]
    fn the_calls_of_a_session_are_forgotten_with_its_bookkeeping() {
        let (state, _dir) = app_state("tool-calls-forget");
        let input = json!({"questions": []});
        assert!(state.apply_tool_call_change("s", question("q", &input)));
        state.forget_session_bookkeeping("s");
        assert!(!state.apply_tool_call_change("s", ended("a")));
    }
}
