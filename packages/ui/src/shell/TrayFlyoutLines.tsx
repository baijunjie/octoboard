import { ChevronDown, ChevronRight } from "lucide-react";
import React from "react";

import { joinPhrases } from "../i18n/joinPhrases";
import type { Language } from "../i18n/languages";
import type { StatusItemIcon, StatusItemSubmenuItem, TrayFlyoutLine, TrayFlyoutQuota } from "../platform";
import { StatusBadge } from "../components/status/StatusBadge";
import { Meter, MenuItem, MenuSection, MenuSeparator } from "../primitives";

/** What a rendering of lines needs from the page: the one line opened in place, and the language the phrases of a line
 * without its own label are joined in. */
export interface LineContext {
  expanded: string | undefined;
  language: Language;
}

/** The ring of a row or tab under Windows high contrast, where the box shadows the library draws its focus with are
 * dropped: an outline in the system's highlight colour. */
export const FORCED_FOCUS_CLASS =
  "forced-colors:ui-keyboard-focus:outline-2 forced-colors:ui-keyboard-focus:outline-solid forced-colors:ui-keyboard-focus:-outline-offset-2 forced-colors:ui-keyboard-focus:outline-[Highlight]";

const ROW_CLASS = `w-full gap-2 rounded-lg min-h-8 py-1 ${FORCED_FOCUS_CLASS}`;
/** A row that shows information: it is never highlighted and keeps full strength, though it is a disabled item (so it
 * is reached and read with the arrow keys but never chosen, as the macOS menu's information lines are). */
const INFO_CLASS = `${ROW_CLASS} cursor-default ui-disabled:opacity-100 hover:bg-transparent`;
const DETAIL_CLASS = `${INFO_CLASS} ps-9`;
/** The separator between two accounts of an opened quota row, set in to where an account's content begins (`ps-9`). */
const DETAIL_SEPARATOR_CLASS = "ms-9 w-auto";

const rgb = (colour: readonly number[]) => `rgb(${colour[0]} ${colour[1]} ${colour[2]}${colour[3] === undefined ? "" : ` / ${colour[3] / 255}`})`;
/** The custom property the bars take their fill from (`TrayFlyout.css`). */
const fillStyle = (colour: readonly number[] | undefined): React.CSSProperties | undefined =>
  colour && ({ "--tray-flyout-fill": rgb(colour) } as React.CSSProperties);

/** A line's picture, 18px: a template one is drawn in the text colour through its alpha, as a system menu tints its
 * own; any other as it is. Decoration, hidden from assistive technology: the label is what is read. */
function Picture({ icon }: { icon: StatusItemIcon | undefined }): React.ReactElement | null {
  if (!icon) return null;
  const src = `data:image/png;base64,${icon.png}`;
  if (icon.template) {
    return <span aria-hidden="true" className="tray-flyout-picture" style={{ maskImage: `url(${src})`, WebkitMaskImage: `url(${src})` }} />;
  }
  return <img alt="" aria-hidden="true" src={src} width={18} height={18} className="size-[18px] shrink-0" />;
}

/** A bar of `fraction` (0 to 1) in `colour`, over the quota track, as wide as its row; `thin` is the slim one beneath
 * another. The row's own label carries its facts, so the bar is hidden from assistive technology. */
function Bar({ name, fraction, colour, thin }: { name: string; fraction: number; colour: readonly number[] | undefined; thin?: boolean }): React.ReactElement {
  return (
    <div aria-hidden="true">
      <Meter label={name} value={Math.round(Math.min(1, Math.max(0, fraction)) * 100)} className="tray-flyout-bar gap-0" style={fillStyle(colour)} trackClassName={`tray-flyout-track ${thin ? "h-1" : "h-1.5"}`} />
    </div>
  );
}

/** The compact bars on a quota line's opener: the 5-hour remainder and, thinner beneath it, the weekly one. */
function OpenerBars({ quota, colour }: { quota: TrayFlyoutQuota; colour: readonly number[] | undefined }): React.ReactElement | null {
  if (quota.quota_remaining === undefined) return null;
  const track = (percent: number, thin: boolean) => (
    <span className={`tray-flyout-track quota-bar-track block overflow-hidden rounded-full ${thin ? "h-[3px]" : "h-1"}`}>
      <span className="tray-flyout-mini-fill block h-full rounded-full" style={{ width: `${percent}%` }} />
    </span>
  );
  return (
    <span aria-hidden="true" className="flex w-8 shrink-0 flex-col gap-0.5" style={fillStyle(colour)}>
      {track(quota.quota_remaining, false)}
      {quota.quota_weekly_remaining !== undefined && track(quota.quota_weekly_remaining, true)}
    </span>
  );
}

/** The colour of the agent's bars: the first window or meter that names one. */
function quotaColour(items: StatusItemSubmenuItem[]): readonly number[] | undefined {
  for (const item of items) {
    if ((item.kind === "window" || item.kind === "meter") && item.fill_colour) return item.used_up && item.used_up_colour ? item.used_up_colour : item.fill_colour;
  }
  return undefined;
}

function Chevron({ expanded }: { expanded: boolean }): React.ReactElement {
  return expanded ? <ChevronDown aria-hidden="true" className="size-3.5 shrink-0 text-app-muted" /> : <ChevronRight aria-hidden="true" className="size-3.5 shrink-0 text-app-muted rtl:-scale-x-100" />;
}

/** The accessible name of a header row of a quota submenu: its own phrase, else its parts joined as a list. */
function headerName(item: Extract<StatusItemSubmenuItem, { kind: "header" }>, language: Language): string {
  return item.label ?? joinPhrases(language, [item.title, item.suffix, item.status?.label, item.detail].filter((part): part is string => Boolean(part)));
}

/** One row of a quota submenu, shown in place under its opener. None of them is chosen. A separator sets one account
 * off from the next, as in the macOS submenu. */
function detailRow(item: StatusItemSubmenuItem, id: string, owner: string, language: Language, colour: readonly number[] | undefined): React.ReactElement | undefined {
  const common = { id, disabled: true, className: DETAIL_CLASS, "data-flyout-owner": owner } as const;
  switch (item.kind) {
    case "separator":
      return <MenuSeparator key={id} className={DETAIL_SEPARATOR_CLASS} />;
    case "note":
      return (
        <MenuItem key={id} {...common} textValue={item.text}>
          <span className="min-w-0 text-sm text-app-muted [overflow-wrap:anywhere]">{item.text}</span>
        </MenuItem>
      );
    case "header": {
      const { status } = item;
      const name = headerName(item, language);
      return (
        <MenuItem key={id} {...common} textValue={name} label={name}>
          <span className="flex w-full min-w-0 items-baseline gap-2 text-sm">
            <span className="min-w-0 font-medium [overflow-wrap:anywhere]">{item.title}</span>
            {item.suffix && <span className="text-app-muted">{item.suffix}</span>}
            {status && (
              <StatusBadge color="warning" variant="soft" className="tray-flyout-status">
                {status.label}
              </StatusBadge>
            )}
            {item.detail && <span className="ms-auto shrink-0 text-app-muted">{item.detail}</span>}
          </span>
        </MenuItem>
      );
    }
    case "window": {
      const bar = item.used_up ? (item.used_up_colour ?? colour) : (item.fill_colour ?? colour);
      return (
        <MenuItem key={id} {...common} textValue={item.label} label={item.label}>
          <span className="flex w-full min-w-0 flex-col gap-1 text-sm">
            <span className="flex items-baseline justify-between gap-3">
              <span className="min-w-0 font-medium [overflow-wrap:anywhere]">{item.name}</span>
              <span className="shrink-0 text-app-muted">{item.detail}</span>
            </span>
            <Bar name={item.name} fraction={item.fraction} colour={bar} />
          </span>
        </MenuItem>
      );
    }
    case "meter": {
      const bar = item.used_up ? (item.used_up_colour ?? colour) : (item.fill_colour ?? colour);
      return (
        <MenuItem key={id} {...common} textValue={item.label} label={item.label}>
          <span className="flex w-full min-w-0 flex-col gap-1 text-sm">
            <span className="flex items-baseline justify-between gap-3">
              <span className="min-w-0 font-medium [overflow-wrap:anywhere]">{item.session.name}</span>
              <span className="shrink-0 text-app-muted">{item.session.detail}</span>
            </span>
            <Bar name={item.session.name} fraction={item.session.fraction} colour={bar} />
            <Bar name={item.weekly.name} fraction={item.weekly.fraction} colour={bar} thin />
            <span className="flex items-baseline justify-between gap-3 text-app-muted">
              <span className="min-w-0 [overflow-wrap:anywhere]">{item.weekly.name}</span>
              <span className="shrink-0">{item.weekly.detail}</span>
            </span>
          </span>
        </MenuItem>
      );
    }
  }
}

/** A line the user can choose, or one that only says something. */
function plainRow(line: Extract<TrayFlyoutLine, { kind: "item" | "note" }>, key: string): React.ReactElement {
  if (line.kind === "item") {
    return (
      <MenuItem key={key} id={line.id} textValue={line.label} className={ROW_CLASS} data-flyout-choosable="">
        <Picture icon={line.icon} />
        <span className="min-w-0 text-sm [overflow-wrap:anywhere]">{line.label}</span>
      </MenuItem>
    );
  }
  return (
    <MenuItem key={key} id={key} disabled textValue={line.label} className={INFO_CLASS}>
      <Picture icon={line.icon} />
      <span className="min-w-0 text-sm text-app-muted [overflow-wrap:anywhere]">{line.label}</span>
    </MenuItem>
  );
}

/** The id of the opener of the line at `at`, which a press, Right and Left act on, and which the page tells apart from
 * the id of a line the shell is told. */
export const openerId = (at: string): string => `open:${at}`;

/**
 * The menu's items for `lines`: a heading takes the item and note lines after it into a section; a quota line is an
 * opener, with its rows after it while `context.expanded` names it. `scope` keeps the ids of the lines of
 * one list from those of another (the panel's, the shared lines').
 */
export function lineRows(lines: TrayFlyoutLine[], scope: string, context: LineContext): React.ReactElement[] {
  const rows: React.ReactElement[] = [];
  for (let index = 0; index < lines.length; index++) {
    const line = lines[index];
    const at = `${scope}${index}`;
    switch (line.kind) {
      case "heading": {
        const run: React.ReactElement[] = [];
        while (index + 1 < lines.length) {
          const next = lines[index + 1];
          if (next.kind !== "item" && next.kind !== "note") break;
          index++;
          run.push(plainRow(next, `${scope}${index}`));
        }
        rows.push(
          <MenuSection
            key={at}
            headingClassName="flex items-center gap-2 px-2 pt-2 pb-1 text-xs font-medium text-app-muted"
            heading={
              <>
                <Picture icon={line.icon} />
                <span className="[overflow-wrap:anywhere]">{line.label}</span>
              </>
            }
          >
            {run}
          </MenuSection>,
        );
        break;
      }
      case "item":
      case "note":
        rows.push(plainRow(line, at));
        break;
      case "separator":
        rows.push(<MenuSeparator key={at} />);
        break;
      case "quota": {
        const quota = line.submenu;
        const id = openerId(at);
        const open = context.expanded === id;
        const colour = quotaColour(quota.items);
        rows.push(
          <MenuItem key={id} id={id} textValue={quota.title} label={quota.quota_label} expanded={open} className={ROW_CLASS} data-flyout-choosable="" data-flyout-opener={id}>
            <Picture icon={quota.icon} />
            <span className="min-w-0 text-sm [overflow-wrap:anywhere]">{quota.title}</span>
            <span className="ms-auto flex items-center gap-2">
              <OpenerBars quota={quota} colour={colour} />
              <Chevron expanded={open} />
            </span>
          </MenuItem>,
        );
        if (open) {
          quota.items.forEach((item, n) => {
            const row = detailRow(item, `${id}/${n}`, id, context.language, colour);
            if (row) rows.push(row);
          });
          // The block's end, where another line follows it that no separator of its own sets off.
          const next = lines[index + 1];
          if (next && next.kind !== "separator") rows.push(<MenuSeparator key={`${id}/end`} />);
        }
        break;
      }
    }
  }
  return rows;
}
