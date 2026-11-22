import { createContext, useContext } from "react";

/** Why a session cannot take a quote: it takes no `input` now, or the `program` on its screen does not read a paste as
 * one piece, so the quote's line breaks would send it line by line. */
export type QuoteBlock = "input" | "program";

/** Where a Quote of selected text goes: the input of the session the window shows. */
export interface QuoteTarget {
  /** Why that session cannot take a quote now, if it cannot. Asked as a menu opens, not during a render. */
  blocked: () => QuoteBlock | undefined;
  /** Writes `text` into that session's input as a quote, without sending it. */
  quote: (text: string) => void;
  /** Shows that session's input and puts keyboard focus on it, for a quote made from a dialog that closes with it:
   * `closeSurface` closes that dialog, in one commit with what covers the terminal being cleared (a maximized or floating
   * panel, the archive view, a page), and focus lands at once where `focusTerminal` puts it, before any later restore of
   * the dialog's opener has acted; where the dialog is still on its way out, it lands once that has gone. It commits
   * synchronously, so it is called only from an event or a microtask, never while rendering or from an effect. */
  revealInput: (closeSurface: () => void) => void;
}

/** Absent where there is no session terminal to quote into; Quote is then not offered. */
export const QuoteTargetContext = createContext<QuoteTarget | undefined>(undefined);

export function useQuoteTarget(): QuoteTarget | undefined {
  return useContext(QuoteTargetContext);
}
