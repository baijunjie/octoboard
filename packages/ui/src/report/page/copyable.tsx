// The blocks of a page that carry text to copy: a code block, a diff and a copyable block share a label above the
// block and a copy button floating over the block's top end corner (`CopyBox`).
import React from "react";

import { COPY_BUTTON_INSET, CopyBox, CopyCommand } from "../../components/controls/CopyBox";
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

/** The surface a copy block is drawn on, the same as a `code` block's: the code theme's own fill and text colour, which
 * stand apart from the panel in both appearances. `style` goes on the block's box (the box's border follows the text
 * colour, as the code block's does) and `surface` on its `CopyBox`. */
export function useCodeSurface(): { style: React.CSSProperties; surface: string; boxClass: string } {
  const { resolved } = useOctoboardTheme();
  const { background, foreground } = CODE_THEMES[resolved];
  return { style: { backgroundColor: background, color: foreground }, surface: background, boxClass: "rounded-lg border border-current/25" };
}

/** The words of a block's copy button: its name ("Copy {label}" where the block has a label) and what the error alert says
 * when the copy fails. */
export function useCopyNames(label: string | undefined): { copyText: string; failedText: string } {
  const t = useT();
  return { copyText: label === undefined ? t("common.copy") : t("report.copyNamed", { label: isolate(label) }), failedText: t("report.copyFailed") };
}

export function CopyableView({ component }: ComponentProps<"copyable">): React.ReactElement {
  const t = useT();
  const monospace = component.monospace !== false;
  const codeSurface = useCodeSurface();
  const names = useCopyNames(component.label);
  return (
    <div className="flex flex-col gap-1">
      <BlockLabel label={component.label} />
      {monospace ? (
        <CopyCommand text={component.text} variant={codeSurface} dir="auto" wrap={false} scrollLabel={component.label ?? t("report.block.copyable")} {...names} />
      ) : (
        <CopyBox text={component.text} dir="auto" surface={codeSurface.surface} {...names}>
          <pre dir="auto" style={codeSurface.style} className={`${codeSurface.boxClass} py-2 ps-3 ${COPY_BUTTON_INSET} align-match-parent font-sans text-sm leading-5 break-words whitespace-pre-wrap`}>
            {component.text}
          </pre>
        </CopyBox>
      )}
    </div>
  );
}
