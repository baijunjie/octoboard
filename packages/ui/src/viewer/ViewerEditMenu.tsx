import { Copy, TextQuote, TextSelect } from "lucide-react";
import React, { useContext, useRef, useState } from "react";

import { raiseError } from "../components/alerts/raiseError";
import { copyText } from "../components/host/copyText";
import { useQuoteTarget, type QuoteBlock } from "../components/host/quoteTarget";
import { raiseQuoteRefused } from "../components/host/raiseQuoteRefused";
import { ActionMenu } from "../components/menus/ActionMenu";
import { ActionableError } from "../errors/actionableError";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { isWholeDocumentSelected } from "./selectAll";
import { SurfaceCloseContext } from "./surfaceClose";
import { readSelection, restoreSelection, type FoundSelection } from "./textSelection";

/** Where a browser tab keeps its own menu: a link and an image, which that menu opens and saves. */
const NATIVE_MENU_TARGET = "a[href], img";

/**
 * The right-click menu of one of the viewer's text surfaces: Copy and Quote, while text of it is selected, and
 * Select All. Quote writes the selection into the input of the session the window shows, where there is a terminal
 * to quote into (`useQuoteTarget`). `region` is the surface's own focusable element, which Select All and a copy are
 * about; `selectAll` is what that surface's Select All key does there. `source` is the Markdown
 * of a rendered document, which a copy of the whole document puts on the clipboard, as the key's
 * does (`copyDocument`).
 *
 * Select All is the key's: where `selectAll` leaves the key to the browser (it returns false), so does the menu.
 * The selection stays through the menu: a browser may clear it as focus moves into the menu, so it is put back
 * when the menu closes without a Select All. A Select All is carried out once the menu has closed, not when it is
 * chosen: WebKit drops a selection made while the surface behind the menu is still inert, as focus returns to it.
 *
 * Where the surface is in a dialog that provides `SurfaceCloseContext` (the file viewer, and Find in files, whose preview
 * holds one), Quote closes that dialog once it has written the quote, so the input shows, and the input takes focus
 * instead of the dialog's opener (`QuoteTarget.revealInput`). The dialog closes once the menu has closed, as Select All
 * acts: the dialog going while the menu still returns focus to it would fight that return.
 */
export function ViewerEditMenu({ region, selectAll, source }: {
  region: React.RefObject<HTMLElement | null>;
  selectAll: (region: HTMLElement) => boolean;
  source?: string;
}): React.ReactElement {
  const t = useT();
  const { clipboard, kind } = usePlatform();
  // Read when the menu opens: opening it moves focus, and the selection of code lies in a shadow root
  // that only `readSelection` reads.
  const [selection, setSelection] = useState("");
  const quoteTarget = useQuoteTarget();
  const closeSurface = useContext(SurfaceCloseContext);
  const [quoteBlock, setQuoteBlock] = useState<QuoteBlock | undefined>("input");
  // The selection as the menu found it, with the text it offers to copy, until the menu has gone.
  const kept = useRef<FoundSelection | null>(null);
  // Set when Select All is chosen, for the close that follows.
  const selectingAll = useRef(false);
  // Set when Quote is chosen inside the viewer, to the text to quote, for the close that follows.
  const quoting = useRef<string | null>(null);
  // The control inside the surface that had focus as the menu opened (the Menu key on a link), which the menu gives
  // focus back to.
  const control = useRef<Element | null>(null);
  const selectEverything = () => {
    const element = region.current;
    if (!element || selectAll(element)) return;
    // The key's own fallback.
    element.focus({ preventScroll: true });
    element.ownerDocument.execCommand("selectAll");
  };
  const copyItem = {
    label: t("common.copy"), icon: Copy,
    onClick: () => { void copyText(selection, { clipboard }).catch((cause: unknown) => raiseError(new ActionableError(t("common.copyFailedCause"), "clipboardCopy", { cause }), { title: t("viewer.copyFailed") })); },
  };
  const quoteItem = {
    label: t("common.quote"), icon: TextQuote, disabled: quoteBlock !== undefined,
    reason: quoteBlock && { state: t(quoteBlock === "program" ? "common.quoteProgramBlocked" : "viewer.quoteUnavailable") },
    onClick: () => {
      if (closeSurface) {
        quoting.current = selection;
        return;
      }
      // The item was offered when the menu opened: the session may have stopped taking a quote since.
      const block = quoteTarget ? quoteTarget.blocked() : "input";
      if (block !== undefined) raiseQuoteRefused(block, t);
      else quoteTarget?.quote(selection);
    },
  };
  const selectAllItem = { label: t("viewer.selectAll"), icon: TextSelect, onClick: () => { selectingAll.current = true; } };
  return (
    <ActionMenu
      label={t("viewer.textActions")}
      className="hidden"
      contextOnly
      contextTargetRef={region}
      leavesContextToBrowser={(event) => kind !== "tauri" && event.target instanceof Element && event.target.closest(NATIVE_MENU_TARGET) !== null}
      onContextOpen={() => {
        // Opened again before it has gone, the menu drops a Select All still waiting for that close.
        selectingAll.current = false;
        quoting.current = null;
        const element = region.current;
        // A menu still closing keeps the selection it found, and offers its text again when opened anew: a browser may
        // have cleared the selection as focus entered the menu, or selected the word under the second right-click.
        if (!kept.current) {
          const found = element ? readSelection(element) : null;
          if (element && found) {
            kept.current = { ends: found.ends, text: source !== undefined && isWholeDocumentSelected(element) ? source : found.text };
          }
        }
        setSelection(kept.current?.text ?? "");
        setQuoteBlock(quoteTarget ? quoteTarget.blocked() : "input");
        // Focus on the surface, where the menu gives it back and the Select All key works from; a control inside
        // it that has focus (the Menu key on a link) keeps it, and gets it back.
        const active = element?.ownerDocument.activeElement ?? null;
        control.current = element && active !== element && element.contains(active) ? active : null;
        if (element && !element.contains(active)) element.focus({ preventScroll: true });
      }}
      onClosed={() => {
        const found = kept.current;
        kept.current = null;
        // Without an exit animation (reduced motion) this runs before the menu's own focus return, with focus
        // still on <body>, and WebKit clears a selection made while the surface is not focused, or as focus moves
        // again afterwards: focus goes where the menu would give it back, the control it was opened from or the
        // surface. Focus that is elsewhere (the control the menu gave it back to, or an editable element) is left where
        // it is.
        const element = region.current;
        const active = element?.ownerDocument.activeElement;
        if (element && (!active || active === element.ownerDocument.body)) {
          const back = control.current instanceof HTMLElement && control.current.isConnected ? control.current : element;
          back.focus({ preventScroll: true });
          // The control may not take it: a shadow host stands for a control inside its shadow root, and is not
          // focusable itself.
          if (element.ownerDocument.activeElement !== back) element.focus({ preventScroll: true });
        }
        control.current = null;
        const quoted = quoting.current;
        quoting.current = null;
        if (quoted !== null) {
          const block = quoteTarget ? quoteTarget.blocked() : "input";
          if (block === undefined && quoteTarget && closeSurface) {
            quoteTarget.quote(quoted);
            quoteTarget.revealInput(closeSurface);
            return;
          }
          // Blocked since the menu opened: nothing is written and the dialog stays, with the selection as for no action
          // at all, and the refusal is told.
          if (block !== undefined) raiseQuoteRefused(block, t);
        }
        if (selectingAll.current) {
          selectingAll.current = false;
          selectEverything();
          return;
        }
        if (found) restoreSelection(found.ends, region.current);
      }}
      items={selection ? [copyItem, ...(quoteTarget ? [quoteItem] : []), selectAllItem] : [selectAllItem]}
    />
  );
}
