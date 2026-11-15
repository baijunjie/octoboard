import { useEffect, useState } from "react";

import { onWindowFocusReturn } from "../components/hooks/windowFocus";
import { usePlatform } from "../platform/react";

/**
 * Whether this computer's SSH server is on (Remote Login on macOS, OpenSSH Server on Windows), as the shell reports it:
 * `undefined` until the first read is in and wherever the shell cannot tell. The user turns it on in the system's
 * settings, outside the app, so it is read when the section opens and again whenever the window gets focus back. A read
 * that fails counts as the shell not being able to tell.
 */
export function useRemoteLogin(): boolean | undefined {
  const { remoteLogin } = usePlatform();
  const [on, setOn] = useState<boolean | undefined>();
  useEffect(() => {
    if (!remoteLogin) return;
    let current = true;
    // Reads overlap when the window regains focus while one is out; only the newest may land.
    let reads = 0;
    const read = () => {
      const attempt = ++reads;
      // Started inside a promise chain, so a capability that throws synchronously fails the read like a rejection.
      void Promise.resolve().then(() => remoteLogin.status()).then(
        (answer) => current && attempt === reads && setOn(answer),
        () => current && attempt === reads && setOn(undefined),
      );
    };
    read();
    const undoFocus = onWindowFocusReturn(read);
    return () => {
      current = false;
      undoFocus();
    };
  }, [remoteLogin]);
  return on;
}
