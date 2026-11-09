import { useEffect, useState } from "react";

import { useLatest } from "../components/hooks/useLatest";
import { useLatestRequest } from "../components/hooks/useLatestRequest";
import type { ErrorPresentation } from "../errors/errorPresentation";
import type { Agent, AgentModel, DefaultModelSource, ModelsUnavailable } from "../protocol";
import type { RequestSender } from "../requests";
import { shownError } from "./shownError";

/** `idle`: not asked, as nothing needs the list yet. `loading`: no answer for the agent and account the
 * form shows now has arrived yet in this hook's life. `ready`: the daemon has answered for them, with a
 * list or with `unavailable` saying why it has none; it may be the kept answer while the question is
 * asked again. `failed`: the daemon refused or answered with something else, with `error`, and none was
 * kept for them. */
export type AgentModelsStatus = "idle" | "loading" | "ready" | "failed";

export interface AgentModels {
  status: AgentModelsStatus;
  models: AgentModel[];
  /** The model a session of the agent gets when no setting names one; `null` where the agent's own
   * default applies. */
  defaultModel: string | null;
  /** Where `defaultModel` came from: Claude Code's own configuration, or Octoboard's choice of the
   * best model the account may use; `null` exactly when `defaultModel` is. */
  defaultModelSource: DefaultModelSource | null;
  unavailable: ModelsUnavailable | null;
  error?: ErrorPresentation;
}

const NONE: Pick<AgentModels, "models" | "defaultModel" | "defaultModelSource" | "unavailable"> = { models: [], defaultModel: null, defaultModelSource: null, unavailable: null };
const LOADING: AgentModels = { status: "loading", ...NONE };
const IDLE: AgentModels = { status: "idle", ...NONE };

/**
 * Asks the daemon which models `agent` offers under `account` (an account id, `null` for the default
 * account; `list_agent_models`) while `enabled`, and again whenever the agent or the account changes.
 * Only the answer to the latest question counts: a reply for an agent or account the form has since
 * moved off, or for a dialog that has closed, is dropped.
 *
 * The last answer for each agent and account stays with the hook, and is what a question asked again
 * shows in place of `loading` while it is out, and in place of `failed` when the ask is refused: a form that
 * switches back to an agent it showed before would otherwise lock its pickers again and replace their
 * empty-state hints with loading placeholders for every round trip.
 */
export function useAgentModels(
  request: RequestSender,
  agent: Agent,
  account: string | null,
  enabled: boolean,
): AgentModels {
  const key = enabled ? JSON.stringify([agent, account]) : undefined;
  const { value, send, hold, reconnects } = useLatestRequest<AgentModels>({
    scope: key, initial: LOADING,
  });
  // The answers the daemon gave, a list or why there is none, are kept; a refusal is not.
  const [answered, setAnswered] = useState<ReadonlyMap<string, AgentModels>>(() => new Map());
  const latest = useLatest({ request, agent, account, send });

  useEffect(() => {
    // An answer is kept across the section being collapsed and opened again: the scope the answer is
    // held for already refuses it for any other question.
    if (key === undefined) return;
    const { request, agent, account, send } = latest.current;
    // A refusal held for the scope being left is dropped at once, so the render that comes back to it does not
    // show it for a frame before the ask below puts `loading` in its place.
    const leave = () => hold((held) => (held.status === "failed" ? LOADING : held), key);
    void send({
      request: () => request({ type: "list_agent_models", agent, ...(account && { account }) }),
      accept: (event) => {
        const answer: AgentModels = { status: "ready", models: event.models, defaultModel: event.default_model ?? null, defaultModelSource: event.default_model_source ?? null, unavailable: event.unavailable };
        setAnswered((kept) => new Map(kept).set(key, answer));
        return () => answer;
      },
      // A refusal held from an earlier ask is not an answer: asking again shows `loading` as the first ask does.
      pending: (held) => (held.status === "failed" ? LOADING : held),
      failed: (failure, _held, t) => ({ status: "failed", ...NONE, error: shownError(failure, t) }),
    });
    return leave;
  }, [key, reconnects]);

  if (key === undefined) return IDLE;
  return value.status === "loading" || value.status === "failed" ? (answered.get(key) ?? value) : value;
}
