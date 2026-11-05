import { useT } from "../../i18n/react";
import { usePlatform } from "../../platform/react";

export function useShortcutScopeDescription(): string {
  const t = useT();
  const { kind } = usePlatform();
  return t(kind === "tauri" ? "shortcuts.scope.desktop" : "shortcuts.scope.browser");
}
