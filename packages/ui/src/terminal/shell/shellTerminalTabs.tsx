import { X } from "lucide-react";
import React, { useRef, useState } from "react";

import { HandoffOnUnmount } from "../../components/focus/HandoffOnUnmount";
import { AnchoredTooltip } from "../../components/hint/AnchoredTooltip";
import { ActionMenu } from "../../components/menus/ActionMenu";
import type { ActionMenuEntry } from "../../components/menus/menuEntries";
import { t } from "../../i18n/language";
import { raiseError } from "../../components/alerts/raiseError";
import { menuOfTrigger } from "../../primitives";
import type { Daemon } from "../../store";

// The parts of a shell terminal's tab strip that stand on their own: what a tab holds and does,
// apart from the panel's state.

/** Asks the daemon to close `terminal`; a refusal is reported as an error. Settles once it has answered. */
export async function closeShellTerminal(daemon: Pick<Daemon, "request">, terminal: string): Promise<void> {
  try {
    await daemon.request({ type: "close_shell_terminal", shell_terminal: terminal });
  } catch (err) {
    raiseError(err, { title: t("shellTerminal.closeFailedTitle") });
  }
}

/** A tab's context menu (a right-click, or Shift+F10 and the menu key on the focused tab), with the
 * actions the tab row's menu has for the shown tab. It attaches to `tab` as it mounts, so it is
 * mounted once the tab is. `onGone` is called when it unmounts with focus in its open menu, as the
 * tab goes while the menu is up. */
export function TabContextMenu({ label, items, tab, onGone }: { label: string; items: ActionMenuEntry[]; tab: HTMLElement; onGone: () => void }): React.ReactElement {
  const [target] = useState<React.RefObject<HTMLElement | null>>(() => ({ current: tab }));
  // Kept past the wrapper's unmount, whose ref is cleared before the handoff looks for the menu.
  const scope = useRef<HTMLDivElement | null>(null);
  // Only this menu: another one open (the tab row's) keeps its focus when a tab goes. The menu is
  // labelled by its hidden trigger, in the wrapper.
  const ownMenu = () => menuOfTrigger(scope.current?.querySelector("button"));
  return (
    <div ref={(element) => { if (element) scope.current = element; }} className="contents">
      <HandoffOnUnmount from={ownMenu} to={onGone} />
      <ActionMenu label={label} items={items} contextTargetRef={target} contextOnly />
    </div>
  );
}

/** Scrolls `scroller` just far enough to bring `tab` into view, honouring the scroll padding and
 * margins the strip and the tab set. The pixel difference is between rects, so it holds in a
 * right-to-left strip, where `scrollLeft` counts from the right edge. */
export function revealInScroller(scroller: HTMLElement, tab: HTMLElement): void {
  const px = (value: string) => parseFloat(value) || 0;
  const strip = getComputedStyle(scroller);
  const own = getComputedStyle(tab);
  const box = scroller.getBoundingClientRect();
  const rect = tab.getBoundingClientRect();
  const beforeEdge = box.left + px(strip.scrollPaddingLeft);
  const afterEdge = box.right - px(strip.scrollPaddingRight);
  const behind = rect.left - px(own.scrollMarginLeft) - beforeEdge;
  const ahead = rect.right + px(own.scrollMarginRight) - afterEdge;
  if (behind < 0) scroller.scrollLeft += behind;
  else if (ahead > 0) scroller.scrollLeft += Math.min(ahead, behind);
}

/** A tab's close mark, for the pointer only and kept out of the tab's own press so it closes rather
 * than selects: the keyboard closes a tab from the tab row's menu or the tab's context menu, as a
 * tab holds no control of its own. Drawn at 16 and hit at
 * 24 (`::after`), and its tooltip is anchored, as a tab cannot hold a trigger. */
export function TabCloseMark({ title, onClose }: { title: string; onClose: () => void }): React.ReactElement {
  const ref = useRef<HTMLSpanElement>(null);
  const [hovered, setHovered] = useState(false);
  return (
    <>
      <span
        ref={ref}
        aria-hidden="true"
        className="relative flex size-4 items-center justify-center rounded-sm text-app-muted after:absolute after:-inset-1 after:content-[''] transition-colors duration-100 hover:bg-foreground/15 hover:text-foreground active:bg-foreground/25 active:text-foreground motion-reduce:transition-none"
        onPointerEnter={() => setHovered(true)}
        onPointerLeave={() => setHovered(false)}
        onPointerDown={(event) => event.stopPropagation()}
        onMouseDown={(event) => event.preventDefault()}
        onClick={(event) => {
          event.stopPropagation();
          onClose();
        }}
      >
        <X className="size-3" />
      </span>
      <AnchoredTooltip tooltip={title} anchor={ref} hovered={hovered} />
    </>
  );
}
