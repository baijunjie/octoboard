import { createContext, useContext } from "react";

import type { PlatformAdapter } from "./index";

const PlatformContext = createContext<PlatformAdapter | undefined>(undefined);

export const PlatformProvider = PlatformContext.Provider;

export function usePlatform(): PlatformAdapter {
  const platform = useContext(PlatformContext);
  if (!platform) throw new Error("usePlatform() called outside a PlatformProvider");
  return platform;
}
