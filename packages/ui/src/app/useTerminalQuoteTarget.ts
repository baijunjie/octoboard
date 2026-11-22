import { useState, type RefObject } from "react";

import type { QuoteTarget } from "../components/host/quoteTarget";
import type { TerminalPaneHandle } from "../terminal/TerminalPane";

/** The quote target of the window: the session terminal pane behind `terminalRef`, read when asked, so the value
 * keeps its identity for good and nothing that reads it renders as the session changes. */
export function useTerminalQuoteTarget(terminalRef: RefObject<TerminalPaneHandle | null>): QuoteTarget {
  const [target] = useState<QuoteTarget>(() => ({
    blocked: () => (terminalRef.current ? terminalRef.current.quoteBlock() : "input"),
    quote: (text) => terminalRef.current?.quote(text),
  }));
  return target;
}
