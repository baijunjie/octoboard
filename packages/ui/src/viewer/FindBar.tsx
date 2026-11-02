import { Input, ToggleButton } from "@heroui/react";
import { CaseSensitive, ChevronDown, ChevronUp, Regex, Search, SearchX, WholeWord, X } from "lucide-react";
import React, { useId } from "react";

import { StatusAnnouncer, AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { Button } from "../components/controls/Button";
import { IconToggle } from "../components/controls/IconToggle";
import { ROW_ICON_BUTTON_CLASS } from "../components/controls/RowControls";
import { TitledControl } from "../components/hint/TitledControl";
import { isImeKey } from "../input/imeKey";
import { useT } from "../i18n/react";
import type { FindOptions } from "./find";

/** How a search stands, for the count beside the query. */
export type FindStatus =
  | { kind: "idle" }
  | { kind: "invalid" }
  | { kind: "none" }
  | { kind: "found"; current: number; total: number; capped: boolean };

const OPTIONS: { option: keyof FindOptions; label: "viewer.find.matchCase" | "viewer.find.wholeWord" | "viewer.find.regex"; Icon: typeof Regex }[] = [
  { option: "matchCase", label: "viewer.find.matchCase", Icon: CaseSensitive },
  { option: "wholeWord", label: "viewer.find.wholeWord", Icon: WholeWord },
  { option: "regex", label: "viewer.find.regex", Icon: Regex },
];

/** A toggle of the find bar: pressed is drawn as a fill and an inset outline, so it is not told by
 * colour alone. The outline is an `inset-ring`, which shares no variable with the focus ring (`ring`),
 * so a keyboard focus ring is drawn over it rather than replaced by it. A press keeps focus where
 * it was, in the query. */
const TOGGLE_CLASS = `row-icon-control ${ROW_ICON_BUTTON_CLASS} data-[selected=true]:bg-default data-[selected=true]:text-foreground data-[selected=true]:inset-ring-1 data-[selected=true]:inset-ring-app-muted`;

/**
 * The find bar of the viewer: the query, the count of matches, the three switches, Previous and Next,
 * and Close. It is controlled; `useFind` owns what the search does. Enter steps to the next match
 * and Shift+Enter to the previous one. Escape is the viewer's to take (`useFind`), so that it also
 * works with focus in the code.
 *
 * The bar stays on one row down to a phone's width (the gap it leaves in the scrolled text is one
 * row tall), so the count drawn beside the query is short: "3/12", "0/0", "Invalid". It is
 * hidden from assistive technology and describes the query; what is announced is the full sentence,
 * from a status region of its own that stays mounted.
 */
export function FindBar({
  query,
  options,
  status,
  inputRef,
  barRef,
  focusHandoff,
  onQueryChange,
  onOptionsChange,
  onStep,
  onClose,
}: {
  query: string;
  options: FindOptions;
  status: FindStatus;
  inputRef: React.Ref<HTMLInputElement>;
  barRef: React.Ref<HTMLDivElement>;
  /** Hands focus on if the bar is taken away while it holds focus (`useFocusHandoff`). */
  focusHandoff: Pick<React.ComponentProps<"div">, "onFocus" | "onBlur">;
  onQueryChange: (query: string) => void;
  onOptionsChange: (options: FindOptions) => void;
  onStep: (direction: "next" | "previous") => void;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const countId = useId();
  const invalid = status.kind === "invalid";
  const params = status.kind === "found" ? { current: status.current, total: status.total } : undefined;
  const count =
    status.kind === "found"
      ? t(status.capped ? "viewer.find.countShortMore" : "viewer.find.countShort", params!)
      : status.kind === "none"
        ? t("viewer.find.countShort", { current: 0, total: 0 })
        : status.kind === "invalid"
          ? t("viewer.find.invalidShort")
          : "";
  const spoken =
    status.kind === "found"
      ? t(status.capped ? "viewer.find.countMore" : "viewer.find.count", params!)
      : status.kind === "none"
        ? t("viewer.find.none")
        : status.kind === "invalid"
          ? t("viewer.find.invalid")
          : undefined;
  const stepDisabled = status.kind !== "found";
  return (
    <div
      ref={barRef}
      role="search"
      aria-label={t("viewer.find")}
      className="floating-surface absolute end-5 top-2 z-20 flex max-w-[calc(100%-2.5rem)] items-center gap-(--row-control-gap) rounded-lg border border-separator bg-overlay p-0.5 text-xs shadow-(--panel-shadow) max-phone:end-2 max-phone:max-w-[calc(100%-1rem)]"
      {...focusHandoff}
    >
      {/* Left as typed: a leading or trailing space is part of what is searched for, so the field does
          not go through `useTrimmedField`. */}
      <Input
        ref={inputRef}
        data-1p-ignore
        aria-label={t("viewer.find")}
        aria-describedby={countId}
        aria-invalid={invalid || undefined}
        data-invalid={invalid ? "true" : undefined}
        value={query}
        placeholder={t("viewer.find.placeholder")}
        dir="auto"
        spellCheck={false}
        autoComplete="off"
        autoCapitalize="off"
        autoCorrect="off"
        variant="secondary"
        className="h-7 w-48 min-w-12 shrink text-sm"
        onChange={(event) => onQueryChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key !== "Enter" || isImeKey(event.nativeEvent)) return;
          event.preventDefault();
          onStep(event.shiftKey ? "previous" : "next");
        }}
      />
      {/* The ratio reads left to right in any language ("1/5,000+" would show its plus first); the word does not. */}
      <span id={countId} aria-hidden="true" dir={invalid ? undefined : "ltr"} className={`shrink-0 px-0.5 text-center whitespace-nowrap tabular-nums ${invalid ? "text-(--tint-danger-fg)" : "text-app-muted"}`}>
        {count}
      </span>
      <StatusAnnouncer text={spoken} firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} />
      {OPTIONS.map(({ option, label, Icon }) => (
        <TitledControl key={option} title={t(label)}>
          <ToggleButton
            isIconOnly
            size="sm"
            variant="ghost"
            className={TOGGLE_CLASS}
            aria-label={t(label)}
            isSelected={options[option]}
            onChange={(selected) => onOptionsChange({ ...options, [option]: selected })}
            preventFocusOnPress
          >
            <Icon aria-hidden="true" className="size-4" />
          </ToggleButton>
        </TitledControl>
      ))}
      <TitledControl title={t("viewer.find.previous")}>
        <Button isIconOnly size="xs" variant="ghost" className={`row-icon-control ${ROW_ICON_BUTTON_CLASS}`} aria-label={t("viewer.find.previous")} isDisabled={stepDisabled} onPress={() => onStep("previous")} preventFocusOnPress>
          <ChevronUp aria-hidden="true" className="size-4" />
        </Button>
      </TitledControl>
      <TitledControl title={t("viewer.find.next")}>
        <Button isIconOnly size="xs" variant="ghost" className={`row-icon-control ${ROW_ICON_BUTTON_CLASS}`} aria-label={t("viewer.find.next")} isDisabled={stepDisabled} onPress={() => onStep("next")} preventFocusOnPress>
          <ChevronDown aria-hidden="true" className="size-4" />
        </Button>
      </TitledControl>
      <TitledControl title={t("viewer.find.close")}>
        <Button isIconOnly size="xs" variant="ghost" className={`row-icon-control ${ROW_ICON_BUTTON_CLASS}`} aria-label={t("viewer.find.close")} onPress={onClose}>
          <X aria-hidden="true" className="size-4" />
        </Button>
      </TitledControl>
    </div>
  );
}

/** The header's view control that opens and closes the find bar, as ⌘F and Escape do. Its label says
 * what pressing does and its icon whether the bar is open, as the other view controls'. */
export function FindToggle({ open, onToggle, ref }: { open: boolean; onToggle: () => void; ref?: React.Ref<HTMLButtonElement> }): React.ReactElement {
  const t = useT();
  return (
    <IconToggle ref={ref} label={t(open ? "viewer.find.close" : "viewer.find")} isSelected={open} expanded={open} onChange={onToggle}>
      {open ? <SearchX aria-hidden="true" className="size-4" /> : <Search aria-hidden="true" className="size-4" />}
    </IconToggle>
  );
}
