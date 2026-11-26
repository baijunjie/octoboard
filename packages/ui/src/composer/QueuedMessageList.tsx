import { ArrowDown, ArrowUp, Clock, GripVertical, Hand, Paperclip, Pencil, Send, X } from "lucide-react";
import React, { useRef, useState } from "react";

import { useHandoffAfterRemoval } from "../components/focus/HandoffOnUnmount";
import { useStableHandler } from "../components/hooks/useLatest";
import { ICON_TRIGGER_CLASS } from "../components/controls/iconTrigger";
import { ActionMenu } from "../components/menus/ActionMenu";
import type { ActionMenuEntry } from "../components/menus/menuEntries";
import { releaseListMotion, useListMotion } from "../components/motion/useListMotion";
import { StatusBadge } from "../components/status/StatusBadge";
import { StatusMark } from "../components/status/StatusMark";
import { daemonMessage } from "../errors/daemonMessage";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { useCurrentLanguage, useT } from "../i18n/react";
import { Button, Spinner } from "../primitives";
import type { QueuedMessage, QueuedMessageFailure, QueuedMode } from "../protocol";
import { useDaemonStore } from "../store";
import type { ComposerContext } from "./composerActions";
import { moveQueued, sendQueuedNow, setQueuedMode, withdrawQueued } from "./composerActions";
import { messageExcerpt, queuedMode } from "./messageQueue";
import { beginRowDrag } from "./rowDrag";

/** Why a queued message came back, in the user's language: the daemon's notice or refusal as a line of its own. */
function FailureMark({ failure }: { failure: QueuedMessageFailure }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const line = useDaemonStore((s) => daemonMessage(language, failure.code, failure.params, failure.message, s, true) || failure.message);
  return <StatusMark inline label={t("queue.failureReason")} line={line} detail={failure.message === line ? undefined : failure.message} />;
}

/**
 * A session's composer queue, in queue order, each message with its text, attachments, mode and state, and what can be
 * done with it. In the composer (`full`) a message can also be dragged by its handle to another place (the menu's Move
 * earlier and Move later for every input), edited (`onEdit`, which `editing` marks) and have its mode changed; in the
 * compact list of a window with the composer off, it can be sent now or withdrawn. The order shown is the daemon's: a
 * drop shows the new order only until the daemon's queue changes, or its move is refused. Focus on a message that leaves
 * goes to the message now in its place, else the one before it, landing on its Withdraw (never on Send now, where a
 * repeated Enter would send it), else to `fallback`.
 */
export function QueuedMessageList({ session, messages, context, full = false, editing, editedMode, onEditedMode, onEdit, fallback, label }: {
  session: string;
  messages: readonly QueuedMessage[];
  context: ComposerContext;
  full?: boolean;
  editing?: string;
  /** The mode the message being edited is to wait in once its edit ends, which its menu shows and changes through
   * `onEditedMode` in place of the daemon's: while it is edited the message is held back as manual. */
  editedMode?: QueuedMode;
  onEditedMode?: (mode: QueuedMode) => void;
  onEdit?: (message: QueuedMessage) => void;
  /** Where focus goes when the last message holding it leaves. */
  fallback: () => HTMLElement | null | undefined;
  /** The list's accessible name. */
  label: string;
}): React.ReactElement {
  const listRef = useListMotion<HTMLUListElement>();
  const [elements] = useState(() => new Map<string, HTMLElement>());
  // The order a drop asked for, shown until the queue the daemon sends changes (or the move is refused).
  const [dropped, setDropped] = useState<{ order: string[]; from: readonly QueuedMessage[] } | null>(null);
  const shown = dropped && dropped.from === messages
    ? dropped.order.flatMap((id) => messages.filter((message) => message.id === id))
    : messages;
  const ids = shown.map((message) => message.id);
  // Where the message that last held focus stood, which the one taking its place is found by once it has gone.
  const focusedAt = useRef(0);
  const handoff = useHandoffAfterRemoval(() => {
    const rows = [...(listRef.current?.querySelectorAll<HTMLElement>("[data-queued]") ?? [])].filter((row) => row.isConnected && !row.hasAttribute("inert"));
    const row = rows[Math.min(focusedAt.current, rows.length - 1)];
    return row?.querySelector<HTMLElement>("[data-row-landing]") ?? fallback();
  });
  const dragStart = useStableHandler((id: string, event: PointerEvent) => {
    beginRowDrag({
      id,
      ids,
      elements,
      event,
      onDrop: (order, to, moved) => {
        const row = elements.get(moved);
        if (row) releaseListMotion(row);
        setDropped({ order, from: messages });
        void moveQueued(context, session, moved, to).then((taken) => {
          if (!taken) setDropped(null);
        });
      },
    });
  });
  return (
    <ul ref={listRef} aria-label={label} className="flex flex-col">
      {shown.map((message, index) => (
        <li
          key={message.id}
          data-list-motion={message.id}
          data-queued={message.id}
          ref={(element) => {
            if (element) elements.set(message.id, element);
            else elements.delete(message.id);
          }}
          className={`hairline-b [&:not(:has(~[data-list-motion]))]:border-b-0 ${full ? "bg-panel" : ""}`}
          onFocus={() => { focusedAt.current = index; }}
        >
          {handoff(`[data-queued="${CSS.escape(message.id)}"]`)}
          <QueuedMessageRow
            session={session}
            message={message}
            context={context}
            full={full}
            editing={editing === message.id}
            editedMode={editedMode}
            onEditedMode={onEditedMode}
            onEdit={onEdit}
            index={index}
            count={shown.length}
            onDragStart={full && shown.length > 1 ? dragStart : undefined}
          />
        </li>
      ))}
    </ul>
  );
}

function QueuedMessageRow({ session, message, context, full, editing, editedMode, onEditedMode, onEdit, index, count, onDragStart }: {
  session: string;
  message: QueuedMessage;
  context: ComposerContext;
  full: boolean;
  editing: boolean;
  editedMode?: QueuedMode;
  onEditedMode?: (mode: QueuedMode) => void;
  onEdit?: (message: QueuedMessage) => void;
  index: number;
  count: number;
  onDragStart?: (id: string, event: PointerEvent) => void;
}): React.ReactElement {
  const t = useT();
  const sending = message.state === "sending";
  // While the message is edited its menu shows and records the mode it is to wait in afterwards, sending nothing: the
  // daemon holds it as manual until the edit ends.
  const mode = editing && editedMode ? editedMode : queuedMode(message);
  const pickMode = (picked: QueuedMode) => (editing && onEditedMode ? onEditedMode(picked) : void setQueuedMode(context, session, message.id, picked));
  const excerpt = messageExcerpt(message);
  const attachments = message.images.length + message.files.length;
  const items = (): ActionMenuEntry[] => [
    { label: t("queue.edit"), icon: Pencil, disabled: sending || editing, onClick: () => onEdit?.(message) },
    "separator",
    { label: t("composer.mode.when_idle"), icon: Clock, selected: mode === "when_idle", disabled: sending, onClick: () => pickMode("when_idle") },
    { label: t("composer.mode.manual"), icon: Hand, selected: mode === "manual", disabled: sending, onClick: () => pickMode("manual") },
    "separator",
    { label: t("queue.moveEarlier"), icon: ArrowUp, disabled: index <= 0, onClick: () => void moveQueued(context, session, message.id, index - 1) },
    { label: t("queue.moveLater"), icon: ArrowDown, disabled: index >= count - 1, onClick: () => void moveQueued(context, session, message.id, index + 1) },
  ];
  return (
    <div className={`flex items-start gap-2 py-2 ps-1 pe-2 ${editing ? "bg-[var(--tint-accent-bg)]" : ""}`}>
      {onDragStart ? (
        // A handle for a mouse or pen; the menu's Move earlier and Move later are the way for every input.
        <span
          aria-hidden
          className="mt-0.5 flex size-6 shrink-0 cursor-grab items-center justify-center text-app-muted"
          onPointerDown={(event) => onDragStart(message.id, event.nativeEvent)}
        >
          <GripVertical className="size-4" />
        </span>
      ) : (
        <span aria-hidden className="w-1 shrink-0" />
      )}
      <div className="min-w-0 flex-1">
        {excerpt !== "" && (
          <FadeOverflow dir="auto" titleWhenClipped={excerpt} className={`text-sm ${message.command ? "font-mono" : ""}`}>{excerpt}</FadeOverflow>
        )}
        <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-app-muted">
          {sending ? (
            <StatusBadge variant="soft" color="accent" className="inline-flex items-center gap-1">
              <Spinner size="xs" color="current" />
              {t("queue.sending")}
            </StatusBadge>
          ) : (
            <StatusBadge variant="soft">{t(mode === "when_idle" ? "composer.mode.when_idle" : "composer.mode.manual")}</StatusBadge>
          )}
          {message.command && <StatusBadge>{t("queue.command")}</StatusBadge>}
          {attachments > 0 && (
            <span className="inline-flex items-center gap-1">
              <Paperclip className="size-3.5" />
              {t("queue.attachments", { count: attachments })}
            </span>
          )}
          {message.failure && (
            <span className="inline-flex items-center gap-1">
              <StatusBadge variant="soft" color="danger">{t(message.failure.kind === "refused" ? "queue.refused" : "queue.notDelivered")}</StatusBadge>
              <FailureMark failure={message.failure} />
            </span>
          )}
          {editing && <StatusBadge color="accent">{t("queue.editing")}</StatusBadge>}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        {/* The actions menu's trigger look (`ICON_TRIGGER_CLASS`), so the row's icon buttons read as one set. */}
        {/* Not while the message is being edited: what would go is not what the editor holds. */}
        <Button variant="ghost" className={ICON_TRIGGER_CLASS} label={t("queue.sendNow")} tooltip={t("queue.sendNow")} disabled={sending || editing} onPress={() => void sendQueuedNow(context, session, message.id)}>
          <Send className="size-4" />
        </Button>
        <Button data-row-landing="" variant="ghost" className={ICON_TRIGGER_CLASS} label={t("queue.withdraw")} tooltip={t("queue.withdraw")} onPress={() => void withdrawQueued(context, session, message.id)}>
          <X className="size-4" />
        </Button>
        {full && <ActionMenu label={t("queue.actions")} items={items} />}
      </div>
    </div>
  );
}
