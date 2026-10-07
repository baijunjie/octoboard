import { Button } from "@heroui/react";
import React, { useState } from "react";

import { BareTitleBar } from "./components/TitleBar";
import { Message, useT } from "./i18n/react";
import { useAppExit } from "./lifecycle/useAppExit";

interface StartupScreenProps {
  message: React.ReactNode;
  /** An extra button beside Quit, for a screen that has something to offer besides giving up. */
  action?: { label: string; onSelect: () => void };
}

/**
 * The window with no daemon connection behind it: no address known yet, the daemon failed to
 * start, or the UI itself crashed. Where the platform has a quit flow it always carries its own
 * quit affordance, and that is not cosmetic — the Rust side only lets a window block a quit once
 * some screen has mounted `useAppExit`, and a screen that registers without offering a way out
 * would leave the window unquittable. A plain browser has no quit flow, so there is no Quit.
 *
 * `getSessions`/`requestShutdown` are both omitted: there is no connection here to ask about
 * sessions or to shut down, so quitting happens immediately with no confirmation.
 */
export function StartupScreen({ message, action }: StartupScreenProps): React.ReactElement {
  const t = useT();
  const [quitError, setQuitError] = useState<string>();
  const { requestQuit, canQuit } = useAppExit({ toastError: setQuitError });

  return (
    <div className="flex h-full flex-col">
      <BareTitleBar />
      {/* The window never scrolls (`style.css`), so content taller than it scrolls here, under the
          title bar; `m-auto` centres it while it fits, where `justify-center` would clip its top. */}
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        <div className="m-auto flex flex-col items-center gap-4 p-8 text-center">
          <div className="max-w-xl">{message}</div>
          <div className="flex gap-2">
            {action && <Button onPress={action.onSelect}>{action.label}</Button>}
            {canQuit && <Button onPress={() => void requestQuit()}>{t("common.quit")}</Button>}
          </div>
          {/* Quitting is the only action this screen has, so a quit that fails silently would leave
              the user with nothing at all. */}
          {quitError && <p className="text-danger">{quitError}</p>}
        </div>
      </div>
    </div>
  );
}

/** What the screen says when the shell passed the daemon's startup failure along as `?error=`. */
export function DaemonFailedMessage({ error }: { error: string }): React.ReactElement {
  return <Message id="startup.daemonFailed" params={{ error }} />;
}

/** What the screen says when there is no daemon address to connect to, and how to give it one. */
export function NoAddressMessage(): React.ReactElement {
  return (
    <Message
      id="startup.noAddress"
      params={{
        port: <code>?port=</code>,
        dev: <code>vite dev</code>,
        portVar: <code>VITE_DAEMON_PORT</code>,
        proxyVar: <code>OCTOBOARD_DAEMON_PORT</code>,
      }}
    />
  );
}
