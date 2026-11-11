// The blocks of a page that carry text to copy: a code block, a diff and a copyable block share a label above the
// block and a copy button floating over the block's top end corner.
import React from "react";

import { CopyButton } from "../../components/controls/CopyButton";
import { ROW_ICON_BUTTON_CLASS } from "../../components/controls/RowControls";
import { FadedScroller, useFadeRegion } from "../../components/scroll/scrollFade";
import { isolate } from "../../i18n/isolate";
import { useT } from "../../i18n/react";
import { useOctoboardTheme } from "../../preferences/theme";
import { CODE_THEMES } from "../../viewer/code/codeTheme";
import type { ComponentProps } from "./registry";

/** A block's label, above its box; nothing where the block has none. `decorative` hides it from assistive technology,
 * for a block whose name is read from elsewhere. */
export function BlockLabel({ label, decorative }: { label?: string; decorative?: boolean }): React.ReactElement | null {
  if (label === undefined) return null;
  return <span aria-hidden={decorative || undefined} className="text-xs font-medium text-app-muted align-match-parent [overflow-wrap:anywhere]" dir="auto">{label}</span>;
}

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
 * (`auto` follows the text) so that the button sits at the end of the text it copies. The button is named after the
 * block's `label` where it has one. Copying is not editing, so the button works on a page that is read-only, in history
 * or submitted. */
export function CopyBox({ text, label, dir, surface, tint, children }: { text: string; label?: string; dir: "auto" | "ltr"; surface: string; tint?: string; children: React.ReactNode }): React.ReactElement {
  const t = useT();
  return (
    <div dir={dir} className="relative">
      {children}
      <span className="absolute end-px top-px flex h-9 w-9 items-center justify-center overflow-hidden rounded-se-[7px]" style={{ backgroundColor: surface }}>
        {tint && <span aria-hidden="true" className={`absolute inset-x-0 top-2 h-5 ${tint}`} />}
        <CopyButton
          text={text}
          beforeCopyText={label === undefined ? undefined : t("report.copyNamed", { label: isolate(label) })}
          failedText={t("report.copyFailed")}
          variant="ghost"
          size="xs"
          className={`relative ${ROW_ICON_BUTTON_CLASS}`}
        />
      </span>
    </div>
  );
}

/** The surface a copy block is drawn on, the same as a `code` block's: the code theme's own fill and text colour, which
 * stand apart from the panel in both appearances. `style` goes on the block's box (the box's border follows the text
 * colour, as the code block's does) and `surface` on its `CopyBox`. */
export function useCodeSurface(): { style: React.CSSProperties; surface: string; boxClass: string } {
  const { resolved } = useOctoboardTheme();
  const { background, foreground } = CODE_THEMES[resolved];
  return { style: { backgroundColor: background, color: foreground }, surface: background, boxClass: "rounded-lg border border-current/25" };
}

/** A box that scrolls sideways instead of wrapping its text. Its frame (the box's border, fill and corners, `className`
 * and `style`) is around the scroller, which takes focus only while it overflows, as a region named `label`, so the
 * keyboard reaches the text beyond its edge; the focus ring is drawn on the frame. */
export function ScrollBox({ label, className = "", style, children }: { label: string; className?: string; style?: React.CSSProperties; children: React.ReactNode }): React.ReactElement {
  const region = useFadeRegion({ label });
  return (
    <div style={style} className={`${className} ${region.ring}`}>
      <FadedScroller orientation="horizontal" region={region}>{children}</FadedScroller>
    </div>
  );
}

export function CopyableView({ component }: ComponentProps<"copyable">): React.ReactElement {
  const t = useT();
  const monospace = component.monospace !== false;
  const { style, surface, boxClass } = useCodeSurface();
  return (
    <div className="flex flex-col gap-1">
      <BlockLabel label={component.label} />
      <CopyBox text={component.text} label={component.label} dir="auto" surface={surface}>
        {monospace ? (
          <ScrollBox label={component.label ?? t("report.block.copyable")} className={boxClass} style={style}>
            <pre dir="auto" className={`w-max min-w-full py-2 ps-3 ${COPY_BUTTON_INSET} align-match-parent font-mono text-xs leading-5`}>
              {component.text}
            </pre>
          </ScrollBox>
        ) : (
          <pre dir="auto" style={style} className={`${boxClass} py-2 ps-3 ${COPY_BUTTON_INSET} align-match-parent font-sans text-sm leading-5 break-words whitespace-pre-wrap`}>
            {component.text}
          </pre>
        )}
      </CopyBox>
    </div>
  );
}
