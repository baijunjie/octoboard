import { useEffect, useState } from "react";

import { useOptionalPlatform } from "./react";

/** The running application's version, as the shell reports it; `undefined` until it has answered, and for a platform
 * that has no updater to ask (a browser). `enabled` false asks nothing. */
export function useAppVersion(enabled = true): string | undefined {
  const platform = useOptionalPlatform();
  const [version, setVersion] = useState<string>();
  useEffect(() => {
    if (!enabled) return;
    let current = true;
    platform?.updater?.currentVersion().then((found) => { if (current) setVersion(found); }, () => {});
    return () => { current = false; };
  }, [enabled, platform]);
  return version;
}
