import { useEffect } from "react";

import { useLatest } from "../components/hooks/useLatest";
import { useLatestRequest } from "../components/hooks/useLatestRequest";
import type { ErrorPresentation } from "../errors/errorPresentation";
import type { Agent, AgentModel, DefaultModelSource, ModelsUnavailable } from "../protocol";
import type { RequestSender } from "../requests";
import { shownError } from "./shownError";

/** `idle`: not asked, as nothing needs the list yet. `loading`: the answer for the agent and account
 * the form shows now is not in yet. `ready`: the daemon has answered for them, with a list or with
 * `unavailable` saying why it has none. `failed`: the daemon refused or answered with something else, with `error`. */
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
 */
export function useAgentModels(
  request: RequestSender,
  agent: Agent,
  account: string | null,
  enabled: boolean,
): AgentModels {
  const key = enabled ? JSON.stringify([agent, account]) : undefined;
  const { value, send, reconnects } = useLatestRequest<AgentModels>({
    scope: key, initial: LOADING,
  });
  const latest = useLatest({ request, agent, account, send });

  useEffect(() => {
    // An answer is kept across the section being collapsed and opened again: the scope the answer is
    // held for already refuses it for any other question.
    if (key === undefined) return;
    const { request, agent, account, send } = latest.current;
    void send({
      request: () => request({ type: "list_agent_models", agent, ...(account && { account }) }),
      accept: (event) => () => ({ status: "ready", models: event.models, defaultModel: event.default_model ?? null, defaultModelSource: event.default_model_source ?? null, unavailable: event.unavailable }),
      failed: (failure, _held, t) => ({ status: "failed", ...NONE, error: shownError(failure, t) }),
    });
  }, [key, reconnects]);

  return key === undefined ? IDLE : value;
}
