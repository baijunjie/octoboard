import React, { useLayoutEffect, useRef, useState } from "react";

import { Button } from "../../components/controls/Button";
import { providerLabel } from "../../agents/providers";
import { AgentIcon } from "../../components/icons/AgentIcon";
import { ExternalLink } from "../../components/text/ExternalLink";
import { FadeOverflow } from "../../components/text/FadeOverflow";
import { StatusBadge } from "../../components/status/StatusBadge";
import { Dialog } from "../../components/dialog/Dialog";
import { useDialogAction, useFocusLeavingWhenDisabled, useRefocusIfLost } from "../../components/dialog/dialogHooks";
import { appDataRecovery } from "../../errors/errorRecovery";
import { useT } from "../../i18n/react";
import type { Agent } from "../../protocol";
import { isolate } from "../../i18n/isolate";
import { useDaemon, useDaemonStore, useProviders } from "../../store";

/** Every served agent with whether it is installed, to add the installed ones that are not added yet: the dialog stays
 * open for more, and an agent added shows as such in place. An agent that is not installed offers its provider's
 * install instructions instead of the button; one whose installation is not yet determined, or any while the daemon
 * has not decided the list, cannot be added yet. A refusal is acknowledged like any other failure. */
export function AddAgentDialog({ onClose }: { onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const added = useDaemonStore((s) => s.settings.added_agents);
  const availability = useDaemonStore((s) => s.agentAvailability);
  const providers = useProviders();
  const { busy, run } = useDialogAction(undefined, t("settings.accounts.addAgentFailedTitle"), appDataRecovery);
  const [pending, setPending] = useState<Agent>();
  const closeRef = useRef<HTMLButtonElement>(null);
  // An agent just added takes its button with it, and focus with that. Close is disabled until the request
  // settles, so focus goes back when `busy` ends too.
  useRefocusIfLost(() => closeRef.current, [added, busy]);
  // Add is pressed with `preventFocusOnPress`, so focus can still be on Close, or on the dialog's close button, as
  // both turn disabled for the request: it moves to the pending Add button, which stays focusable, rather than
  // falling to `<body>` where the dialog's Escape and Tab stop working.
  const pendingRef = useRef<HTMLButtonElement>(null);
  const closeTriggerRef = useRef<HTMLElement | null>(null);
  // Above the two hooks below: they read the ref when they mount.
  useLayoutEffect(() => {
    closeTriggerRef.current = closeRef.current?.closest("[role=dialog]")?.querySelector<HTMLElement>("[data-slot$=close-trigger]") ?? null;
  }, []);
  useFocusLeavingWhenDisabled(closeRef, busy, pendingRef);
  useFocusLeavingWhenDisabled(closeTriggerRef, busy, pendingRef);

  const add = (agent: Agent) => {
    setPending(agent);
    void run(async () => {
      await request({ type: "add_agent", agent });
    }).finally(() => setPending(undefined));
  };

  return (
    <Dialog
      title={t("settings.accounts.addAgent")}
      onClose={onClose}
      busy={busy}
      footer={
        <Button ref={closeRef} type="button" variant="secondary" onPress={onClose} isDisabled={busy}>
          {t("common.close")}
        </Button>
      }
    >
      <p className="text-sm text-app-muted">{t("settings.addAgent.description")}</p>
      <ul aria-label={t("settings.accounts.agents")} className="flex flex-col">
        {providers.map((provider) => {
          const agent = provider.id;
          const label = providerLabel(t, providers, agent);
          const installUrl = provider.install_url;
          const state = availability.get(agent)?.availability;
          const isAdded = added?.includes(agent) ?? false;
          const status = isAdded ? "added" : state === "available" ? "installed" : state === "unavailable" ? "notInstalled" : "checking";
          const dimmed = status !== "installed";
          return (
            <li key={agent} className="flex items-center gap-3 groove-b py-3 first:pt-0 last:border-b-0 last:pb-0">
              <AgentIcon agent={agent} className={`size-5 shrink-0 ${dimmed ? "opacity-60" : ""}`} />
              <div className="flex min-w-0 flex-1 items-center gap-2">
                <FadeOverflow as="span" dir="auto" className={`min-w-0 ${dimmed ? "text-app-muted" : ""}`} titleWhenClipped={label}>{label}</FadeOverflow>
                <StatusBadge>{t(`settings.addAgent.${status}`)}</StatusBadge>
              </div>
              {status === "notInstalled" && installUrl && (
                <ExternalLink href={installUrl} aria-label={t("settings.addAgent.installLabel", { agent: isolate(label) })}>
                  {t("settings.addAgent.install")}
                </ExternalLink>
              )}
              {status !== "added" && status !== "notInstalled" && (
                <Button
                  ref={pending === agent ? pendingRef : undefined}
                  size="sm"
                  variant="outline"
                  preventFocusOnPress
                  aria-label={t("settings.addAgent.addLabel", { agent: isolate(label) })}
                  isDisabled={status !== "installed" || added === null || (busy && pending !== agent)}
                  isPending={pending === agent}
                  onPress={() => add(agent)}
                >
                  {t("common.add")}
                </Button>
              )}
            </li>
          );
        })}
      </ul>
    </Dialog>
  );
}
