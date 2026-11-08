import { useEffect } from "react";

import { usePlatform } from "../platform/react";
import { showMenuBarIcon } from "./showMenuBarIcon";
import { useSerialPush } from "../components/hooks/useSerialPush";

/**
 * Pushes the choice to show the menu bar icon to the shell on mount and on every change. The shell
 * shows the icon from launch, so the push of an unchanged default is a no-op there, and a window
 * whose UI never loads keeps the icon. Best-effort: a shell left on its previous choice is not worth
 * surfacing to the user.
 */
export function useMenuBarIconSync(): void {
  const { statusItem } = usePlatform();
  const shown = showMenuBarIcon.useValue();
  // Serial, so two in-flight pushes cannot settle out of order and leave the shell on the earlier choice.
  const push = useSerialPush();

  useEffect(() => {
    if (!statusItem) return;
    push(() => statusItem.setVisible(shown));
  }, [statusItem, shown, push]);
}
