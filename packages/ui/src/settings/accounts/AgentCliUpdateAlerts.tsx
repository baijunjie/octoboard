import { useEffect } from "react";

import { raiseError } from "../../components/alerts/raiseError";
import { useLatest } from "../../components/hooks/useLatest";
import { CodedError } from "../../errors/codedError";
import { daemonMessage, NO_RECORDS } from "../../errors/daemonMessage";
import { providerLabel } from "../../agents/providers";
import { useCurrentLanguage, useT } from "../../i18n/react";
import { trackAgentCliUpdate, useDaemon } from "../../store";

/** Raises the error alert for a CLI update this window asked for (`cliUpdatesStartedHere`) that ended unfinished, once,
 * and settles the request's mark either way: the user asked for the update, so its failure is theirs to be told. One
 * ended unfinished that another window started, or that was found so when Settings opened, is the Version row's mark
 * alone. Mounted for as long as the window lives, as `LoginPageOpener` is, not as long as the Version dialog is open:
 * the user may close the dialog while the update runs, and the alert must still come. It watches the store itself
 * rather than rendering with the two maps it reads, so no render waits on it. */
export function AgentCliUpdateAlerts(): null {
  const wording = useLatest({ t: useT(), language: useCurrentLanguage() });
  const { store } = useDaemon();
  useEffect(() => {
    const settle = () => {
      const { cliUpdatesStartedHere, agentCliStatuses, providers } = store.getState();
      for (const [agent, from] of cliUpdatesStartedHere) {
        const update = agentCliStatuses.get(agent)?.update;
        // Still the update the status held when the request went out, or still running: not this window's ended yet.
        if (!update || update.state === "running" || update.started_at === from) continue;
        trackAgentCliUpdate(store, agent, undefined);
        if (update.state !== "failed") continue;
        const { t, language } = wording.current;
        const code = update.error ?? "agent_cli_update_failed";
        // The daemon's codes carry no agent; the alert's sentence names it from here, so the params hold it too.
        const params = { agent: providerLabel(t, providers, agent), ...(update.detail ? { detail: update.detail } : {}) };
        const message = daemonMessage(language, code, params, code, NO_RECORDS);
        raiseError(new CodedError(message, code, params), { title: t("settings.accounts.cli.updateFailedTitle") });
      }
    };
    settle();
    return store.subscribe((state, previous) => {
      if (state.agentCliStatuses !== previous.agentCliStatuses || state.cliUpdatesStartedHere !== previous.cliUpdatesStartedHere) settle();
    });
  }, [store]);
  return null;
}
