import { ClipboardPaste, Copy, FolderOpen, TextSelect } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { ActionMenu } from "../components/menus/ActionMenu";
import { copyText } from "../components/host/copyText";
import { useLatest } from "../components/hooks/useLatest";
import { ActionableError } from "../errors/actionableError";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import type { PathLinkTarget } from "./links/pathLinks";
import { OPTION_FORCES_SELECTION, type TerminalController } from "./TerminalController";
import { raiseError } from "../components/alerts/raiseError";

/** Editing works on xterm's buffer and input pipeline, never on its hidden textarea's value. */
export function TerminalEditMenu({ controllerRef, contextTargetRef, sessionId }: {
  controllerRef: React.RefObject<TerminalController | undefined>;
  contextTargetRef: React.RefObject<HTMLElement | null>;
  sessionId: string;
}): React.ReactElement {
  const t = useT();
  const { clipboard } = usePlatform();
  const currentSession = useLatest(sessionId);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  const [selection, setSelection] = useState("");
  const [mouseReporting, setMouseReporting] = useState(false);
  // The path link under the pointer when the menu opened, which the menu offers to show in the file manager.
  const [pathLink, setPathLink] = useState<PathLinkTarget>();
  const [inputSession, setInputSession] = useState<string>();
  const paste = () => {
    const controller = controllerRef.current;
    const destination = inputSession;
    if (!clipboard?.readText || !controller || destination !== sessionId) return;
    void clipboard.readText().then(
      (text) => {
        // A permission prompt may outlive the session that asked for it.
        if (mounted.current && currentSession.current === destination && controllerRef.current === controller) controller.paste(text, destination);
      },
      (cause: unknown) => raiseError(new ActionableError(t("terminal.pasteFailed"), "clipboard", { cause }), { title: t("terminal.pasteFailedTitle") }),
    );
  };
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
        // Only a pointer-opened menu has a link under it: a key-opened one is anchored to the focused element.
        setPathLink(byKey ? undefined : controller?.pathLinkAt(clientX, clientY));
        controller?.focus();
      }}
      items={[
        ...(pathLink
          ? [{ label: t("viewer.openInFileManager"), icon: FolderOpen, onClick: () => { void controllerRef.current?.revealPathLink(pathLink); } }, "separator" as const]
          : []),
        {
          label: t("common.copy"), icon: Copy,
          items: [{
            label: t("terminal.copySelection"), icon: Copy, disabled: !selection,
            reason: !selection && mouseReporting ? { state: t(OPTION_FORCES_SELECTION ? "terminal.copyHint.mac" : "terminal.copyHint") } : undefined,
            onClick: () => { void copyText(selection, { clipboard }).catch((cause: unknown) => raiseError(new ActionableError(t("terminal.copyFailed"), "clipboard", { cause }), { title: t("common.copyFailedTitle") })); },
          }],
        },
        {
          label: t("terminal.paste"), icon: ClipboardPaste, disabled: !clipboard?.readText || inputSession !== sessionId,
          // A session that cannot take input says why on the terminal itself (a spinner, the disconnected notice).
          reason: !clipboard?.readText ? { state: t("terminal.pasteNoClipboard") } : undefined,
          onClick: paste,
        },
        { label: t("terminal.selectAll"), icon: TextSelect, onClick: () => controllerRef.current?.selectAll() },
      ]}
    />
  );
}
