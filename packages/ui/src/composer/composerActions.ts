import { raiseError } from "../components/alerts/raiseError";
import { CodedError } from "../errors/codedError";
import { CODE } from "../errors/codes";
import { basename, type PathStyle } from "../format/hostPath";
import type { Translate } from "../i18n/catalog";
import type { QueuedMessage, QueuedMode, SendMode } from "../protocol";
import type { RequestSender } from "../requests";
import type { Daemon } from "../store";
import { isSavedAsIs } from "../terminal/pastedImage";
import type { DroppedFile } from "../terminal/fileDrag";
import { uploadFiles, type UploadContext } from "../terminal/droppedFiles";
import { composerDraft, setComposerDraft, updateComposerDraft, type ComposerAttachment, type ComposerDraft } from "./composerDrafts";
import { isLoneSlashCommand, queuedMode } from "./messageQueue";

/** What the composer's actions need of the window: the daemon's store and requests, and the language to word a
 * failure in. */
export interface ComposerContext {
  store: Daemon["store"];
  request: RequestSender;
  t: Translate;
}

let attachmentCount = 0;

function nextAttachmentId(): string {
  attachmentCount += 1;
  return `attachment-${attachmentCount}`;
}

/** The text a draft's message carries: leading blank lines and trailing whitespace dropped, indentation kept. */
export function messageText(text: string): string {
  return text.replace(/^(?:[^\S\n]*\n)+/, "").trimEnd();
}

/** The most images one message carries: the daemon refuses more (`MAX_IMAGES` in
 * `apps/daemon/src/coordinator/messages.rs`), and the composer holds a message with more back from being sent. */
export const MAX_MESSAGE_IMAGES = 10;

/** How many images `draft`'s message would carry. */
export function draftImageCount(draft: ComposerDraft): number {
  return draft.attachments.filter((attachment) => attachment.image).length;
}

/** Whether `draft` holds anything to send, nothing still being sent to the daemon, and no more images than a message
 * takes. */
export function canSendDraft(draft: ComposerDraft): boolean {
  if (draft.attachments.some((attachment) => attachment.path === undefined)) return false;
  if (draftImageCount(draft) > MAX_MESSAGE_IMAGES) return false;
  return messageText(draft.text) !== "" || draft.attachments.length > 0;
}

/** The fields of the queued message `draft` stands for: its images and files apart, and whether it is a lone slash
 * command, which goes as a command where the session's agent takes slash commands (`slashCommands`) and as text
 * otherwise. */
export function messageOfDraft(draft: ComposerDraft, slashCommands: boolean): { text: string; images: string[]; files: string[]; command: boolean } {
  const images = draft.attachments.filter((attachment) => attachment.image).map((attachment) => attachment.path!);
  const files = draft.attachments.filter((attachment) => !attachment.image).map((attachment) => attachment.path!);
  const text = messageText(draft.text);
  const command = slashCommands && images.length === 0 && files.length === 0 && isLoneSlashCommand(text);
  return { text: command ? text.trim() : text, images, files, command };
}

/** The text left once `sent` has gone out of `current`: none where it is unchanged; what was added after it where it
 * only grew while the message was out (the field is read-only then, so only Quote and Add as context write into it, and
 * they append), less the one line break that set the addition apart; all of it where it was changed otherwise. */
function textLeftAfter(current: string, sent: string): string {
  if (current === sent) return "";
  if (!current.startsWith(sent)) return current;
  const added = current.slice(sent.length);
  return added.startsWith("\n") ? added.slice(1) : added;
}

/** What `current` holds once the message sent from `sent` has been taken: the text added since, and the attachments
 * added since; what was typed or attached while the message was out is never dropped, and what went is never sent
 * twice. */
function leftAfterSending(current: { text: string; attachments: readonly ComposerAttachment[] }, sent: ComposerDraft): { text: string; attachments: readonly ComposerAttachment[] } {
  const sentIds = new Set(sent.attachments.map((attachment) => attachment.id));
  return {
    text: textLeftAfter(current.text, sent.text),
    attachments: current.attachments.filter((attachment) => !sentIds.has(attachment.id)),
  };
}

/** Puts the message `session`'s draft holds on its queue in `mode` (sent at once for `now`; with none, the daemon's
 * default for the session), and takes what was sent out of the draft once the daemon has taken it; resolves to whether
 * it did. A refusal keeps the draft, so nothing typed is lost, and raises its alert. */
export async function sendDraft({ store, request, t }: ComposerContext, session: string, mode: SendMode | undefined, slashCommands: boolean): Promise<boolean> {
  const draft = composerDraft(store, session);
  if (!canSendDraft(draft) || draft.editing) return false;
  const { text, images, files, command } = messageOfDraft(draft, slashCommands);
  try {
    await request({
      type: "create_queued_message",
      session,
      text,
      ...(images.length > 0 && { images }),
      ...(files.length > 0 && { files }),
      ...(command && { command }),
      ...(mode !== undefined && { mode }),
    });
  } catch (error) {
    raiseError(error, { title: t("composer.sendFailedTitle"), session });
    return false;
  }
  // Only what was sent leaves: what was typed or attached while the message was out stays (in the message set aside,
  // where an edit began meanwhile).
  updateComposerDraft(store, session, (current) => (current.editing
    ? { ...current, editing: { ...current.editing, set: leftAfterSending(current.editing.set, draft) } }
    : { ...current, ...leftAfterSending(current, draft) }));
  return true;
}

/** Starts editing the queued `message` in `session`'s composer: its text and attachments take the editor's place, and the
 * new message being written there is kept aside until the edit ends. A message waiting to go once the agent is idle is
 * held back as manual while it is edited, so the daemon does not send it half-changed; its mode comes back when the edit
 * ends (`endEdit`, `saveEdit`). */
export function editQueuedMessage(context: ComposerContext, session: string, message: QueuedMessage, style: PathStyle): void {
  const { store } = context;
  const current = composerDraft(store, session);
  const set = current.editing?.set ?? { text: current.text, attachments: current.attachments };
  // Another message whose edit this one replaces gets its mode back first.
  restoreMode(context, session, current);
  const attachment = (path: string, image: boolean): ComposerAttachment => ({ id: nextAttachmentId(), name: basename(path, style), image, path });
  const holdBack = queuedMode(message) === "when_idle";
  setComposerDraft(store, session, {
    text: message.text,
    attachments: [...message.images.map((path) => attachment(path, true)), ...message.files.map((path) => attachment(path, false))],
    editing: { message: message.id, images: message.images, files: message.files, set, ...(holdBack && { restoreMode: "when_idle" as const }) },
  });
  if (holdBack) void setQueuedMode(context, session, message.id, "manual", { ignoreGone: true });
}

/** Puts back the mode the message `draft` is editing is to wait in once the edit ends: the one it waited in before the
 * edit held it back, or the one chosen for it during the edit (`chooseEditedMode`); nothing for one that stays manual. */
function restoreMode(context: ComposerContext, session: string, draft: ComposerDraft): void {
  if (draft.editing?.restoreMode) void setQueuedMode(context, session, draft.editing.message, draft.editing.restoreMode, { ignoreGone: true });
}

/** Records `mode` as the one the message being edited in `session`'s composer waits in once the edit ends, sending
 * nothing now: the message stays held back as manual while it is edited, so the daemon does not send the old text, and
 * the end of the edit applies the choice. */
export function chooseEditedMode(store: Daemon["store"], session: string, mode: QueuedMode): void {
  updateComposerDraft(store, session, (draft) => {
    if (!draft.editing) return draft;
    const editing = { ...draft.editing };
    if (mode === "when_idle") editing.restoreMode = "when_idle";
    else delete editing.restoreMode;
    return { ...draft, editing };
  });
}

/** The ids of the sessions whose composer has a queued message being edited. */
export function sessionsEditing(store: Daemon["store"]): string[] {
  return [...store.getState().sessions.keys()].filter((session) => composerDraft(store, session).editing !== undefined);
}

/** Ends the edit under way in `session`'s composer, bringing back the new message that was being written before it and
 * the edited message's mode. */
export function endEdit(context: ComposerContext, session: string): void {
  const draft = composerDraft(context.store, session);
  if (!draft.editing) return;
  restoreMode(context, session, draft);
  setComposerDraft(context.store, session, { text: draft.editing.set.text, attachments: draft.editing.set.attachments });
}

/** Leaves the edit of a queued message that has left the queue (sent, withdrawn or dropped meanwhile): what the editor
 * holds stays, as a new message the user can still send, and the message set aside before the edit is dropped only
 * where it held nothing. */
export function keepEditAsNew(store: Daemon["store"], session: string): void {
  updateComposerDraft(store, session, (draft) => {
    if (!draft.editing) return draft;
    const { set } = draft.editing;
    const text = set.text === "" ? draft.text : `${set.text}${set.text.endsWith("\n") ? "" : "\n"}${draft.text}`;
    return { text, attachments: [...set.attachments, ...draft.attachments] };
  });
}

/** Saves the edit under way in `session`'s composer to its queued message, sending the lists only where they changed,
 * and ends the edit once the daemon has taken it, putting back the message's mode where the edit held it back; resolves
 * to whether it did. What was typed or attached while the save was out stays in the editor, after the message set aside
 * before the edit. A refusal keeps the edit and raises its alert. */
export async function saveEdit(context: ComposerContext, session: string, slashCommands: boolean): Promise<boolean> {
  const { store, request, t } = context;
  const draft = composerDraft(store, session);
  if (!draft.editing || !canSendDraft(draft)) return false;
  const { text, images, files, command } = messageOfDraft(draft, slashCommands);
  const same = (a: readonly string[], b: readonly string[]) => a.length === b.length && a.every((path, i) => path === b[i]);
  try {
    await request({
      type: "update_queued_message",
      session,
      message: draft.editing.message,
      text,
      command,
      ...(!same(images, draft.editing.images) && { images }),
      ...(!same(files, draft.editing.files) && { files }),
    });
  } catch (error) {
    raiseError(error, { title: t("queue.changeFailedTitle"), session });
    return false;
  }
  const current = composerDraft(store, session);
  if (current.editing?.message !== draft.editing.message) return true;
  restoreMode(context, session, current);
  const { set } = current.editing;
  const left = leftAfterSending(current, draft);
  const joined = left.text === "" ? set.text : set.text === "" ? left.text : `${set.text}${set.text.endsWith("\n") ? "" : "\n"}${left.text}`;
  setComposerDraft(store, session, { text: joined, attachments: [...set.attachments, ...left.attachments] });
  return true;
}

/** Adds files the platform gave as absolute paths on this computer (a drop in the desktop application's own window, whose
 * daemon runs on the same computer) to `session`'s composer, each as a file the message names. */
export function attachLocalPaths(store: Daemon["store"], session: string, paths: readonly string[], style: PathStyle): void {
  if (paths.length === 0) return;
  updateComposerDraft(store, session, (draft) => ({
    ...draft,
    attachments: [...draft.attachments, ...paths.map((path) => ({ id: nextAttachmentId(), name: basename(path, style), image: false, path }))],
  }));
}

/** Adds files the page read (picked, pasted or dropped) to `session`'s composer, sending each to the daemon as a pasted
 * or dropped file goes (`uploadFiles`): an image the agent takes as one is saved as an image of the message, anything
 * else attached as a file. Each shows at once, waiting for its path; one refused or cancelled leaves again, its failure
 * told as a drop's is. One removed meanwhile stops the files after it. */
export function attachFiles(store: Daemon["store"], session: string, files: DroppedFile[], latest: UploadContext): void {
  if (files.length === 0) return;
  const { takesImages } = latest();
  const takes = takesImages(session);
  const placed = new Map(files.map((dropped) => [dropped, { id: nextAttachmentId(), name: dropped.file.name, image: takes && isSavedAsIs(dropped.file) }] as const));
  const ids = new Set([...placed.values()].map((attachment) => attachment.id));
  updateComposerDraft(store, session, (draft) => ({ ...draft, attachments: [...draft.attachments, ...placed.values()] }));
  const stillWanted = () => composerDraft(store, session).attachments.some((attachment) => ids.has(attachment.id));
  void uploadFiles(files, session, latest, {
    wanted: stillWanted,
    onUploaded: ({ file, path, image }) => {
      const id = placed.get(file)?.id;
      updateComposerDraft(store, session, (draft) => ({
        ...draft,
        attachments: draft.attachments.map((attachment) => (attachment.id === id ? { ...attachment, path, image } : attachment)),
      }));
    },
  }).finally(() => {
    // What did not arrive (refused, cancelled, or no longer wanted) leaves the composer.
    updateComposerDraft(store, session, (draft) => {
      const kept = draft.attachments.filter((attachment) => !ids.has(attachment.id) || attachment.path !== undefined);
      return kept.length === draft.attachments.length ? draft : { ...draft, attachments: kept };
    });
  });
}

/** Takes the attachment `id` off `session`'s composer. */
export function removeAttachment(store: Daemon["store"], session: string, id: string): void {
  updateComposerDraft(store, session, (draft) => ({ ...draft, attachments: draft.attachments.filter((attachment) => attachment.id !== id) }));
}

/** Sets `session`'s default send mode, which the composer's selector shows. */
export function setSendMode({ request, t }: ComposerContext, session: string, mode: SendMode): Promise<void> {
  return request({ type: "set_session_send_mode", session, mode }).then(
    () => {},
    (error: unknown) => raiseError(error, { title: t("composer.modeFailedTitle"), session }),
  );
}

/** Sends the queued `message` now, whatever its mode. */
export function sendQueuedNow({ request, t }: ComposerContext, session: string, message: string): Promise<void> {
  return request({ type: "send_queued_message", session, message }).then(
    () => {},
    (error: unknown) => raiseError(error, { title: t("composer.sendFailedTitle"), session }),
  );
}

/** Takes the queued `message` off the queue. */
export function withdrawQueued({ request, t }: ComposerContext, session: string, message: string): Promise<void> {
  return request({ type: "withdraw_queued_message", session, message }).then(
    () => {},
    (error: unknown) => raiseError(error, { title: t("queue.withdrawFailedTitle"), session }),
  );
}

/** Sets the mode the queued `message` waits in. With `ignoreGone`, for an edit holding a message back or putting its mode
 * back, a message the daemon has sent or dropped meanwhile (the agent went idle just as the edit began) has no mode left
 * to change, so that refusal is not told; any other is. */
export function setQueuedMode({ request, t }: ComposerContext, session: string, message: string, mode: QueuedMode, { ignoreGone = false }: { ignoreGone?: boolean } = {}): Promise<void> {
  return request({ type: "set_queued_message_mode", session, message, mode }).then(
    () => {},
    (error: unknown) => {
      if (ignoreGone && error instanceof CodedError && error.code === CODE.queued_message_not_found) return;
      raiseError(error, { title: t("queue.changeFailedTitle"), session });
    },
  );
}

/** Moves the queued `message` to `position` among the others; resolves to whether the daemon took the move (a refusal
 * raises its alert). */
export function moveQueued({ request, t }: ComposerContext, session: string, message: string, position: number): Promise<boolean> {
  return request({ type: "move_queued_message", session, message, position }).then(
    () => true,
    (error: unknown) => {
      raiseError(error, { title: t("queue.changeFailedTitle"), session });
      return false;
    },
  );
}
