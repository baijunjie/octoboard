import { Checkbox, Description, Label, ProgressBar } from "@heroui/react";
import React, { useRef, useState } from "react";

import { AGENT_LABEL } from "../agents";
import { InlineChip } from "../components/InlineChip";
import { DaemonErrorMessage } from "../components/DaemonErrorMessage";
import { ErrorAlert } from "../components/ErrorAlert";
import { useListMotion } from "../components/useListMotion";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { errorPresentation, type ErrorPresentation } from "../errorPresentation";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { Session } from "../protocol";
import { sessionDisplayTitle } from "../sessionLabel";
import { useDaemon, useDaemonStore } from "../store";
import { ConfirmDialog } from "./ConfirmDialog";

interface SwitchFailure {
  session: Session;
  error: ErrorPresentation;
}

export function SwitchAccountDialog({
  session,
  account,
  accountName,
  onClose,
  onSwitchAccount,
}: {
  session: Session;
  account: string | null;
  accountName: string;
  onClose: () => void;
  onSwitchAccount: (sessionId: string, account: string | null) => Promise<void>;
}): React.ReactElement {
  const t = useT();
  const { store } = useDaemon();
  const sessions = useDaemonStore((s) => s.sessions);
  const [all, setAll] = useState(false);
  const [snapshot, setSnapshot] = useState<Session[]>();
  const [progress, setProgress] = useState<{ completed: number; total: number; current?: Session }>();
  const [failures, setFailures] = useState<SwitchFailure[]>([]);
  const failuresRef = useListMotion<HTMLUListElement>();
  const completed = useRef(new Set<string>());
  const pending = useRef<Promise<void> | undefined>(undefined);
  const eligible = (candidate: Session) => candidate.console_id === session.console_id
    && candidate.agent === session.agent && candidate.status !== "archived"
    && (candidate.account_id ?? null) !== account;
  const candidates = Array.from(sessions.values()).filter(eligible);
  const progressText = progress && t("dialog.switchAccount.progress", { count: progress.total, completed: progress.completed });
  const currentText = progress?.current && <Message id="dialog.switchAccount.current" params={{ title: <InlineChip dir="auto" wrap size="inherit">{sessionDisplayTitle(t, progress.current)}</InlineChip> }} />;
  const announcement = progress?.current
    ? t("dialog.switchAccount.progressCurrent", { count: progress.total, completed: progress.completed, title: isolate(sessionDisplayTitle(t, progress.current)) })
    : progressText;

  const switchSessions = async () => {
    // Freeze the consented scope: sessions opened during the batch never join it, and retries
    // never relaunch a success even before its daemon event has arrived.
    const targets = snapshot ?? (all ? candidates : [sessions.get(session.id) ?? session].filter(eligible));
    setSnapshot(targets);
    setFailures([]);
    const failed: SwitchFailure[] = [];
    let processed = completed.current.size;
    setProgress({ completed: processed, total: targets.length });
    for (const target of targets) {
      if (completed.current.has(target.id)) continue;
      const latest = store.getState().sessions.get(target.id);
      if (latest && eligible(latest)) {
        setProgress({ completed: processed, total: targets.length, current: latest });
        try {
          await onSwitchAccount(target.id, account);
          completed.current.add(target.id);
        } catch (error) {
          failed.push({ session: latest, error: errorPresentation(error as Error) });
        }
      } else {
        completed.current.add(target.id);
      }
      processed += 1;
      setProgress({ completed: processed, total: targets.length });
    }
    setFailures(failed);
    if (failed.length === 0) onClose();
  };

  return (
    <ConfirmDialog
      title={<Message id="dialog.switchAccount.title" params={{ title: <InlineChip dir="auto" wrap size="inherit">{sessionDisplayTitle(t, session)}</InlineChip>, account: <InlineChip dir="auto" wrap size="inherit">{accountName}</InlineChip> }} />}
      message={t(all ? "dialog.switchAccount.messageAll" : session.has_conversation ? "dialog.switchAccount.message" : "dialog.switchAccount.messageFresh")}
      confirmLabel={t(failures.length ? "connection.retry" : "dialog.switchAccount.confirm")}
      pendingLabel={t("dialog.switchAccount.pending")}
      onCancel={onClose}
      onConfirm={() => {
        pending.current ??= switchSessions().finally(() => { pending.current = undefined; });
        return pending.current;
      }}
      extraControl={
        <>
          <Checkbox variant="secondary" isSelected={all} onChange={setAll} isReadOnly={snapshot !== undefined}>
            <Checkbox.Content className="min-h-6">
              <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
              <Label>{t("dialog.switchAccount.all", { agent: AGENT_LABEL[session.agent] })}</Label>
            </Checkbox.Content>
            <Description>{t("dialog.switchAccount.scope", { count: snapshot?.length ?? candidates.length })}</Description>
          </Checkbox>
          <StatusAnnouncer text={announcement} />
          {progress && progress.total > 0 && (
            <div className="flex min-w-0 flex-col gap-2 text-sm">
              <ProgressBar value={progress.completed} maxValue={progress.total} aria-label={t("dialog.switchAccount.progressLabel")} aria-valuetext={progressText}>
                <Label>{progressText}</Label>
                <ProgressBar.Track><ProgressBar.Fill /></ProgressBar.Track>
              </ProgressBar>
              {currentText && <p className="[overflow-wrap:anywhere]">{currentText}</p>}
            </div>
          )}
          {failures.length > 0 && (
            <ErrorAlert title={t("dialog.switchAccount.failed", { count: failures.length })}>
              <ul ref={failuresRef} className="relative flex flex-col gap-2">
                {failures.map((failure) => (
                  <li key={failure.session.id} data-list-motion={failure.session.id}>
                    <p dir="auto" className="font-semibold">{sessionDisplayTitle(t, sessions.get(failure.session.id) ?? failure.session)}</p>
                    <DaemonErrorMessage error={failure.error} />
                  </li>
                ))}
              </ul>
            </ErrorAlert>
          )}
        </>
      }
    />
  );
}
