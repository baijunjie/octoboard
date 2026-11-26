import type { QueuedMessage, QueuedMode, SendMode } from "../protocol";
import { useDaemonStore } from "../store";
import { knownValues } from "../values/knownValue";

const NO_MESSAGES: readonly QueuedMessage[] = [];

/** The send modes the composer offers, in the order its selector lists them. */
export const SEND_MODES = ["now", "when_idle", "manual"] as const satisfies readonly SendMode[];

/** Whether `raw`, a mode the daemon sent, is one this client knows. */
export const isKnownSendMode = knownValues<SendMode>({ now: true, when_idle: true, manual: true });

/** The mode a queued message waits in as this client reads it: a mode a newer daemon added reads as `manual`, the one
 * that never sends by itself. */
export function queuedMode(message: Pick<QueuedMessage, "mode">): QueuedMode {
  return message.mode === "when_idle" ? "when_idle" : "manual";
}

/** A session's default send mode as this client reads it (`Session.send_mode`); `undefined` for one it does not know (a
 * newer daemon's), which the composer shows as not chosen and leaves to the daemon by sending no mode. */
export function sendModeOf(raw: string | undefined): SendMode | undefined {
  return raw !== undefined && isKnownSendMode(raw) ? raw : undefined;
}

/** `session`'s composer queue in queue order, as the daemon last sent it; empty with nothing queued. */
export function useMessageQueue(session: string | undefined): readonly QueuedMessage[] {
  return useDaemonStore((s) => (session === undefined ? NO_MESSAGES : (s.messageQueues?.get(session) ?? NO_MESSAGES)));
}

/** Whether the daemon keeps a composer queue at all: one that predates it sends none, and the window then offers none. */
export function useQueueOffered(): boolean {
  return useDaemonStore((s) => s.messageQueues !== null);
}

/** Whether `text` alone is one slash command for the agent to run (`/name` and what follows it, on one line), which the
 * composer sends as a command rather than as a message. */
export function isLoneSlashCommand(text: string): boolean {
  // The command's name is everything up to the first space; a second `/` in it makes it a path (`/usr/bin/env`).
  return /^\/[^\s/]+(?:[^\S\n][^\n]*)?$/.test(text.trim());
}

/** A queued message's text as one line, for a row or a list that shows only its start. */
export function messageExcerpt(message: Pick<QueuedMessage, "text">): string {
  return message.text.replace(/\s+/g, " ").trim();
}
