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
  /** Shows that session's input and puts keyboard focus on it, for a quote made from a surface that closes with it:
   * a panel covering the terminal is cleared (restored, or closed where it floats), and focus lands once the
   * surface's own dialog is gone, after its focus return. Called as the surface closes. */
  revealInput: () => void;
}

/** Absent where there is no session terminal to quote into; Quote is then not offered. */
export const QuoteTargetContext = createContext<QuoteTarget | undefined>(undefined);

export function useQuoteTarget(): QuoteTarget | undefined {
  return useContext(QuoteTargetContext);
}
