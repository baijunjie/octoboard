import { ClipboardPaste, Copy, FolderOpen, SquareArrowOutUpRight, TextQuote, TextSelect } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { ActionMenu } from "../components/menus/ActionMenu";
import type { ActionMenuEntry } from "../components/menus/menuEntries";
import { copyText } from "../components/host/copyText";
import type { QuoteBlock } from "../components/host/quoteTarget";
import { raiseQuoteRefused } from "../components/host/raiseQuoteRefused";
import { quoteIntoComposer } from "../composer/composerDrafts";
import { composerTakesInput } from "../composer/composerInput";
import { useLatest, useStableHandler } from "../components/hooks/useLatest";
import { ActionableError } from "../errors/actionableError";
import { useT } from "../i18n/react";
import { useDaemon } from "../store";
import { usePlatform } from "../platform/react";
import type { PathLinkTarget } from "./links/pathLinks";
import type { TerminalController } from "./TerminalController";
import { raiseError } from "../components/alerts/raiseError";

/** Editing works on xterm's buffer and input pipeline, never on its hidden textarea's value. */
export function TerminalEditMenu({ controllerRef, contextTargetRef, sessionId, quote = false }: {
  controllerRef: React.RefObject<TerminalController | undefined>;
  contextTargetRef: React.RefObject<HTMLElement | null>;
  sessionId: string;
  /** Offers Quote, which writes the selection back into the input as a block quote: for a terminal whose program is
   * an agent, since a shell would run the quote's lines. While the session's composer is on (`composerTakesInput`),
   * the quote goes at the end of its text instead. */
  quote?: boolean;
}): React.ReactElement {
  const t = useT();
  const { clipboard, host } = usePlatform();
  const optionForcesSelection = (host.xtermSelectionModifier ?? (host.os === "macos" ? "alt" : "shift")) === "alt";
  // The host's file manager and applications are this machine's only where the daemon shares it.
  const { isLocal, store } = useDaemon();
  const currentSession = useLatest(sessionId);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  const [selection, setSelection] = useState("");
  const [mouseReporting, setMouseReporting] = useState(false);
  // The path link under the pointer when the menu opened, which the menu offers to open in the file manager or the default application.
  const [pathLink, setPathLink] = useState<PathLinkTarget>();
  const [inputSession, setInputSession] = useState<string>();
  const [quoteBlock, setQuoteBlock] = useState<QuoteBlock | undefined>("input");
  const paste = () => {
    const controller = controllerRef.current;
    const destination = inputSession;
    if (!clipboard?.readText || !controller || destination !== sessionId || controller.inputSessionId !== destination) return;
    void clipboard.readText().then(
      (text) => {
        // A permission prompt may outlive the session that asked for it.
        if (!mounted.current || currentSession.current !== destination || controllerRef.current !== controller) return;
        // No text: what a paste of an image is, which the controller turns into the image's path for a session whose
        // agent reads one, and into an empty paste for any other terminal or clipboard.
        if (text === "") controller.pasteImage(destination);
        else controller.paste(text, destination);
      },
      (cause: unknown) => raiseError(new ActionableError(t("terminal.pasteFailed"), "clipboardPaste", { cause }), { title: t("terminal.pasteFailedTitle") }),
    );
  };
  const revealLink = useStableHandler((link: PathLinkTarget) => void controllerRef.current?.revealPathLink(link));
  const openLinkInDefaultApplication = useStableHandler((link: PathLinkTarget) => void controllerRef.current?.openPathLinkInDefaultApplication(link));
  // Open in, as a row of the project pane has it: File manager, then for a file Default application, which a file the
  // system could launch rather than show is not offered.
  const openIn: ActionMenuEntry[] =
    pathLink && isLocal !== false
      ? [
        { label: t("common.fileManager"), icon: FolderOpen, onClick: () => revealLink(pathLink) },
        ...(pathLink.kind === "file" && !pathLink.viewerOnly
          ? [{ label: t("common.defaultApplication"), icon: SquareArrowOutUpRight, onClick: () => openLinkInDefaultApplication(pathLink) }]
          : []),
      ]
      : [];
  return (
    <ActionMenu
      label={t("terminal.actions")}
      contextOnly
      contextTargetRef={contextTargetRef}
      onContextOpen={({ clientX, clientY, byKey }) => {
        const controller = controllerRef.current;
        // The terminal's own selection first; failing that, what a program that selects for itself
        // (under mouse reporting) last copied.
        setSelection(controller?.selectedText || controller?.programCopiedText || "");
        // A program that selects and copies on its own may write the clipboard itself rather than through
        // OSC 52, leaving nothing here to copy: the item then says how to select in the terminal instead.
        setMouseReporting(controller?.mouseReporting ?? false);
        setInputSession(controller?.inputSessionId);
        // The session's composer, while it takes the input, takes a quote whatever the terminal could.
        setQuoteBlock(composerTakesInput(store, sessionId) ? undefined : controller ? controller.quoteBlock(sessionId) : "input");
        // Only a pointer-opened menu has a link under it: a key-opened one is anchored to the focused element.
        setPathLink(byKey ? undefined : controller?.pathLinkAt(clientX, clientY));
        controller?.focus();
      }}
      items={[
        ...(openIn.length > 0 ? [{ label: t("common.openIn"), icon: SquareArrowOutUpRight, items: openIn }, "separator" as const] : []),
        {
          label: t("terminal.copy"), icon: Copy, disabled: !selection,
          reason: !selection && mouseReporting ? { state: t(optionForcesSelection ? "terminal.copyHint.mac" : "terminal.copyHint") } : undefined,
          onClick: () => { void copyText(selection, { clipboard }).catch((cause: unknown) => raiseError(new ActionableError(t("common.copyFailedCause"), "clipboardTerminalCopy", { cause }), { title: t("terminal.copyFailed") })); },
        },
        {
          label: t("terminal.paste"), icon: ClipboardPaste, disabled: !clipboard?.readText || inputSession !== sessionId,
          // A session that cannot take input says why on the terminal itself (a spinner, the disconnected notice).
          reason: !clipboard?.readText ? { state: t("terminal.pasteNoClipboard") } : undefined,
          onClick: paste,
        },
        ...(quote
          ? [{
            label: t("common.quote"), icon: TextQuote, disabled: !selection || quoteBlock !== undefined,
            // A session that takes no input says why on the terminal itself, as for Paste; a program that would send
            // the quote line by line shows nothing of it.
            reason: selection && quoteBlock === "program" ? { state: t("common.quoteProgramBlocked") } : undefined,
            onClick: () => {
              // Into the composer while it takes the input, at the end of its text: focus stays where the menu leaves it.
              if (composerTakesInput(store, sessionId)) {
                quoteIntoComposer(store, sessionId, selection);
                return;
              }
              const controller = controllerRef.current;
              if (!controller) return;
              // Offered when the menu opened: the session may have stopped taking a quote since.
              const block = controller.quoteBlock(sessionId);
              if (block !== undefined) raiseQuoteRefused(block, t, { inTerminal: true });
              else controller.quote(selection, sessionId);
            },
          }]
          : []),
        { label: t("terminal.selectAll"), icon: TextSelect, onClick: () => controllerRef.current?.selectAll() },
      ]}
    />
  );
}
