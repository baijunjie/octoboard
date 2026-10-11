import { LayoutDashboard, Plus } from "lucide-react";
import React, { useImperativeHandle, useRef } from "react";

import { noAgentAvailable } from "./agents";
import { EmptyPanel } from "./components/EmptyPanel";
import type { DialogRequest } from "./dialogs/dialogRequest";
import { useT } from "./i18n/react";
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
 * opens the same new-console dialog the rail's does. With no agent on the machine's `PATH` the
 * install prompt is shown beside it, as the sidebar's console sessions section shows it, but the
 * button stays enabled: a console's agent fields are only defaults.
 *
 * Its `focus` stands in for the terminal's where `App` hands focus to "the terminal" (`Toasts`, the
 * banner, a pane that hides): with no session the terminal's own hands focus to the top bar, which
 * has no enabled control here, so this hands it to the action. Like the terminal's it leaves focus
 * on `<body>` alone, since `refocusIfLost` asks at mount and launching must not pull focus onto a
 * control.
 */
export function WelcomeScreen({ onOpenDialog, ref }: WelcomeScreenProps): React.ReactElement {
  const t = useT();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const rootRef = useRef<HTMLElement>(null);

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
    <main
      ref={rootRef}
      data-region="welcome"
      aria-label={t("welcome.label")}
      className="flex min-w-0 flex-1 flex-col items-center justify-center"
    >
      <EmptyPanel
        icon={LayoutDashboard}
        message={t("welcome.heading")}
        description={t("welcome.description")}
        action={{ label: t("welcome.newConsole"), icon: Plus, onPress: () => onOpenDialog({ kind: "new-console" }) }}
      />
      {noAgentAvailable(agentAvailability) && (
        <p className="max-w-sm px-4 text-center text-sm text-muted">{t("agents.installPrompt")}</p>
      )}
    </main>
  );
}
