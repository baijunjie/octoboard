import { ActionableError } from "../../errors/actionableError";
import type { Translate } from "../../i18n/catalog";
import { raiseError } from "../alerts/raiseError";
import type { QuoteBlock } from "./quoteTarget";

/** The error alert for a Quote that was offered and then refused: the session took no input, or its program did not read
 * a paste as one piece, by the time the item was chosen, though neither held when its menu opened. Nothing was written.
 * `inTerminal` is for the session terminal's own menu, which is about the one session right-clicked, not about whichever
 * session the window shows. The remedy is to quote again once the cause has passed. */
export function raiseQuoteRefused(block: QuoteBlock, t: Translate, { inTerminal = false }: { inTerminal?: boolean } = {}): void {
  const message = block === "program" ? t("common.quoteFailedProgram") : t(inTerminal ? "terminal.quoteFailedInput" : "common.quoteFailedInput");
  raiseError(new ActionableError(message, block === "program" ? "quoteProgram" : "quoteInput"), { title: t("common.quoteFailed") });
}
