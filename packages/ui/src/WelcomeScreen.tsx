import { Card, Chip } from "@heroui/react";
import { Check, CircleDashed, FileUp, LayoutDashboard, Minus, Plus } from "lucide-react";
import React, { useId, useImperativeHandle, useRef } from "react";

import { AGENT_OPTIONS } from "./agents";
import { AgentIcon } from "./components/AgentIcon";
import { EmptyPanel } from "./components/EmptyPanel";
import { PATH_VAR_PARAMS } from "./components/literalParams";
import { useFadeRegion, useScrollFade } from "./components/scrollFade";
import type { DialogRequest } from "./dialogs/dialogRequest";
import { Message, useT } from "./i18n/react";
import { usePlatform } from "./platform/react";
import { useDaemonStore } from "./store";

export interface WelcomeScreenHandle {
  focus: () => void;
}

interface WelcomeScreenProps {
  onOpenDialog: (dialog: DialogRequest) => void;
  ref?: React.Ref<WelcomeScreenHandle>;
}

/**
 * What the content area shows while the daemon has no console: the one thing to do is make one, so
 * the sidebar, the terminal and the aside are not rendered and this fills the area alone. The action
 * opens the same new-console dialog the rail's does. Where the platform can open a file, a quieter
 * action beside it imports a configuration exported elsewhere, which is where a computer with
 * nothing on it starts when it is set up from another one. The supported CLI list stays visible
 * with each tool's availability, including while detection is pending. Creating a console stays
 * enabled regardless of availability because its agent fields are only defaults.
 *
 * Its `focus` stands in for the terminal's where `App` hands focus to "the terminal" (`Toasts`, the
 * banner, a pane that hides): with no session the terminal's own hands focus to the top bar, which
 * has no enabled control here, so this hands it to the action. Like the terminal's it leaves focus
 * on `<body>` alone, since `refocusIfLost` asks at mount and launching must not pull focus onto a
 * control.
 */
export function WelcomeScreen({ onOpenDialog, ref }: WelcomeScreenProps): React.ReactElement {
  const t = useT();
  const { configurationFile } = usePlatform();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const rootRef = useRef<HTMLElement>(null);
  const toolsHeadingId = useId();
  const scrollRegion = useFadeRegion({ label: t("welcome.label") });
  // The welcome region owns its landmark and focus target, so the fade stays on the main element.
  const scrollProps = useScrollFade<HTMLElement>({
    ref: rootRef,
    region: scrollRegion,
    className: "flex min-h-0 min-w-0 flex-1 flex-col items-center justify-center-safe overflow-y-auto px-4 pb-8",
  });

  useImperativeHandle(
    ref,
    () => ({
      focus: () => {
        if (document.activeElement !== document.body) rootRef.current?.querySelector("button")?.focus();
      },
    }),
    [],
  );

  return (
    <div className={`flex min-h-0 min-w-0 flex-1 ${scrollRegion.focused ? "ring-2 ring-inset ring-app-focus" : ""}`}>
      <main
        {...scrollProps}
        role="main"
        data-region="welcome"
        aria-label={t("welcome.label")}
      >
        <EmptyPanel
          icon={LayoutDashboard}
          message={t("welcome.heading")}
          description={t("welcome.description")}
          action={{ label: t("welcome.newConsole"), icon: Plus, variant: "primary", onPress: () => onOpenDialog({ kind: "new-console" }) }}
          secondaryAction={
            configurationFile && { label: t("welcome.importConfiguration"), icon: FileUp, onPress: () => onOpenDialog({ kind: "import-configuration" }) }
          }
        />
        <section aria-labelledby={toolsHeadingId} className="w-full max-w-sm shrink-0">
          <h2 id={toolsHeadingId} className="mb-3 text-center text-sm font-medium text-app-muted">
            {t("welcome.supportedTools")}
          </h2>
          <Card>
            <Card.Content>
              <ul>
                {AGENT_OPTIONS.map(({ value, label }) => {
                  const availability = agentAvailability.get(value)?.availability;
                  const available = availability === "available";
                  const unavailable = availability === "unavailable";
                  const StatusIcon = available ? Check : unavailable ? Minus : CircleDashed;
                  return (
                    <li key={value} className="border-b border-separator py-3 first:pt-0 last:border-b-0 last:pb-0">
                      <div role="status" className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
                        <span className="flex min-w-0 items-center gap-3 text-sm font-medium">
                          <AgentIcon agent={value} className="size-5 shrink-0" />
                          <bdi className="break-words">{label}</bdi>
                        </span>
                        <Chip size="sm" variant="soft" color={available ? "success" : "default"}>
                          <StatusIcon aria-hidden="true" className="size-3.5 shrink-0" />
                          <Chip.Label>{t(available ? "welcome.toolInstalled" : unavailable ? "welcome.toolNotFound" : "welcome.toolChecking")}</Chip.Label>
                        </Chip>
                      </div>
                    </li>
                  );
                })}
              </ul>
            </Card.Content>
          </Card>
          <p className="mt-3 text-center text-xs text-app-muted"><Message id="welcome.toolsHint" params={PATH_VAR_PARAMS} /></p>
        </section>
      </main>
    </div>
  );
}
