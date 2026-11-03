import { useRef } from "react";

import { focusLanding, focusLandingWithoutText, focusTopBar } from "../components/focus/regionFocus";
import type { TerminalPaneHandle } from "../terminal/TerminalPane";
import { interactionIsTouch } from "../input/touchInput";
import type { WelcomeScreenHandle } from "../WelcomeScreen";

/**
 * Where focus meant for the agent terminal goes. The caller attaches `terminalRef` to the terminal
 * pane, `welcomeRef` to the welcome screen and `agentAreaRef` to the terminal's area, which a
 * maximized panel covers and makes inert. `focusTerminal` keeps its identity for good, as it reads
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
    const covering = agentAreaRef.current?.hasAttribute("inert") ? document.querySelector("[data-panel-maximized], [data-floating-panel] [data-panel-card]") : null;
    if (covering && (touch ? focusLandingWithoutText : focusLanding)(covering)) return;
    if (touch) return focusTopBar();
    (terminalRef.current ?? welcomeRef.current)?.focus();
  };
  return { terminalRef, welcomeRef, agentAreaRef, focusTerminal };
}
