import { useRef } from "react";

import { focusLanding, focusLandingWithoutText, focusTopBar } from "../components/focus/regionFocus";
import type { TerminalPaneHandle } from "../terminal/TerminalPane";
import { interactionIsTouch } from "../input/lastInput";
import type { WelcomeScreenHandle } from "../WelcomeScreen";

/**
 * Where focus meant for the agent terminal goes. The caller attaches `terminalRef` to the terminal
 * pane, `welcomeRef` to the welcome screen and `agentAreaRef` to the terminal's area, which a
 * maximized panel covers and makes inert; the open archive view, which covers the terminal too, is where
 * focus meant for it goes while it is open. `focusTerminal` keeps its identity for good, as it reads
 * nothing but those refs and the DOM.
 */
export function useTerminalFocus() {
  const terminalRef = useRef<TerminalPaneHandle>(null);
  const welcomeRef = useRef<WelcomeScreenHandle>(null);
  const agentAreaRef = useRef<HTMLDivElement>(null);
  // Only one of the two is mounted: the welcome screen stands in for the terminal while there is no
  // console. While a maximized panel covers the agent terminal, focus meant for it goes where F6
  // lands in that panel instead. Read off the DOM rather than the state, so a handoff that runs in
  // the commit restoring the layout (the maximized or floating panel closing) already finds the terminal
  // uncovered: it comes before the pane in the tree, so its `inert` is gone by the time the panel's
  // own handoff runs.
  //
  // After a tap it lands on nothing that takes text (`interactionIsTouch`): the terminal, or a
  // shell terminal covering it, would raise the on-screen keyboard over what was just shown. It
  // lands on a covering panel's first control that takes none, else on the top bar's first control,
  // so focus still never falls to `<body>` from what is going away.
  const focusTerminal = () => {
    const touch = interactionIsTouch();
    const areaInert = agentAreaRef.current?.hasAttribute("inert");
    const covering = areaInert ? document.querySelector("[data-panel-maximized], [data-floating-panel] [data-panel-card]") : null;
    if (covering && (touch ? focusLandingWithoutText : focusLanding)(covering)) return;
    // The open archive view covers the terminal and takes focus itself a moment after it opens, so focus meant
    // for the terminal goes to it, after a tap too (its root takes no text), instead of passing over the top bar.
    // A closing view hands its focus on only once it has closed (`useArchiveView`), when it is no longer open here.
    const archive = areaInert ? null : agentAreaRef.current?.querySelector<HTMLElement>("[data-state=open] > [data-region=archive]");
    if (archive) return archive.focus({ preventScroll: true });
    if (touch) return focusTopBar();
    (terminalRef.current ?? welcomeRef.current)?.focus();
  };
  // `focusTerminal` for the closing archive view's handoff, which has lost its focus to `<body>` or holds it in the
  // view still: unlike `focusTerminal`, it does not leave `<body>` alone, so with no session shown focus lands on the
  // empty state's action, else the top bar.
  const handOffToTerminal = () => {
    focusTerminal();
    const active = document.activeElement;
    if (active && active !== document.body && !active.closest("[data-region=archive]")) return;
    if (terminalRef.current) terminalRef.current.handOff();
    else focusTopBar();
  };
  return { terminalRef, welcomeRef, agentAreaRef, focusTerminal, handOffToTerminal };
}
