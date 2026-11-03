import { useEffect } from "react";

import { useLatest } from "../components/hooks/useLatest";
import { useLatestRequest } from "../components/hooks/useLatestRequest";
import type { ErrorPresentation } from "../errors/errorPresentation";
import { useT } from "../i18n/react";
import type { Agent, AgentModel, Event, ModelsUnavailable, RequestBody } from "../protocol";
import { shownError } from "./shownError";

/** `idle`: not asked, as nothing needs the list yet. `loading`: the answer for the agent and account
 * the form shows now is not in yet. `ready`: the daemon has answered for them, with a list or with
 * `unavailable` saying why it has none. `failed`: the daemon refused or answered with something else, with `error`. */
export type AgentModelsStatus = "idle" | "loading" | "ready" | "failed";

export interface AgentModels {
  status: AgentModelsStatus;
  models: AgentModel[];
  unavailable: ModelsUnavailable | null;
  error?: ErrorPresentation;
}

const NONE: Pick<AgentModels, "models" | "unavailable"> = { models: [], unavailable: null };
const LOADING: AgentModels = { status: "loading", ...NONE };
const IDLE: AgentModels = { status: "idle", ...NONE };

/**
 * Asks the daemon which models `agent` offers under `account` (an account id, `null` for the default
 * account; `list_agent_models`) while `enabled`, and again whenever the agent or the account changes.
 * Only the answer to the latest question counts: a reply for an agent or account the form has since
 * moved off, or for a dialog that has closed, is dropped.
 */
export function useAgentModels(
  request: (body: RequestBody) => Promise<Event>,
  agent: Agent,
  account: string | null,
  enabled: boolean,
): AgentModels {
  const t = useT();
  const key = enabled ? JSON.stringify([agent, account]) : undefined;
  const { value, send, reconnects } = useLatestRequest<AgentModels>({
    scope: key, initial: LOADING,
  });
  const latest = useLatest({ request, agent, account, send, t });

  useEffect(() => {
    // An answer is kept across the section being collapsed and opened again: the scope the answer is
    // held for already refuses it for any other question.
    if (key === undefined) return;
    const { request, agent, account, send } = latest.current;
    void send({
      request: () => request({ type: "list_agent_models", agent, ...(account && { account }) }),
      accept: (event) => {
        const answer: AgentModels =
          event.type === "agent_models"
            ? { status: "ready", models: event.models, unavailable: event.unavailable }
            : { status: "failed", ...NONE, error: latest.current.t("dialog.agentSettings.modelsUnexpected") };
        return () => answer;
      },
      failed: (failure, _held, t) => ({ status: "failed", ...NONE, error: shownError(failure, t) }),
    });
  }, [key, reconnects]);

  return key === undefined ? IDLE : value;
}
