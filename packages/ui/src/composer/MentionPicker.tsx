import { CircleAlert, Folder, MessageSquare } from "lucide-react";
import React, { useEffect } from "react";

import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { useAlertAnnouncement } from "../components/announce/useAlertAnnouncement";
import { FileTypeIcon } from "../components/icons/FileTypeIcon";
import { FadedScroller } from "../components/scroll/scrollFade";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { displayWirePath } from "../format/wirePath";
import type { Translate } from "../i18n/catalog";
import { useT } from "../i18n/react";
import { Spinner } from "../primitives";
import type { Session } from "../protocol";
import { sessionChoiceTitle, sessionLocation } from "../sessions/sessionLabel";
import { pastedPath } from "../terminal/pastedImage";
import { useDaemonStore } from "../store";
import { mentionSessions, sessionMention } from "./mentions";
import { useMentionPaths, type MentionPaths } from "./useMentionPaths";

/** The most sessions the picker lists at once. */
export const MENTION_SESSION_LIMIT = 30;

/** One choice the picker offers: what picking it inserts, and what its row shows. */
export interface MentionOption {
  key: string;
  kind: "file" | "directory" | "session";
  /** The text picking it puts in place of what was typed. */
  inserted: string;
  label: string;
  detail?: string;
}

/** What the picker offers for `query` in `session`'s composer while `active`: the files and directories of its project
 * (none for a console session), then the sessions `mentionSessions` offers. The sessions are read only while the
 * picker is open, so a closed one renders nothing as they change. */
export function useMentionOptions(session: Session, query: string, active: boolean): { options: MentionOption[]; files: MentionPaths["state"] | undefined } {
  const t = useT();
  const project = session.project_id ?? undefined;
  const paths = useMentionPaths(project, query, active);
  const sessions = useDaemonStore((s) => (active ? s.sessions : undefined));
  const consoles = useDaemonStore((s) => (active ? s.consoles : undefined));
  const projects = useDaemonStore((s) => (active ? s.projects : undefined));
  if (!active) return { options: [], files: undefined };
  const options: MentionOption[] = paths.paths.map((entry) => {
    const shown = displayWirePath(entry.path);
    // A path with whitespace goes in quoted, as a file dropped on the terminal does: an agent splits a bare one at the
    // space.
    return { key: `path:${entry.path}`, kind: entry.directory ? "directory" : "file", inserted: pastedPath(shown), label: shown };
  });
  const title = (other: Session) => sessionChoiceTitle(t, other);
  const offered = sessions ? mentionSessions(sessions.values(), session, query, title).slice(0, MENTION_SESSION_LIMIT) : [];
  for (const other of offered) {
    options.push({
      key: `session:${other.id}`,
      kind: "session",
      inserted: sessionMention(title(other), other.id),
      label: title(other),
      detail: consoles && projects ? sessionLocation(t, other, consoles, projects) : undefined,
    });
  }
  return { options, files: project === undefined ? undefined : paths.state };
}

/** The id of the option at `index` in the list `listId`. */
export function mentionOptionId(listId: string, index: number): string {
  return `${listId}-option-${index}`;
}

/** The note in the files' place while there are no files to list: being looked for, failed, or none matching. */
function filesNoteText(t: Translate, state: MentionPaths["state"]): string {
  if (state === "loading") return t("browser.filter.searching");
  return state === "error" ? t("composer.mention.filesFailed") : t("composer.mention.noFiles");
}

/** The glyph before the files note: the wait indicator while the files are looked for, the error glyph for a
 * failure. */
function filesNoteGlyph(state: MentionPaths["state"]): React.ReactNode {
  if (state === "loading") return <Spinner size="xs" />;
  return state === "error" ? <CircleAlert className="size-3.5 shrink-0 text-danger-soft-foreground" /> : null;
}

/**
 * The list `@` opens above the composer's text, of the files and directories of the session's project and the other
 * sessions, under a heading each. It is a hand-built listbox rather than HeroUI's `ListBox` or react-aria's
 * `Autocomplete`, which cannot meet the picker's need: focus stays in the multi-line text area, whose own Enter starts
 * a line and whose Escape must not clear what was written, while the arrow keys, Enter and Tab (taken by the composer
 * while the list is open) move through the options as the area's active descendant (`TextArea`'s `suggestions`). A
 * press on a row picks it without taking focus from the area; the pointer moves the highlight.
 */
export function MentionList({ id, options, files, active, onActive, onPick }: {
  id: string;
  options: readonly MentionOption[];
  /** The files' state, for a project session; none for a console session, which lists no files. */
  files: MentionPaths["state"] | undefined;
  active: number;
  onActive: (index: number) => void;
  onPick: (index: number) => void;
}): React.ReactElement {
  const t = useT();
  const activeId = options.length > 0 ? mentionOptionId(id, active) : undefined;
  // The highlighted option is kept in view as the keys move it.
  useEffect(() => {
    if (activeId) document.getElementById(activeId)?.scrollIntoView?.({ block: "nearest" });
  }, [activeId]);
  const fileCount = options.filter((option) => option.kind !== "session").length;
  const row = (option: MentionOption, index: number) => (
    <div
      key={option.key}
      id={mentionOptionId(id, index)}
      role="option"
      aria-selected={index === active}
      data-current={index === active || undefined}
      className="flex min-w-0 cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm data-[current]:bg-(--default)"
      // The text area keeps focus: a press picks the row without moving it.
      onMouseDown={(event) => event.preventDefault()}
      onPointerMove={() => {
        if (index !== active) onActive(index);
      }}
      onClick={() => onPick(index)}
    >
      {option.kind === "session" ? (
        <MessageSquare className="size-4 shrink-0 text-app-muted" />
      ) : option.kind === "directory" ? (
        <Folder className="size-4 shrink-0 text-app-muted" />
      ) : (
        <FileTypeIcon path={option.label} />
      )}
      {/* A path that does not fit keeps its end, the name, in view. */}
      <FadeOverflow as="span" dir="auto" clip={option.kind === "session" ? "end" : "start"} className="block min-w-0 flex-1">{option.label}</FadeOverflow>
      {option.detail && <FadeOverflow as="span" dir="auto" className="block max-w-[40%] min-w-0 shrink-0 ps-2 text-xs text-app-muted">{option.detail}</FadeOverflow>}
    </div>
  );
  const heading = (headingId: string, text: string) => (
    <div id={headingId} role="presentation" className="px-2 pt-1.5 pb-1 text-xs font-medium text-app-muted">{text}</div>
  );
  const sessions = options.slice(fileCount);
  // A note stands outside the lists, which hold options only: in the files' place, the files being looked for or why
  // none are listed; for a console session, which lists no files, that no session matches.
  // Focus stays in the text area, so the note reaches a screen reader on its own: a status mounted with the picker
  // (written after its first-write delay, and at once as it changes) for the files being looked for or none matching,
  // an alert for a failed search. The visible note is hidden from it, so nothing is said twice.
  const noteOf = (content: React.ReactNode) => <div aria-hidden className="flex items-center gap-2 px-2 py-1.5 text-xs text-app-muted">{content}</div>;
  const filesNoteShown = files !== undefined && fileCount === 0;
  const sessionsNoteShown = files === undefined && sessions.length === 0;
  const noteText = filesNoteShown ? filesNoteText(t, files) : sessionsNoteShown ? t("composer.mention.none") : undefined;
  const failed = filesNoteShown && files === "error";
  const listLabel = t("composer.mention.label");
  return (
    // A floating surface, as a menu's or a select's list is, so that in the dark theme it stands above the panel.
    <div className="floating-surface absolute inset-x-0 bottom-full z-20 mb-1 flex min-h-0 flex-col rounded-lg bg-overlay shadow-lg">
      <FadedScroller className="max-h-64 p-1">
        <StatusAnnouncer text={failed ? undefined : noteText} />
        {failed && <SearchFailureAnnouncement text={noteText} />}
        {filesNoteShown && noteOf(<>{filesNoteGlyph(files)}{noteText}</>)}
        <div id={id} role="listbox" aria-label={listLabel}>
          {fileCount > 0 && (
            <div role="group" aria-labelledby={`${id}-files`}>
              {heading(`${id}-files`, t("composer.mention.files"))}
              {options.slice(0, fileCount).map(row)}
            </div>
          )}
          {sessions.length > 0 && (
            <div role="group" aria-labelledby={`${id}-sessions`}>
              {heading(`${id}-sessions`, t("composer.mention.sessions"))}
              {sessions.map((option, index) => row(option, fileCount + index))}
            </div>
          )}
        </div>
        {sessionsNoteShown && noteOf(noteText)}
      </FadedScroller>
    </div>
  );
}

/** The failed search spoken as an alert, as a failure is: mounted with the failure, it takes its role a moment later,
 * so the alert lands in a region that already exists and after the echo of the key typed. */
function SearchFailureAnnouncement({ text }: { text: string | undefined }): React.ReactElement {
  const announcement = useAlertAnnouncement(true, undefined, AFTER_FOCUS_MOVE_DELAY_MS);
  return <span key={announcement.key} role={announcement.role} className="sr-only">{text}</span>;
}
