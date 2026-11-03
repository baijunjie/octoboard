import type React from "react";
import { useEffect, useLayoutEffect, useRef } from "react";

import { useLatest } from "../components/hooks/useLatest";
import { useRefocusIfLost } from "../components/dialog/dialogHooks";
import { arrowNavigation } from "./arrowKeys";
import type { ViewerContent } from "./content";
import type { ViewerNavigation } from "./viewerNavigation";

/**
 * Keeps keyboard focus working in the viewer's dialog while its subject and navigation change: the
 * refs of the Previous and Next buttons and of the header (`headerRef`, which finds the dialog), the
 * record of which button had focus, and the effects that move focus back and handle Left and Right.
 */
export function useNavigationFocus({
  navigation,
  subjectKey,
  content,
  wrapClaimed,
}: {
  navigation: ViewerNavigation | undefined;
  subjectKey: string;
  content: ViewerContent;
  wrapClaimed: boolean;
}) {
  // A navigation button that turns disabled while it has focus — the last subject reached with
  // Next, or the list changing under the viewer — loses focus to `<body>`, taking the dialog's
  // Escape and Tab containment with it. Focus moves to the other button when it still leads
  // somewhere, and to the dialog otherwise. Which button had focus is remembered from its own
  // focus events, since by the time this runs WebKit may already have moved focus off it.
  const previousRef = useRef<HTMLButtonElement>(null);
  const nextRef = useRef<HTMLButtonElement>(null);
  // On an element the viewer always renders, so the dialog is found even when the buttons are gone.
  const headerRef = useRef<HTMLDivElement>(null);
  const focusedNavigation = useRef<"previous" | "next">(undefined);
  const hasPrevious = Boolean(navigation?.onPrevious);
  const hasNext = Boolean(navigation?.onNext);
  useLayoutEffect(() => {
    const which = focusedNavigation.current;
    if (!which || (which === "previous" ? hasPrevious : hasNext)) return;
    const button = which === "previous" ? previousRef.current : nextRef.current;
    if (document.activeElement !== button && document.activeElement !== document.body) return;
    const other = which === "previous" ? (hasNext ? nextRef.current : null) : hasPrevious ? previousRef.current : null;
    (other ?? headerRef.current?.closest<HTMLElement>("[role=dialog]"))?.focus();
  }, [subjectKey, hasPrevious, hasNext]);
  // The subject's content can be replaced under the same subject — read again, or the connection
  // lost — taking the code region that held focus with it, and the wrap choice goes once the last
  // claim on it is released, a commit after the content changed; the dialog takes focus back, or
  // its Escape and Tab would stop working.
  useRefocusIfLost(() => headerRef.current?.closest<HTMLElement>("[role=dialog]"), [content, wrapClaimed]);
  // The latest callbacks, for a listener added once.
  const latestNavigation = useLatest(navigation);
  useEffect(() => {
    const dialog = headerRef.current?.closest<HTMLElement>("[role=dialog]");
    if (!dialog) return;
    const onKeyDown = (event: KeyboardEvent) => {
      const direction = arrowNavigation(event, dialog);
      if (!direction) return;
      event.preventDefault();
      event.stopPropagation();
      (direction === "previous" ? latestNavigation.current?.onPrevious : latestNavigation.current?.onNext)?.();
    };
    dialog.addEventListener("keydown", onKeyDown);
    return () => dialog.removeEventListener("keydown", onKeyDown);
  }, []);
  const focusNavigation = (which: "previous" | "next") => {
    focusedNavigation.current = which;
  };
  const blurNavigation = (which: "previous" | "next", event: React.FocusEvent) => {
    // A blur to nowhere is the button turning disabled under focus; anything else is the user
    // moving on.
    if (event.relatedTarget !== null && focusedNavigation.current === which) focusedNavigation.current = undefined;
  };

  return { previousRef, nextRef, headerRef, focusNavigation, blurNavigation };
}
