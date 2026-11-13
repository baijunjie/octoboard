import React from "react";

import { ScrollBox } from "../scroll/ScrollBox";
import { CopyButton } from "./CopyButton";
import { ROW_ICON_BUTTON_CLASS } from "./RowControls";

/** The end padding that leaves the end of a box's content clear of its copy button: the button's 24px and the 6px on
 * either side. */
export const COPY_BUTTON_INSET = "pe-9";

/** A block's box with its copy button floating over the box's top end corner, centred on the first line. The button is
 * fixed in the corner while a box that scrolls sideways moves under it, so it sits on a backing, drawn in `surface`,
 * the box's own fill, which hides the text scrolled beneath: it reaches the box's end edge and the top, `COPY_BUTTON_INSET`
 * wide and as tall, so no text shows around the button. Every block draws its first line's centre 19px below the box's
 * top, which is where the button is centred. `tint` is a background class laid over `surface` on that first line (a
 * diff's, drawn under 8px of padding) where it has a fill of its own, so the backing matches it. The box keeps
 * `COPY_BUTTON_INSET` of room at the end of its content for the longest line to be scrolled clear. The box takes `dir`
 * (`auto` follows the text) so that the button sits at the end of the text it copies. `copyText` is the button's name
 * and `failedText` what the error alert says when the copy fails: the caller words both, since what is copied is its own.
 * Copying is not editing, so the button works on a page that is read-only, in history or submitted. */
export function CopyBox({ text, copyText, failedText, dir, surface, tint, children }: {
  text: string;
  copyText: string;
  failedText: string;
  dir: "auto" | "ltr";
  surface: string;
  tint?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <div dir={dir} className="relative">
      {children}
      <span className="absolute end-px top-px flex h-9 w-9 items-center justify-center overflow-hidden rounded-se-[7px]" style={{ backgroundColor: surface }}>
        {tint && <span aria-hidden="true" className={`absolute inset-x-0 top-2 h-5 ${tint}`} />}
        <CopyButton
          text={text}
          beforeCopyText={copyText}
          failedText={failedText}
          variant="ghost"
          size="xs"
          className={`relative ${ROW_ICON_BUTTON_CLASS}`}
        />
      </span>
    </div>
  );
}

/** The look of a `CopyCommand`: `secondary`, HeroUI's secondary surface, for a block in a settings row or a dialog; or
 * the code theme's own fill and text colour, as the report page draws its code (`useCodeSurface` in
 * `report/page/copyable.tsx` makes one). */
export type CopyCommandVariant = "secondary" | { style: React.CSSProperties; surface: string; boxClass: string };

const SECONDARY = { style: { backgroundColor: "var(--surface-secondary)" }, surface: "var(--surface-secondary)", boxClass: "rounded-lg border border-transparent" };

/** A command, or a short piece of monospace text, to read whole and copy: its text on a box of the given `variant`, with
 * the copy button of `CopyBox` in the box's top end corner, the same distance from the end edge as from the top, centred
 * on the first line (the line is 20px under 8px of padding and the box's 1px border, which the secondary box draws in
 * transparent so that its first line sits where the report's blocks have theirs) and holding the first line's place when
 * the text wraps below it. The text never runs under the button. It wraps at any width by default, so a long command is
 * read in full; with `wrap: false` it scrolls sideways instead, in a region named `scrollLabel`. `dir` is the text's
 * (`ltr` for a command). */
export function CopyCommand({ text, variant = "secondary", dir = "ltr", copyText, failedText, ...wrapping }: {
  text: string;
  variant?: CopyCommandVariant;
  dir?: "auto" | "ltr";
  copyText: string;
  failedText: string;
} & ({ wrap?: true } | { wrap: false; scrollLabel: string })): React.ReactElement {
  const { style, surface, boxClass } = variant === "secondary" ? SECONDARY : variant;
  const textClass = `py-2 ps-3 ${COPY_BUTTON_INSET} align-match-parent font-mono text-xs leading-5`;
  const box = wrapping.wrap === false
    ? (
      <ScrollBox label={wrapping.scrollLabel} className={boxClass} style={style}>
        <pre dir={dir} className={`w-max min-w-full ${textClass}`}>{text}</pre>
      </ScrollBox>
    )
    : (
      <div style={style} className={boxClass}>
        <pre dir={dir} className={`whitespace-pre-wrap [overflow-wrap:anywhere] ${textClass}`}>{text}</pre>
      </div>
    );
  return <CopyBox text={text} copyText={copyText} failedText={failedText} dir={dir} surface={surface}>{box}</CopyBox>;
}
