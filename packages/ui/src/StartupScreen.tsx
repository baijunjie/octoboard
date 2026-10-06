import { Button } from "@heroui/react";
import React, { useState } from "react";

import { BareTitleBar } from "./components/TitleBar";
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
  const [quitError, setQuitError] = useState<string>();
  const { requestQuit, canQuit } = useAppExit({ toastError: setQuitError });

  return (
    <div className="flex min-h-full flex-col">
      <BareTitleBar />
      <div className="flex flex-1 flex-col items-center justify-center gap-4 p-8 text-center">
        <div className="max-w-xl">{message}</div>
        <div className="flex gap-2">
          {action && <Button onPress={action.onSelect}>{action.label}</Button>}
          {canQuit && <Button onPress={() => void requestQuit()}>Quit</Button>}
        </div>
        {/* Quitting is the only action this screen has, so a quit that fails silently would leave
            the user with nothing at all. */}
        {quitError && <p className="text-danger">{quitError}</p>}
      </div>
    </div>
  );
}
