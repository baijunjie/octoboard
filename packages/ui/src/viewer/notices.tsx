// What the viewer says about the content it shows, as marks: one for each notice, drawn in the header
// after the file path (`FileNotice`) or at the spot in a document it is about (`SpotNotice`,
// `BlockFallback`).
import React, { createContext, useCallback, useContext, useMemo, useState } from "react";
import { createPortal } from "react-dom";

import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/StatusAnnouncer";
import { StatusMark } from "../components/StatusMark";
import { useMarkBesideLink } from "./linkMarks";

/** How long any of the viewer's announcements (its loading, a `Failure`, a notice) wait before their first
 * text: the dialog takes focus as it opens. Moving to another file in an open viewer has no such focus
 * move and is announced at once. */
export const FIRST_ANNOUNCEMENT_DELAY_MS = AFTER_FOCUS_MOVE_DELAY_MS;

/**
 * What kind of thing a notice is saying, which is what tells them apart on screen.
 *
 * Pick the tone by what the sentence tells the user, which is all the user has to go on:
 *
 * - `explained` — it states how the content below is drawn and why, for what this file or change
 *   is: a size budget, a change read at an older version, lines left collapsed because the whole
 *   file is not to be had.
 * - `warned` — it says something about the content that has to be known before reading it: a side
 *   of the change that lies outside the project.
 * - `failed` — it reports that something went wrong, so the view that was asked for is not the one
 *   on screen: a read that errored, a diff that could not be drawn, an image that would not decode.
 *
 * A sentence worded as one and meaning another is a fault in the sentence, not a reason to tone it
 * against its wording: change the message, and the tone follows.
 */
export type NoticeTone = "explained" | "warned" | "failed";

/** What a tone is drawn as: `StatusMark`'s tones, whose glyphs differ in shape (the circled `i`, the
 * warning triangle, the circled `!`) and not only in colour (`docs/ui-accessibility.md`, "Not by colour
 * alone"). */
export const MARK_TONES = { explained: "note", warned: "warning", failed: "danger" } as const;

type Slots = Record<NoticeTone, HTMLElement | null>;

const NO_SLOTS: Slots = { failed: null, warned: null, explained: null };

/** Where `FileNotice` draws its marks: the slots of the nearest `NoticeSlotsProvider`, and how a
 * `NoticeSlotBoxes` registers its elements with it. Absent where there is no header, and the marks are
 * then not drawn. */
const NoticeSlots = createContext<{ slots: Slots; register: (tone: NoticeTone, element: HTMLElement | null) => void } | null>(null);

/**
 * A place for whole-file marks: a slot for each tone, in the order the marks stand (failed, then
 * warned, then notes), whichever surface raises them. The viewer's header has one after the file path,
 * and a side of a compared image has one beside its heading, which takes the notices of that side's
 * content. `children` hold both the content whose notices go there and the `NoticeSlotBoxes`.
 */
export function NoticeSlotsProvider({ children }: { children: React.ReactNode }): React.ReactElement {
  const [slots, setSlots] = useState<Slots>(NO_SLOTS);
  const register = useCallback((tone: NoticeTone, element: HTMLElement | null) => setSlots((current) => (current[tone] === element ? current : { ...current, [tone]: element })), []);
  const value = useMemo(() => ({ slots, register }), [slots, register]);
  return <NoticeSlots value={value}>{children}</NoticeSlots>;
}

/** The slots themselves, in the order the marks stand: `display: contents` boxes, so the marks lay out
 * as items of whatever holds them. */
export function NoticeSlotBoxes(): React.ReactElement {
  const register = useContext(NoticeSlots)?.register;
  const failed = useCallback((element: HTMLElement | null) => register?.("failed", element), [register]);
  const warned = useCallback((element: HTMLElement | null) => register?.("warned", element), [register]);
  const explained = useCallback((element: HTMLElement | null) => register?.("explained", element), [register]);
  return (
    <>
      <span ref={failed} className="contents" />
      <span ref={warned} className="contents" />
      <span ref={explained} className="contents" />
    </>
  );
}

/** Says `text` in a status region that is emptied again once it has been read, so a screen reader
 * reading the page line by line meets the sentence only once, on the mark. */
function SpokenSentence({ text, element }: { text: string; element?: "div" | "span" }): React.ReactElement {
  return <StatusAnnouncer text={text} firstWriteDelayMs={FIRST_ANNOUNCEMENT_DELAY_MS} clearAfterRead element={element} />;
}

/**
 * What is said about the whole file or change on screen: why it is shown this way, what to know about
 * it first, or what part of it could not be shown. It is one mark in the `NoticeSlotBoxes` of the nearest
 * `NoticeSlotsProvider`, not where this is rendered. Without them there is nowhere to draw it and nothing is.
 *
 * The mark is named by `sentence`, which is also what it says: every tone opens it in a popover.
 * `line` is the same sentence with markup in it (a path in a chip), shown in the popover in place of
 * the plain text; the popover is named by the tone either way.
 *
 * A notice that can appear after the surface has been drawn would otherwise never reach a reader,
 * so it is also spoken: pass `speak`, which says the sentence in a status region. A notice that is
 * there from the first paint, or that a region of the surface already speaks, passes none. A failed
 * notice is spoken the same way, as a status and not an alert: it is a rendering fallback, with the
 * content still shown. A failed notice that is not a rendering fallback (a conflict's read failure)
 * passes none either: the pane's `Failure` speaks it, as an alert.
 */
export function FileNotice({
  tone = "explained",
  sentence,
  speak = false,
  line,
}: {
  tone?: NoticeTone;
  sentence: string;
  speak?: boolean;
  line?: React.ReactNode;
}): React.ReactElement {
  const slot = useContext(NoticeSlots)?.slots[tone];
  return (
    <>
      {slot && createPortal(<StatusMark tone={MARK_TONES[tone]} label={sentence} line={line} />, slot)}
      {speak && <SpokenSentence text={sentence} />}
    </>
  );
}

/**
 * A notice about one spot of a document, drawn at that spot as a mark named and worded by its
 * sentence (`children`), opened in a popover whatever the tone. The mark is phrasing content, so it
 * is valid inside a paragraph. The glyph tells the tones apart by shape, so colour is not what does.
 *
 * `speak` is announced once, after the same delay as the other notices. A notice that is present
 * from the first paint passes none. The announcement is a `span`: this notice may sit where a `div`
 * cannot.
 *
 * A popover mark is a button, so it is a tab stop in the running text; inside a link it is drawn
 * right after the link rather than in it (`LinkWithMarks`).
 */
export function SpotNotice({
  tone = "explained",
  speak = false,
  children,
}: {
  tone?: NoticeTone;
  speak?: boolean;
  children: string;
}): React.ReactElement {
  const mark = useMemo(
    () => (
      <span className="mx-1">
        <StatusMark inline tone={MARK_TONES[tone]} label={children} />
      </span>
    ),
    [tone, children],
  );
  // Inside a link the mark is drawn beside it, outside the anchor (see `LinkWithMarks`).
  const besideLink = useMarkBesideLink(mark);
  return (
    <>
      {!besideLink && mark}
      {speak && <SpokenSentence text={children} element="span" />}
    </>
  );
}

/**
 * The source a block of a document (a diagram, a display formula) falls back to, with the mark that
 * says why at its start. The mark is level with the source's first line, which `children` draw with
 * 0.75rem padding and 1.25rem line height. `className` is the block's margin.
 */
export function BlockFallback({
  tone = "explained",
  speak = false,
  sentence,
  className,
  children,
}: {
  tone?: NoticeTone;
  speak?: boolean;
  sentence: string;
  className: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <div className={`${className} flex items-start gap-1`}>
      <div className="mt-3 flex h-5 shrink-0 items-center">
        <StatusMark tone={MARK_TONES[tone]} label={sentence} />
      </div>
      <div className="min-w-0 flex-1">{children}</div>
      {speak && <SpokenSentence text={sentence} />}
    </div>
  );
}
