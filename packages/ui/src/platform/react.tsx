import { createContext, useContext } from "react";

import type { PlatformAdapter } from "./index";

const PlatformContext = createContext<PlatformAdapter | undefined>(undefined);

export const PlatformProvider = PlatformContext.Provider;

export function useOptionalPlatform(): PlatformAdapter | undefined {
  return useContext(PlatformContext);
}

export function usePlatform(): PlatformAdapter {
  const platform = useOptionalPlatform();
  if (!platform) throw new Error("usePlatform() called outside a PlatformProvider");
  return platform;
}
