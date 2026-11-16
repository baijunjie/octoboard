import { useEffect, useState } from "react";

import type { RemoteForward, RemoteForwardsCapability } from "../platform";

const NONE: RemoteForward[] = [];

/** The ports this remote window forwards to its daemon's machine, in the order they were made, read again whenever the
 * shell says they changed. Subscribed first and listed after, so a change between the two is not missed; an answer to
 * an earlier listing never replaces a later one. Empty where there is no capability (any window but a remote one). */
export function useRemoteForwards(capability: RemoteForwardsCapability | undefined): RemoteForward[] {
  const [forwards, setForwards] = useState<RemoteForward[]>(NONE);
  useEffect(() => {
    if (!capability) return;
    let latest = 0;
    let live = true;
    const load = () => {
      const mine = ++latest;
      capability.list().then((list) => live && mine === latest && setForwards(list), () => {});
    };
    const unsubscribe = capability.onChange(load);
    load();
    return () => {
      live = false;
      unsubscribe();
    };
  }, [capability]);
  return capability ? forwards : NONE;
}
