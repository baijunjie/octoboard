import { Info, Paperclip } from "lucide-react";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";

import { takesMessageImages, takesSlashCommands } from "../agents/providers";
import { useFocusLeavingWhenDisabled } from "../components/dialog/dialogHooks";
import { handFocusOff } from "../components/focus/handFocusOff";
import { OptionSelect, type Option } from "../components/form/OptionSelect";
import { useHostPaths } from "../components/hooks/useHostPaths";
import { useLatest } from "../components/hooks/useLatest";
import { StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { FadedScroller } from "../components/scroll/scrollFade";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { StatusBadge } from "../components/status/StatusBadge";
import { useT } from "../i18n/react";
import { isImeKey } from "../input/imeKey";
import { isPrimaryModifier } from "../input/keys";
import { usePlatform } from "../platform/react";
import { useAnnouncedOnRise } from "../terminal/useAnnouncedOnRise";
import { Button, liveAnnouncerProps, Spinner, Tag, TagGroup, TagList, TagRemoveButton, TextArea, TextField } from "../primitives";
import type { SendMode, Session } from "../protocol";
import { hasNoProcess } from "../sessions/sessionStatus";
import { BindingKbd } from "../shortcuts/ShortcutKbd";
import { useDaemon, useProviders } from "../store";
import type { DroppedFile } from "../terminal/fileDrag";
import type { UploadContext } from "../terminal/droppedFiles";
import { FILE_DROP_TARGET_CLASS, watchFileDrops } from "../terminal/watchFileDrops";
import type { ViewedFile } from "../viewer/viewedFile";
import {
  attachFiles,
  attachLocalPaths,
  canSendDraft,
  chooseEditedMode,
  draftImageCount,
  editQueuedMessage,
  MAX_MESSAGE_IMAGES,
  endEdit,
  keepEditAsNew,
  removeAttachment,
  saveEdit,
  sendDraft,
  setSendMode,
  type ComposerContext,
} from "./composerActions";
import { registerComposerEditor, updateComposerDraft, useComposerDraft } from "./composerDrafts";
import { composerOffered } from "./composerInput";
import { MentionList, mentionOptionId } from "./MentionPicker";
import { QueuedMessageList } from "./QueuedMessageList";
import { sessionComposerEnabled } from "./composerSetting";
import { SEND_MODES, sendModeOf, useMessageQueue, useQueueOffered } from "./messageQueue";
import { useComposerMention } from "./useComposerMention";

/** The key that sends: ⌘ Enter on a Mac, Ctrl Enter elsewhere. */
const SEND_BINDING = { code: "Enter", primary: true } as const;

/**
 * The message composer below the selected session's terminal (while `sessionComposerEnabled` is on): the session's
 * queue of unsent messages above a multi-line editor, the files attached to the message, and its controls. ⌘ Enter
 * (Ctrl Enter elsewhere) sends, Enter starts a new line, and a key an input method is composing with never sends. The
 * mode selector shows and sets the session's default send mode (`Session.send_mode`); the send button sends the message
 * by it, at once or onto the queue. A lone slash command goes as a command. Files are attached by the button, by
 * pasting and by dropping them on the composer: in the desktop application's own window a dropped file is given by its
 * own path, elsewhere it is sent to the daemon, an image as one where the agent takes images. The text being written
 * is this window's alone, kept per session as the user moves between sessions (`composerDrafts`); the queue shown is
 * always the daemon's. A queued message is edited in the editor itself, the new message being written set aside until
 * the edit ends. The terminal still takes typing: the composer only adds a place to write.
 */
export function SessionComposer({ session, onViewFile, exposed = true }: {
  session: Session;
  /** False while the composer is out of reach (inert behind the archive view or a panel over the agent area, or hidden
   * behind a page): a sentence it announces is held until it is exposed again. */
  exposed?: boolean;
  /** Shows a file in the window's file viewer (`useLinkedFile`'s `viewLinkedFile`), focus going back to `returnFocus`'s
   * element as it closes; an attachment opens there. */
  onViewFile?: (file: ViewedFile, returnFocus?: () => boolean) => void;
}): React.ReactElement {
  const t = useT();
  const daemon = useDaemon();
  const { store, request } = daemon;
  const context: ComposerContext = { store, request, t };
  const platform = usePlatform();
  const { host, fileDrop } = platform;
  const { style } = useHostPaths();
  const providers = useProviders();
  const sessionId = session.id;
  const draft = useComposerDraft(store, sessionId);
  const messages = useMessageQueue(sessionId);
  const mode = sendModeOf(session.send_mode);
  const takesImages = takesMessageImages(providers, session.agent);
  const slashCommands = takesSlashCommands(providers, session.agent);
  const editing = draft.editing;
  const [sending, setSending] = useState(false);
  const rootRef = useRef<HTMLElement>(null);
  const fieldRef = useRef<HTMLTextAreaElement>(null);
  const pickerRef = useRef<HTMLInputElement>(null);
  const sendRef = useRef<HTMLButtonElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  // Where the caret goes once text inserted at it is in the editor.
  const caretAfter = useRef<number | null>(null);
  const uploads: UploadContext = () => ({ t, request, attachmentLimits: store.getState().attachmentLimits, takesImages: () => takesImages });
  // The `@` mention being typed and its picker; a picked option replaces it, the caret going after it.
  const mention = useComposerMention({
    session,
    fieldRef,
    sending,
    apply: (text, caret) => {
      caretAfter.current = caret;
      updateComposerDraft(store, sessionId, (current) => ({ ...current, text }));
    },
  });
  const latest = useLatest({ context, slashCommands, draft, mode, sending, uploads, style, sessionId });
  const tagsRef = useRef<HTMLDivElement>(null);

  // A message being edited that left the queue (sent, withdrawn, dropped) leaves the editor as a new message.
  const editedGone = editing !== undefined && !messages.some((message) => message.id === editing.message);
  useEffect(() => {
    if (editedGone) keepEditAsNew(store, sessionId);
  }, [editedGone, sessionId]);

  // Read through `latest`: the field's key listener is set up once, and would otherwise word a failure in the language the
  // window had then.
  const submit = async () => {
    const { context: current, slashCommands: commands, draft: now, mode: chosen, sending: busy, sessionId: id } = latest.current;
    if (busy || !canSendDraft(now)) return;
    setSending(true);
    if (now.editing) await saveEdit(current, id, commands);
    else await sendDraft(current, id, chosen, commands);
    setSending(false);
  };

  // Keys and pastes are taken on the text area itself, which react-aria's handling would otherwise stop.
  useEffect(() => {
    const field = fieldRef.current;
    if (!field) return;
    const keydown = (event: KeyboardEvent) => {
      if (mention.onKey(event)) {
        event.preventDefault();
        return;
      }
      if (event.key !== "Enter" || isImeKey(event) || event.shiftKey || event.altKey || !isPrimaryModifier(event, host)) return;
      event.preventDefault();
      void submit();
    };
    const paste = (event: ClipboardEvent) => {
      const files = Array.from(event.clipboardData?.files ?? []);
      if (files.length === 0) return;
      event.preventDefault();
      attachFiles(store, latest.current.sessionId, files.map((file): DroppedFile => ({ file, folder: false })), latest.current.uploads);
    };
    const { sync, close } = mention;
    field.addEventListener("keydown", keydown);
    field.addEventListener("paste", paste);
    // Moves of the caret alone; an edit is taken in `onChange`.
    for (const type of ["keyup", "mouseup", "focus"]) field.addEventListener(type, sync);
    field.addEventListener("blur", close);
    return () => {
      field.removeEventListener("keydown", keydown);
      field.removeEventListener("paste", paste);
      for (const type of ["keyup", "mouseup", "focus"]) field.removeEventListener(type, sync);
      field.removeEventListener("blur", close);
    };
  }, []);

  // Files dropped on the composer: by their own paths where the platform reports them, else as the page's drop.
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    return watchFileDrops(root, fileDrop, {
      // Not while the composer is inert (behind the archive view), as the terminal takes nothing under its cover.
      accepting: () => root.closest("[inert]") === null,
      paths: (paths) => attachLocalPaths(store, latest.current.sessionId, paths, latest.current.style),
      files: (files) => attachFiles(store, latest.current.sessionId, files, latest.current.uploads),
    });
  }, []);

  // The editor on screen is where focus goes for this session's composer (`focusComposer`).
  useEffect(() => registerComposerEditor(store, sessionId, { focus: () => fieldRef.current?.focus() }), [sessionId]);
  useLayoutEffect(() => {
    const caret = caretAfter.current;
    if (caret === null) return;
    caretAfter.current = null;
    fieldRef.current?.setSelectionRange(caret, caret);
  }, [draft.text]);

  const modeOptions: Option<SendMode>[] = SEND_MODES.map((value) => ({ value, label: t(`composer.mode.${value}`) }));
  // A mode this client does not know is the daemon's to apply: the message goes with none, and its button says Send.
  const queues = mode === "when_idle" || mode === "manual";
  const sendLabel = editing ? t("composer.save") : t(queues ? "composer.queue" : "composer.send");
  const tooManyImages = draftImageCount(draft) > MAX_MESSAGE_IMAGES;
  const sendDisabled = !canSendDraft(draft) && !tooManyImages;
  // Sent, saved or emptied, the button disables while it may hold focus, which goes to the text area.
  useFocusLeavingWhenDisabled(sendRef, sendDisabled, fieldRef, () => rootRef.current);
  const changeMode = (value: SendMode | null) => {
    if (value !== null && value !== mode) void setSendMode(context, sessionId, value);
  };
  const removeAttachments = (ids: string[]) => {
    // The last tag going takes the group with it, and focus with the group: it goes to the text area.
    if (draft.attachments.every((attachment) => ids.includes(attachment.id))) handFocusOff(tagsRef.current, fieldRef.current, true);
    for (const id of ids) removeAttachment(store, sessionId, id);
  };
  // An attachment opens in the file viewer, read from the daemon's host by its path as a terminal's path link is; focus
  // comes back to its tag as the viewer closes, or to the text area where the tag has gone meanwhile.
  const viewAttachment = (path: string, tag: string) => {
    const returnFocus = () => {
      const target = rootRef.current?.querySelector<HTMLElement>(`[data-attachment="${CSS.escape(tag)}"]`) ?? fieldRef.current;
      target?.focus();
      return target !== null && target !== undefined;
    };
    onViewFile?.({ link: path, ...(session.project_id !== null && { project: session.project_id }) }, returnFocus);
  };
  const note = session.status === "waiting_user" ? t("composer.waitingUser") : hasNoProcess(session.status) ? t("composer.noProcess") : undefined;
  const waitingSaid = useAnnouncedOnRise(sessionId, session.status === "waiting_user", t("composer.waitingUser"), !exposed);
  const pick = (files: FileList | null) => {
    const picked = Array.from(files ?? []).map((file): DroppedFile => ({ file, folder: false }));
    attachFiles(store, sessionId, picked, uploads);
    if (pickerRef.current) pickerRef.current.value = "";
  };

  return (
    <section
      ref={rootRef}
      data-region="composer"
      aria-label={t("composer.region")}
      className={`groove-t relative flex min-h-0 flex-1 flex-col bg-panel ${FILE_DROP_TARGET_CLASS}`}
    >
      {messages.length > 0 && (
        // The groove on an element around the scroller: the scroller's own edge fades with its content once it overflows.
        <div className="flex max-h-48 min-h-0 shrink flex-col groove-b">
          <FadedScroller className="min-h-0">
            <QueuedMessageList
              session={sessionId}
              messages={messages}
              context={context}
              full
              editing={editing?.message}
              editedMode={editing ? (editing.restoreMode ?? "manual") : undefined}
              onEditedMode={(picked) => chooseEditedMode(store, sessionId, picked)}
              onEdit={(message) => {
                editQueuedMessage(context, sessionId, message, style);
                fieldRef.current?.focus();
              }}
              fallback={() => fieldRef.current}
              label={t("queue.label")}
            />
          </FadedScroller>
        </div>
      )}
      <div className="flex flex-col gap-2 px-(--pane-inset) py-2">
        {/* The agent turning to wait on the user is said once as it happens under the open composer, after an attended
            wait and held while the composer is out of reach; a session opened already waiting is not announced apart
            from it, and nothing is said once the session changes (the terminal's own bar says one that is not
            running). A live announcer, as the terminal's attended regions are: the write may come with an unrelated modal
            open. It takes no layout box, so it adds no gap to the column. The visible note is there for whoever looks. */}
        <div {...liveAnnouncerProps()} className="contents">
          <StatusAnnouncer text={waitingSaid} clearAfterRead />
        </div>
        {note && (
          <p className="flex items-center gap-1.5 text-xs text-app-muted">
            <Info className="size-3.5 shrink-0" />
            {note}
          </p>
        )}
        {draft.attachments.length > 0 && (
          // One tab stop: the arrow keys move between the files, Enter (or a press) opens one in the file viewer, Delete or
          // Backspace (or its remove button) takes it off.
          <TagGroup label={t("composer.attachments")} onRemove={removeAttachments}>
            <TagList ref={tagsRef} className="flex flex-wrap gap-1.5">
              {draft.attachments.map((attachment) => {
                const path = attachment.path;
                const opens = path !== undefined && onViewFile !== undefined;
                return (
                  <Tag
                    key={attachment.id}
                    id={attachment.id}
                    textValue={attachment.name}
                    data-attachment={attachment.id}
                    // A tag still being sent has no action yet, so it takes neither the pointer nor the hover tint.
                    className={opens ? "max-w-full" : "max-w-full cursor-default hover:bg-default ui-hovered:bg-default"}
                    onAction={opens ? () => viewAttachment(path, attachment.id) : undefined}
                    description={opens ? t("composer.attachmentOpens") : undefined}
                  >
                    {path === undefined ? <Spinner size="xs" label={t("composer.attaching", { name: attachment.name })} /> : <Paperclip className="size-3 shrink-0" />}
                    <FadeOverflow as="span" dir="auto" titleWhenClipped={attachment.name} className="min-w-0">{attachment.name}</FadeOverflow>
                    {attachment.image && <StatusBadge variant="soft" size="sm">{t("composer.image")}</StatusBadge>}
                    <TagRemoveButton label={t("composer.removeAttachmentTooltip")} />
                  </Tag>
                );
              })}
            </TagList>
          </TagGroup>
        )}
        <div className="relative">
          {mention.pickerOpen && <MentionList id={mention.listId} options={mention.options} files={mention.files} active={mention.activeIndex} onActive={mention.setActive} onPick={mention.pick} />}
          <TextField
            fullWidth
            value={draft.text}
            onChange={(text) => {
              updateComposerDraft(store, sessionId, (current) => ({ ...current, text }));
              mention.sync();
            }}
            readOnly={sending}
            label={editing ? t("composer.editLabel") : t("composer.label")}
          >
            <TextArea
              ref={fieldRef}
              landing
              rows={3}
              resizable={false}
              dir="auto"
              placeholder={t("composer.placeholder")}
              suggestions={{
                list: mention.listId,
                expanded: mention.pickerOpen,
                active: mention.options.length > 0 ? mentionOptionId(mention.listId, mention.activeIndex) : undefined,
              }}
            />
          </TextField>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" variant="tertiary" iconOnly label={t("composer.attach")} tooltip={t("composer.attach")} onPress={() => pickerRef.current?.click()}>
            <Paperclip className="size-4" />
          </Button>
          <input ref={pickerRef} type="file" multiple hidden tabIndex={-1} aria-hidden onChange={(event) => pick(event.target.files)} />
          {!editing && (mode === undefined ? (
            // A mode this client does not know shows as none chosen.
            <OptionSelect<SendMode> inline variant="primary" label={t("composer.mode.label")} placeholder={t("composer.mode.label")} options={modeOptions} value={null} onChange={changeMode} />
          ) : (
            <OptionSelect<SendMode> inline variant="primary" label={t("composer.mode.label")} options={modeOptions} value={mode} onChange={changeMode} />
          ))}
          <div className="ms-auto flex items-center gap-2">
            {editing && (
              <Button
                ref={cancelRef}
                size="sm"
                variant="secondary"
                onPress={() => {
                  // The button goes with the edit: focus goes to the text area first.
                  handFocusOff(cancelRef.current, fieldRef.current, true);
                  endEdit(context, sessionId);
                }}
              >
                {t("common.cancel")}
              </Button>
            )}
            <Button
              ref={sendRef}
              size="sm"
              pending={sending}
              disabled={sendDisabled}
              unavailable={tooManyImages ? t("composer.tooManyImages", { count: MAX_MESSAGE_IMAGES }) : undefined}
              tooltip={<span className="inline-flex items-center gap-2">{sendLabel}<BindingKbd binding={SEND_BINDING} /></span>}
              onPress={() => void submit()}
            >
              {sendLabel}
            </Button>
          </div>
        </div>
      </div>
    </section>
  );
}

/** Whether the composer shows below `session`'s terminal: it is on in this window, the daemon keeps a queue, and the
 * session is one a message can be written to (selected and not archived). */
export function useComposerShown(session: Session | undefined): boolean {
  const enabled = sessionComposerEnabled.useValue();
  const offered = useQueueOffered();
  return composerOffered(enabled, offered, session);
}
