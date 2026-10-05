import { Button } from "@heroui/react";
import React from "react";

import type { ConnectionState } from "../daemon-client";

/**
 * The strip across the top while the control connection is down: it says the client is retrying on
 * its own while the automatic attempts last, then offers a Retry once they are spent, because
 * nothing retries forever (see "Losing the daemon connection" in
 * `docs/product/application-lifecycle.md`). Renders nothing in any other state.
 *
 * Retry keeps keyboard focus where it is: once the connection is back the banner unmounts, and a
 * press that focused the button would drop focus to `<body>`, away from the terminal.
 */
export function ConnectionBanner({
  state,
  onRetry,
}: {
  state: ConnectionState;
  onRetry: () => void;
}): React.ReactElement | null {
  if (state !== "reconnecting" && state !== "closed") return null;
  return (
    <div role="status" className="flex flex-none items-center justify-center gap-3 bg-warning px-3 py-1 text-xs text-warning-foreground">
      {state === "closed" ? (
        <>
          <span>Disconnected from the daemon.</span>
          <Button size="sm" variant="outline" preventFocusOnPress onPress={onRetry}>
            Retry
          </Button>
        </>
      ) : (
        <span>Disconnected from the daemon — reconnecting…</span>
      )}
    </div>
  );
}
