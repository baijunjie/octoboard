import { ClipboardPaste, Copy, TextSelect } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { ActionMenu } from "../components/ActionMenu";
import { copyText } from "../components/copyText";
import { useLatest } from "../components/useLatest";
import { ActionableError } from "../errorRecovery";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { useDaemon } from "../store";
import { OPTION_FORCES_SELECTION, type TerminalController } from "./TerminalController";

/** Editing works on xterm's buffer and input pipeline, never on its hidden textarea's value. */
export function TerminalEditMenu({ controllerRef, contextTargetRef, sessionId }: {
  controllerRef: React.RefObject<TerminalController | undefined>;
  contextTargetRef: React.RefObject<HTMLElement | null>;
  sessionId: string;
}): React.ReactElement {
  const t = useT();
  const { clipboard } = usePlatform();
  const { toastError } = useDaemon();
  const currentSession = useLatest(sessionId);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  const [selection, setSelection] = useState("");
  const [mouseReporting, setMouseReporting] = useState(false);
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
      (cause: unknown) => toastError(new ActionableError(t("terminal.pasteFailed"), "clipboard", { cause }), undefined, t("terminal.pasteFailedTitle")),
    );
  };
  return (
    <ActionMenu
      label={t("terminal.actions")}
      contextOnly
      contextTargetRef={contextTargetRef}
      onContextOpen={() => {
        const controller = controllerRef.current;
        // The terminal's own selection first; failing that, what a program that selects for itself
        // (under mouse reporting) last copied.
        setSelection(controller?.selectedText || controller?.programCopiedText || "");
        // A program that selects and copies on its own may write the clipboard itself rather than through
        // OSC 52, leaving nothing here to copy: the item then says how to select in the terminal instead.
        setMouseReporting(controller?.mouseReporting ?? false);
        setInputSession(controller?.inputSessionId);
        controller?.focus();
      }}
      items={[
        {
          label: t("terminal.copy"), icon: Copy, disabled: !selection,
          reason: !selection && mouseReporting ? { state: t(OPTION_FORCES_SELECTION ? "terminal.copyHint.mac" : "terminal.copyHint") } : undefined,
          onClick: () => { void copyText(selection, { clipboard }).catch((cause: unknown) => toastError(new ActionableError(t("terminal.copyFailed"), "clipboard", { cause }), undefined, t("common.copyFailedTitle"))); },
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
