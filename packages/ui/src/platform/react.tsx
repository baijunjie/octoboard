import { createContext, useContext } from "react";

import { browserHost, type HostFacts } from "./host";
import type { PlatformAdapter } from "./index";

const PlatformContext = createContext<PlatformAdapter | undefined>(undefined);

export const PlatformProvider = PlatformContext.Provider;

export function useOptionalPlatform(): PlatformAdapter | undefined {
  return useContext(PlatformContext);
}

/** What a browser derives, for a component with no platform above it; read once, so each render gets the same object. */
const BROWSER_HOST = browserHost();

/** The facts of the machine the window runs on (`PlatformAdapter.host`); for a component that does not otherwise
 * need a platform, what a browser derives where there is none above it. */
export function useHost(): HostFacts {
  return useOptionalPlatform()?.host ?? BROWSER_HOST;
}

export function usePlatform(): PlatformAdapter {
  const platform = useOptionalPlatform();
  if (!platform) throw new Error("usePlatform() called outside a PlatformProvider");
  return platform;
}
