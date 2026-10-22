import React from "react";

/** Shared shape and tints for icon controls in either side pane and the file viewer. */
export const ROW_ICON_BUTTON_BASE_CLASS = "sidebar-fills shrink-0 rounded-md bg-transparent text-app-muted";

/** The hover tint of an icon control, left off one that cannot act. */
const ROW_ICON_BUTTON_HOVER_CLASS = "hover:bg-default hover:text-foreground";

export const ROW_ICON_BUTTON_CLASS = `${ROW_ICON_BUTTON_BASE_CLASS} ${ROW_ICON_BUTTON_HOVER_CLASS}`;

/** Shared spacing for the icons at a row or header's end. */
export function RowIconGroup({ children, className = "" }: { children: React.ReactNode; className?: string }): React.ReactElement {
  return <div className={`flex shrink-0 items-center gap-(--row-control-gap) ${className}`}>{children}</div>;
}

/** A glyph or marker occupies the shared leading or trailing icon column. */
export function RowIconSlot({ children, placement = "trailing" }: { children: React.ReactNode; placement?: "leading" | "trailing" }): React.ReactElement {
  return <div className={`flex shrink-0 items-center justify-center ${placement === "leading" ? "w-(--row-leading-icon-size)" : "w-(--row-control-size)"}`}>{children}</div>;
}

/** Text keeps its starting column even when a line has no leading glyph. */
export function RowIconLine({ icon, children, className = "" }: { icon?: React.ReactNode; children: React.ReactNode; className?: string }): React.ReactElement {
  return (
    <div className={`flex min-w-0 items-center gap-(--row-leading-icon-gap) ${className}`}>
      <RowIconSlot placement="leading">{icon}</RowIconSlot>
      {children}
    </div>
  );
}

/** The controls at a row's end show while it is hovered, focused, selected or has its menu open.
 * Hidden actions stay in the tab order, so reaching one reveals it. `fixed` markers and pins stay
 * outside the animated clip. The clip contains its leading gap too (`--row-controls-gap` cancels
 * the parent gap), leaving no empty column beside fixed icons when actions are hidden. Two buttons plus both gaps set the animation cap;
 * keyboard focus removes clipping so its ring can paint outside the box. A `reserve`d slot keeps
 * its width while the actions are hidden and only fades them in, for a row whose other content is
 * laid out against its end and must not slide when they show. On a device whose primary input
 * cannot hover (a phone, a tablet's touch screen) they always show, as nothing there would reveal
 * them before a tap. */
export function RowControls({
  children,
  fixed,
  always,
  reserve,
  className = "",
}: {
  children: React.ReactNode;
  /** Persistent trailing markers and buttons, laid out with the same spacing as the actions. */
  fixed?: React.ReactNode;
  always?: boolean;
  reserve?: boolean;
  /** Layout the row gives its controls (`ms-auto` to sit at the end of a row with no filler). */
  className?: string;
}): React.ReactElement {
  return (
    <div className={`-ms-(--row-controls-gap,0px) flex shrink-0 items-center ${className}`}>
      {fixed && <RowIconGroup className="ps-(--row-controls-gap,0px)">{fixed}</RowIconGroup>}
      <div
        className={`flex shrink-0 transition-[opacity,max-width] duration-150 motion-reduce:transition-none ${
          always
            ? ""
            : reserve
              ? "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 group-data-selected:opacity-100 group-data-current:opacity-100 has-[[aria-expanded=true]]:opacity-100 [@media(hover:none)]:opacity-100"
              : "max-w-0 overflow-hidden opacity-0 [@media(hover:none)]:max-w-none [@media(hover:none)]:overflow-visible [@media(hover:none)]:opacity-100 group-hover:max-w-[calc(var(--row-controls-max-width)+var(--row-controls-gap,0px))] group-hover:opacity-100 group-focus-within:max-w-[calc(var(--row-controls-max-width)+var(--row-controls-gap,0px))] group-focus-within:overflow-visible group-focus-within:opacity-100 group-data-selected:max-w-[calc(var(--row-controls-max-width)+var(--row-controls-gap,0px))] group-data-selected:opacity-100 group-data-current:max-w-[calc(var(--row-controls-max-width)+var(--row-controls-gap,0px))] group-data-current:opacity-100 has-[[aria-expanded=true]]:max-w-[calc(var(--row-controls-max-width)+var(--row-controls-gap,0px))] has-[[aria-expanded=true]]:opacity-100"
        }`}
      >
        <RowIconGroup className={fixed ? "ps-(--row-control-gap)" : "ps-(--row-controls-gap,0px)"}>{children}</RowIconGroup>
      </div>
    </div>
  );
}
