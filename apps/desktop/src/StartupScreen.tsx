import React, { useState } from "react";

import { useAppExit } from "./lifecycle/useAppExit";

interface StartupScreenProps {
  message: React.ReactNode;
  /** An extra button beside Quit, for a screen that has something to offer besides giving up. */
  action?: { label: string; onSelect: () => void };
}

/**
 * The window with no daemon connection behind it: no port known yet, the daemon failed to start, or
 * the UI itself crashed. Always carries its own quit affordance, and that is not cosmetic — the
 * Rust side only lets a window block a quit once some screen has mounted `useAppExit`, and a screen
 * that registers without offering a way out would leave the window unquittable.
 *
 * `getSessions`/`requestShutdown` are both omitted: there is no connection here to ask about
 * sessions or to shut down, so quitting happens immediately with no confirmation.
 */
export function StartupScreen({ message, action }: StartupScreenProps): React.ReactElement {
  const [quitError, setQuitError] = useState<string>();
  const { requestQuit } = useAppExit({ toastError: setQuitError });

  return (
    <div className="message-screen">
      <div>{message}</div>
      <div className="message-screen-actions">
        {action && (
          <button type="button" onClick={action.onSelect}>
            {action.label}
          </button>
        )}
        <button type="button" onClick={() => void requestQuit()}>
          Quit
        </button>
      </div>
      {/* Quitting is the only action this screen has, so a quit that fails silently would leave
          the user with nothing at all. */}
      {quitError && <p className="message-screen-error">{quitError}</p>}
    </div>
  );
}
