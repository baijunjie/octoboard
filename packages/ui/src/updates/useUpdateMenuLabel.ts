import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { useUpdateSelector } from "./UpdatesProvider";

/** The menu entry follows update availability even when the window's notice is hidden. Progress ticks leave the
 * label unchanged, so they do not render the caller or rebuild the native menu. */
export function useUpdateMenuLabel(): string | undefined {
  const t = useT();
  return useUpdateSelector((state) => {
    if (state.phase === "installed") return t("settings.updates.version.label");
    if (state.phase === "downloading") return t("updates.notice.downloading", { version: isolate(state.update?.version ?? "") });
    if (state.update) return t("statusItem.updateAvailable", { version: isolate(state.update.version) });
    return t("settings.updates.check");
  });
}
