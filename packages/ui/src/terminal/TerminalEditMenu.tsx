import { ClipboardPaste, Copy, TextSelect } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { ActionMenu } from "../components/ActionMenu";
import { useLatest } from "../components/useLatest";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { useDaemon } from "../store";
import type { TerminalController } from "./TerminalController";

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
      () => toastError(t("terminal.pasteFailed")),
    );
  };
  return (
    <ActionMenu
      label={t("terminal.actions")}
      contextOnly
      contextTargetRef={contextTargetRef}
      onContextOpen={() => {
        const controller = controllerRef.current;
        setSelection(controller?.selectedText ?? "");
        setInputSession(controller?.inputSessionId);
        controller?.focus();
      }}
      items={[
        {
          label: t("terminal.copy"), icon: Copy, disabled: !clipboard || !selection,
          onClick: () => { void clipboard?.writeText(selection).catch(() => toastError(t("terminal.copyFailed"))); },
        },
        { label: t("terminal.paste"), icon: ClipboardPaste, disabled: !clipboard?.readText || inputSession !== sessionId, onClick: paste },
        { label: t("terminal.selectAll"), icon: TextSelect, onClick: () => controllerRef.current?.selectAll() },
      ]}
    />
  );
}
