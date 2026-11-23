import React, { useId } from "react";

import { DiagnosticBlock } from "../components/alerts/DiagnosticBlock";
import { TextDisclosure } from "../components/disclosure/TextDisclosure";
import { useT } from "../i18n/react";
import type { PlainMessageKey } from "../i18n/catalog";
import type { SessionRequestAction } from "../protocol";

/** What the requesting agent's text is called for each action that carries one. */
const LABELS: Partial<Record<SessionRequestAction, PlainMessageKey>> = {
  start: "dialog.sessionRequest.prompt.brief",
  handoff: "dialog.sessionRequest.prompt.note",
};

/** The text the requesting session wrote for a request of `action` (`SessionRequest.prompt`), in a `TextDisclosure`
 * folded away to begin with, under a label that says the session wrote it, for the confirmation of a request that
 * carries one. Opened, it shows the text as it was written, plain (never Markdown or links), whitespace kept and long
 * lines wrapped anywhere, in a block of bounded height that scrolls on its own, so a long text cannot stretch the
 * dialog (`DiagnosticBlock`, which also copies it whole, named by the trigger). The disclosure is not what the
 * dialog's focus starts on. */
export function RequestPrompt({ action, prompt }: { action: SessionRequestAction; prompt: string }): React.ReactElement {
  const t = useT();
  const label = t(LABELS[action] ?? "dialog.sessionRequest.prompt.prompt");
  const triggerId = useId();
  return (
    <TextDisclosure heading code id={triggerId} label={label}>
      <DiagnosticBlock dir="auto" wrap labelledBy={triggerId} failedText={t("dialog.sessionRequest.prompt.copyFailed")}>{prompt}</DiagnosticBlock>
    </TextDisclosure>
  );
}
