import React from "react";

import { ConfirmConsequences } from "../components/dialog/ConfirmConsequences";
import { ConfirmDialog } from "../components/dialog/ConfirmDialog";
import type { PlainMessageKey } from "../i18n/catalog";
import { useT } from "../i18n/react";
import type { Session } from "../protocol";
import type { QuitHolders, RestartDetail, UseAppExitResult } from "./useAppExit";

/** How many sessions a restart's confirmation names before it gathers the rest under "and N more". */
export const MAX_NAMED_INTERRUPTED = 5;

/** The confirmation's text: a message without parameters, or one that states how many sessions the restart interrupts
 * (`count`), for when none of them can be named. `unreachedInMessage` is whether the message already says the terminals
 * that cannot be reached may end, so that the dialog adds no line about them. */
export type ExitMessage =
  | { key: PlainMessageKey; unreachedInMessage: boolean }
  | { key: "app.restart.interrupts.counted" | "app.restart.interrupts.counted.both"; count: number; unreachedInMessage: false };

/** The confirmation's text for what a restart would interrupt or a quit would end. A restart goes by whether it can
 * name sessions (`named`) or only count them (`detail.unnamedSessions`), whether a terminal would end, whether the
 * daemon could prove any of it (`detail.uncertain`: what is said may be interrupted, not will be) and whether terminals
 * could not be reached; a quit, by what holds it up. */
export function exitMessage(restarts: boolean, holders: QuitHolders, named: number, detail: RestartDetail): ExitMessage {
  const message = (key: PlainMessageKey): ExitMessage => ({ key, unreachedInMessage: key === "app.restart.message.unreached" });
  if (!restarts) {
    if (!holders.terminals) return message("app.quit.message");
    return message(holders.sessions ? "app.quit.message.both" : "app.quit.message.terminals");
  }
  if (named === 0 && detail.unnamedSessions > 0) {
    return { key: holders.terminals ? "app.restart.interrupts.counted.both" : "app.restart.interrupts.counted", count: detail.unnamedSessions, unreachedInMessage: false };
  }
  if (named === 0) {
    if (!holders.terminals && detail.unreachedTerminals > 0) return message("app.restart.message.unreached");
    return message(detail.uncertain ? "app.restart.uncertain.terminals" : "app.restart.message.terminals");
  }
  if (detail.uncertain) return message(holders.terminals ? "app.restart.uncertain.both" : "app.restart.uncertain.sessions");
  return message(holders.terminals ? "app.restart.interrupts.both" : "app.restart.interrupts.sessions");
}

/** The sessions a restart would interrupt, by name (those it cannot name gathered with the rest under "and N more"),
 * the terminals that cannot be reached and may end too, and, where it can be said (`keepsRest`), that everything else
 * keeps running. */
function RestartInterruptions({ sessions, detail, keepsRest, unreachedInMessage }: { sessions: Session[]; detail: RestartDetail; keepsRest: boolean; unreachedInMessage: boolean }): React.ReactElement {
  const t = useT();
  const total = sessions.length + detail.unnamedSessions;
  const named = total > MAX_NAMED_INTERRUPTED ? sessions.slice(0, MAX_NAMED_INTERRUPTED - 1) : sessions;
  const more = total - named.length;
  const lines = [
    ...(detail.unreachedTerminals > 0 && !unreachedInMessage ? [{ key: "unreached", text: t("app.restart.interrupts.unreached") }] : []),
    ...(keepsRest ? [{ key: "rest", text: t("app.restart.interrupts.rest") }] : []),
  ];
  return (
    <ConfirmConsequences
      sessions={named.length > 0 ? named : undefined}
      sessionsMore={named.length > 0 && more > 0 ? t("app.restart.interrupts.more", { count: more }) : undefined}
      lines={lines}
    />
  );
}

/** The confirmation that sessions or terminals are still running, which `useAppExit` opens before a quit or a restart:
 * what it is about, and what the user's answer does. Nothing while it is closed. */
export function ExitConfirmDialog({ exit }: { exit: UseAppExitResult }): React.ReactElement | null {
  const t = useT();
  const { exitConfirmOpen, exitHolders, exitInterrupted, exitKeepsRest, exitRestartDetail, exitRestarts, closeExitConfirm, confirmExit } = exit;
  if (!exitConfirmOpen) return null;
  const message = exitMessage(exitRestarts, exitHolders, exitInterrupted.length, exitRestartDetail);
  return (
    <ConfirmDialog
      title={t(exitRestarts ? "app.restart.title" : "app.quit.title")}
      message={"count" in message ? t(message.key, { count: message.count }) : t(message.key)}
      consequences={
        exitRestarts ? (
          <RestartInterruptions sessions={exitInterrupted} detail={exitRestartDetail} keepsRest={exitKeepsRest} unreachedInMessage={message.unreachedInMessage} />
        ) : undefined
      }
      confirmLabel={t(exitRestarts ? "common.restart" : "common.quit")}
      destructive
      onCancel={closeExitConfirm}
      onConfirm={confirmExit}
      // A quit or restart that fails is reported by `useAppExit` under its own title; `confirmExit` never rejects.
      errorTitle={t(exitRestarts ? "exit.restartFailed.title" : "exit.failed.title")}
    />
  );
}
