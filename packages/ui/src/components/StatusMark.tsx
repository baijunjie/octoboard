import { CircleAlert, Info, TriangleAlert } from "lucide-react";
import React from "react";

import { useT } from "../i18n/react";
import { Button } from "./Button";
import { Hint } from "./Hint";

/** What a mark says: an error, a warning, or a note that is neither. */
export type MarkTone = "danger" | "warning" | "note";

const GLYPHS = { danger: CircleAlert, warning: TriangleAlert, note: Info } as const;
const GLYPH_COLOURS = { danger: "text-danger-soft-foreground", warning: "text-warning-soft-foreground", note: "text-accent-soft-foreground" } as const;

/** A mark of any tone: a popover. */
interface StatusMarkProps {
  tone?: MarkTone;
  /** What the mark is about, its accessible name. */
  label: string;
  /** The glyph takes the text's size and baseline. */
  inline?: boolean;
  /** A visible word after the glyph; the accessible name stays `label`. */
  text?: string;
  /** The popover's body; without it the `label` is the one line, and the tone's word names the popover. */
  children?: React.ReactNode;
  /** The one line drawn richer than the plain-text `label` (markup in it), the same sentence. The mark stays
   * bare, so the tone's word still names the popover; with `children`, or with `text` and no `children` (no
   * popover then), it has no effect. */
  line?: React.ReactNode;
  /** The mark stands in a collection's section header, which the collection's own walk does not reach. */
  inHeader?: boolean;
  /** The popover's last line, the recovery button (Try again), given a function that closes the popover. */
  actions?: (close: () => void) => React.ReactNode;
}

/**
 * The minimal mark of a status that belongs to one row, cell or spot: a glyph named by `label`
 * whose content opens from the glyph in one floating surface, never two. What each tone opens, how
 * a popover opens and takes focus, and the in-header and inline forms are in
 * `docs/ui-accessibility.md`, "A failure that belongs to one row".
 *
 * Every tone is the same mark: a button (`MarkButton`) that opens its content in a popover
 * (`PopoverMark`), whatever the tone, so marks side by side look alike, take the pointer alike, are each a
 * tab stop and open the same way; the tones differ only in glyph and colour. A button is phrasing
 * content, so a mark is valid inside a paragraph as well as beside one. With a `text` word and no body
 * the mark stays the plain glyph and word, since the word already says it.
 */
export function StatusMark(props: StatusMarkProps): React.ReactElement {
  const { label, tone = "danger", inline = false, text, children } = props;
  const Icon = GLYPHS[tone];
  const glyph = <Icon aria-hidden="true" className={`${inline ? "size-3.5" : "size-4"} shrink-0 ${GLYPH_COLOURS[tone]}`} />;
  if (!children && text) {
    return (
      <span role="img" aria-label={label} className="inline-flex items-center gap-1.5 whitespace-nowrap">
        {glyph}
        {text}
      </span>
    );
  }
  return <PopoverMark label={label} tone={tone} line={props.line} glyph={glyph} inline={inline} inHeader={props.inHeader ?? false} text={text} actions={props.actions}>{children}</PopoverMark>;
}

/** The mark's trigger, the same for every tone: an icon button, or the glyph and its word, ghost, so it
 * is tinted under the pointer like any icon button. The rest of its props are the trigger wiring of
 * the hint that wraps it. */
function MarkButton({
  label,
  glyph,
  inline,
  text,
  ...wiring
}: { label: string; glyph: React.ReactElement; inline: boolean; text?: string } & Omit<React.ComponentProps<typeof Button>, "children">): React.ReactElement {
  if (text) {
    return (
      <Button
        {...wiring}
        data-status-mark=""
        variant="ghost"
        aria-label={label}
        className="-mx-1 -my-0.5 h-auto min-h-0 w-auto min-w-0 shrink-0 gap-1.5 rounded-full px-1 py-0.5 align-baseline text-[length:inherit] leading-[inherit] font-normal text-inherit whitespace-nowrap [&>svg]:m-0"
      >
        {glyph}
        {text}
      </Button>
    );
  }
  if (inline) {
    // A square box around the glyph (22px for the 14px glyph), so that its round shape is a circle as
    // everywhere else; the negative block margins keep the line's height as it was, not the box's size.
    return (
      <Button
        {...wiring}
        data-status-mark=""
        isIconOnly
        variant="ghost"
        aria-label={label}
        className="mx-[-0.1875rem] my-[-0.25rem] size-[1.375rem] min-h-0 min-w-0 shrink-0 rounded-full p-0 align-middle leading-none [&>svg]:m-0"
      >
        {glyph}
      </Button>
    );
  }
  return (
    <Button {...wiring} data-status-mark="" isIconOnly size="xs" variant="ghost" aria-label={label} className="shrink-0 -my-0.5 rounded-full">
      {glyph}
    </Button>
  );
}

/** The popover the mark opens. */
function PopoverMark({
  label,
  tone,
  line,
  glyph,
  inline,
  inHeader,
  text,
  actions,
  children,
}: {
  label: string;
  tone: MarkTone;
  line?: React.ReactNode;
  glyph: React.ReactElement;
  inline: boolean;
  inHeader: boolean;
  text?: string;
  actions?: (close: () => void) => React.ReactNode;
  children?: React.ReactNode;
}): React.ReactElement {
  const t = useT();
  // No body of its own: the label (or `line`, its richer form) is the one line of reason. It cannot also
  // name the popover, or a screen reader would say it as the dialog's name and again as its content, so
  // the tone does.
  const bare = !children;
  const header = inHeader ? { excludeFromTabOrder: true, "data-section-heading-control": "" } : undefined;
  // The popover gives focus back to what held it as it unmounts, from a frame of its own, so the
  // heading's band takes it from the frame after that, unless the user has put focus elsewhere.
  const handBackToHeading = ({ wasPressed, trigger }: { wasPressed: boolean; trigger: HTMLElement | null }) => {
    if (!wasPressed || !inHeader) return;
    const band = trigger?.closest("[role=row]")?.querySelector<HTMLElement>("[data-section-heading]");
    const popovers = [...document.querySelectorAll("[data-status-mark-popover]")];
    requestAnimationFrame(() => requestAnimationFrame(() => {
      const active = document.activeElement;
      const unclaimed = !active || active === document.body || active === trigger || popovers.some((popover) => popover.contains(active));
      if (unclaimed && band?.isConnected) band.focus();
    }));
  };
  return (
    <Hint
      kind="popover"
      label={bare ? t(`mark.tone.${tone}`) : label}
      bodyClassName={bare && !actions ? undefined : "flex flex-col gap-2 [overflow-wrap:anywhere]"}
      contentProps={{ "data-status-mark-popover": "" }}
      onClosed={handBackToHeading}
      trigger={({ onMouseHoverChange: _, expanded: __, popupId: ___, ...wiring }) => (
        <MarkButton {...wiring} {...header} label={label} glyph={glyph} inline={inline} text={text} />
      )}
    >
      {({ close }) => (
        <>
          {bare ? (line ?? label) : children}
          {actions && <div className="flex justify-end">{actions(close)}</div>}
        </>
      )}
    </Hint>
  );
}
