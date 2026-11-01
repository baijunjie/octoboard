// Imports nothing that reads a stored preference as it loads (the update provider among it): `frame.tsx` imports this
// before `prepareStorage` has run, and one that did would start from the previous scenario's language.
import type { UpdaterCapability } from "../platform";
import { UpdaterError } from "../platform/updaterError";
import type { UpdaterFixture } from "./scenario";

const NEVER = new Promise<never>(() => {});

/** The shell's updater as a scenario describes it (`UpdaterFixture`), which the real `UpdatesProvider` then
 * drives exactly as it drives the desktop app's. */
export function fixtureUpdater({ currentVersion = "0.1.0", check = "upToDate", install = {} }: UpdaterFixture): UpdaterCapability {
  return {
    currentVersion: async () => currentVersion,
    check: async () => {
      if (check === "pending") return NEVER;
      if (check === "upToDate") return undefined;
      if ("error" in check) throw new UpdaterError("failed", check.error);
      return check;
    },
    install: async (onProgress) => {
      if (install.progress) onProgress(install.progress);
      if (install.ends === "pending") return NEVER;
      if (install.ends === "gone") throw new UpdaterError("nothing_to_install");
    },
  };
}
