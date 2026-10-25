// Imports nothing that reads a stored preference as it loads (the update provider among it): `frame.tsx` imports this
// before `prepareStorage` has run, and one that did would start from the previous scenario's language.
import type { UpdateChannel, UpdaterCapability } from "../platform";
import { UpdaterError } from "../platform/updaterError";
import type { UpdaterCheck, UpdaterFixture } from "./scenario";

const NEVER = new Promise<never>(() => {});

/** Whether `check` answers per channel rather than the same for both. */
const perChannel = (check: NonNullable<UpdaterFixture["check"]>): check is Partial<Record<UpdateChannel, UpdaterCheck>> =>
  typeof check === "object" && ("beta" in check || "stable" in check);

/** The shell's updater as a scenario describes it (`UpdaterFixture`), which the real `UpdatesProvider` then
 * drives exactly as it drives the desktop app's. */
export function fixtureUpdater({ currentVersion = "0.2.0-beta.1", check = "upToDate", install = {} }: UpdaterFixture): UpdaterCapability {
  return {
    currentVersion: async () => currentVersion,
    check: async (channel) => {
      const answer = perChannel(check) ? (check[channel] ?? "upToDate") : check;
      if (answer === "pending") return NEVER;
      if (answer === "upToDate") return undefined;
      if (answer === "installed") throw new UpdaterError("installed");
      if ("error" in answer) throw new UpdaterError("failed", answer.error);
      return answer;
    },
    install: async (onProgress) => {
      if (install.progress) onProgress(install.progress);
      if (install.ends === "pending") return NEVER;
      if (install.ends === "gone") throw new UpdaterError("nothing_to_install");
      if (install.ends) throw new UpdaterError("failed", install.ends.error);
    },
  };
}
