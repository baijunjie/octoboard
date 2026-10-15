import { useEffect, useState } from "react";

import { useLatest } from "../components/useLatest";
import { useT } from "../i18n/react";
import type { Agent, AgentModel, Event, ModelsUnavailable, RequestBody } from "../protocol";

/** `idle`: not asked, as nothing needs the list yet. `loading`: the answer for the agent and account
 * the form shows now is not in yet. `ready`: the daemon has answered for them, with a list or with
 * `unavailable` saying why it has none. `failed`: the daemon refused or answered with something else, with `error`. */
export type AgentModelsStatus = "idle" | "loading" | "ready" | "failed";

export interface AgentModels {
  status: AgentModelsStatus;
  models: AgentModel[];
  unavailable: ModelsUnavailable | null;
  error?: string;
}

type Answer = Pick<AgentModels, "models" | "unavailable" | "error"> & { key: string };

const NONE: Pick<AgentModels, "models" | "unavailable"> = { models: [], unavailable: null };

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
  const [answered, setAnswered] = useState<Answer>();
  const latest = useLatest({ request, agent, account, t });

  useEffect(() => {
    // An answer is kept across the section being collapsed and opened again: the `key` check below
    // already refuses it for any other question.
    if (key === undefined) return;
    let current = true;
    const { request, agent, account, t } = latest.current;
    request({ type: "list_agent_models", agent, ...(account && { account }) }).then(
      (event) => {
        if (!current) return;
        setAnswered(
          event.type === "agent_models"
            ? { key, models: event.models, unavailable: event.unavailable }
            : { key, ...NONE, error: t("dialog.agentSettings.modelsUnexpected") },
        );
      },
      (err: Error) => current && setAnswered({ key, ...NONE, error: err.message }),
    );
    return () => {
      current = false;
    };
  }, [key]);

  if (key === undefined) return { status: "idle", ...NONE };
  if (answered?.key !== key) return { status: "loading", ...NONE };
  return { status: answered.error === undefined ? "ready" : "failed", models: answered.models, unavailable: answered.unavailable, error: answered.error };
}
