import { Expand, Shrink, ZoomIn, ZoomOut } from "lucide-react";
import React from "react";

import { ROW_ICON_BUTTON_CLASS } from "../../components/controls/RowControls";
import { TitledControl } from "../../components/hint/TitledControl";
import { useCurrentLanguage, useT } from "../../i18n/react";
import { Button } from "../../primitives";
import { canStep, formatPercent, type ZoomBounds } from "./zoom";

const BAR_BUTTON_CLASS = `row-icon-control ${ROW_ICON_BUTTON_CLASS}`;

/**
 * The zoom bar floating over an image's top trailing corner: Zoom out, the percentage, Zoom in, and the fit button,
 * which goes to actual size while fitted and back to fitted once zoomed. It stands far enough from the trailing edge
 * on a wide window to stay off a scrollbar that takes space. Marked `data-zoom-bar` for a zoom that disables the
 * focused control to hand focus to.
 */
export function ZoomBar({
  bounds,
  scale,
  fitted,
  onStep,
  onToggleFit,
}: {
  bounds: ZoomBounds;
  /** The scale the percentage and the steps are told in. */
  scale: number;
  fitted: boolean;
  onStep: (direction: 1 | -1) => void;
  onToggleFit: () => void;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const fitLabel = fitted ? t("viewer.image.actualSize") : t("viewer.image.fit");
  return (
    <div
      data-zoom-bar
      role="group"
      aria-label={t("viewer.image.zoom")}
      className="floating-surface absolute end-5 top-2 z-20 flex items-center gap-(--row-control-gap) rounded-lg border border-separator bg-overlay p-1 text-xs shadow-(--panel-shadow) max-tablet:end-2"
    >
      <TitledControl title={t("viewer.image.zoomOut")}>
        <Button
          iconOnly
          size="xs"
          variant="ghost"
          className={BAR_BUTTON_CLASS}
          label={t("viewer.image.zoomOut")}
          disabled={!canStep(scale, -1, bounds)}
          onPress={() => onStep(-1)}
          focusOnPress={false}
        >
          <ZoomOut aria-hidden="true" className="size-4" />
        </Button>
      </TitledControl>
      {/* Shown, not spoken: a change made from the keyboard or the buttons is announced on its own. Wide enough for
          "3,200%", so the buttons beside it do not move under the pointer as it grows. */}
      <span aria-hidden="true" className="min-w-12 text-center whitespace-nowrap text-app-muted tabular-nums">
        {formatPercent(language, scale)}
      </span>
      <TitledControl title={t("viewer.image.zoomIn")}>
        <Button
          iconOnly
          size="xs"
          variant="ghost"
          className={BAR_BUTTON_CLASS}
          label={t("viewer.image.zoomIn")}
          disabled={!canStep(scale, 1, bounds)}
          onPress={() => onStep(1)}
          focusOnPress={false}
        >
          <ZoomIn aria-hidden="true" className="size-4" />
        </Button>
      </TitledControl>
      {/* Its name and icon say what pressing does, as a maximize-and-restore button's do: zoomed is a range of scales,
          not one state to show. */}
      <TitledControl title={fitLabel}>
        <Button
          iconOnly
          size="xs"
          variant="ghost"
          className={BAR_BUTTON_CLASS}
          label={fitLabel}
          disabled={fitted && bounds.fit >= 1}
          onPress={onToggleFit}
          focusOnPress={false}
        >
          {fitted ? <Expand aria-hidden="true" className="size-4" /> : <Shrink aria-hidden="true" className="size-4" />}
        </Button>
      </TitledControl>
    </div>
  );
}
