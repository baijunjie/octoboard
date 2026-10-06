import { Button, Chip, Popover, SearchField } from "@heroui/react";
import { BrushCleaning, Search } from "lucide-react";
import { setInteractionModality } from "react-aria";
import React, { useRef, useState } from "react";

import { TitledControl } from "../components/TitledControl";
import { usePointerFocusReturn } from "../components/usePointerFocusReturn";
import { useT } from "../i18n/react";
import { RowIconButton } from "./rows";

/** The search button by the Projects heading: it opens a popover with a field whose keyword
 * filters the project list as it is typed. Enter or Escape closes it and keeps the keyword. While a
 * keyword is in force, a button beside it clears it. */
export function ProjectFilterButton({
  keyword,
  onChange,
}: {
  keyword: string;
  onChange: (keyword: string) => void;
}): React.ReactElement {
  const t = useT();
  const [open, setOpen] = useState(false);
  // The button never takes focus on a press, and react-aria hands focus back to it when the popover
  // closes, which would pull it off the terminal; the element the press came from gets it back.
  const pointerFocus = usePointerFocusReturn();
  const buttonHolder = useRef<HTMLDivElement>(null);
  const clearHolder = useRef<HTMLSpanElement>(null);

  const onOpenChange = (isOpen: boolean) => {
    setOpen(isOpen);
    const restore = pointerFocus.opened(isOpen);
    if (isOpen || !restore) return;
    // react-aria hands focus back to the button a moment after the popover closes; it is passed on
    // the moment it arrives, to where it came from, or dropped when nothing had it (no terminal
    // yet) rather than left on the button showing a ring nobody asked for.
    const holder = buttonHolder.current;
    if (!holder) return;
    const passOn = () => {
      if (restore.previous?.isConnected) restore.previous.focus();
      else (document.activeElement as HTMLElement | null)?.blur();
    };
    if (holder.contains(document.activeElement)) {
      passOn();
      return;
    }
    const listeners = new AbortController();
    holder.addEventListener("focusin", passOn, { once: true, signal: listeners.signal });
    setTimeout(() => listeners.abort(), 500);
  };

  const clear = () => {
    // The clear button goes away with the keyword, taking keyboard focus with it.
    const hadFocus = clearHolder.current?.contains(document.activeElement);
    onChange("");
    if (hadFocus) {
      setInteractionModality("keyboard");
      buttonHolder.current?.querySelector("button")?.focus();
    }
  };

  return (
    <div className="flex items-center gap-0.5">
      {keyword.trim() !== "" && (
        <span ref={clearHolder} className="flex">
          <RowIconButton icon={BrushCleaning} label={t("sidebar.filter.clear")} onPress={clear} />
        </span>
      )}
      <div ref={buttonHolder} className="flex" onPointerDownCapture={pointerFocus.onPointerDownCapture}>
        <Popover isOpen={open} onOpenChange={onOpenChange}>
          <TitledControl title={t("sidebar.filter.open")}>
            <Button
              isIconOnly
              size="sm"
              variant="ghost"
              aria-label={t("sidebar.filter.open")}
              preventFocusOnPress
              className="size-6 min-w-0 rounded-md text-muted hover:text-foreground"
            >
              <Search aria-hidden="true" className="size-4" />
            </Button>
          </TitledControl>
          <Popover.Content placement="bottom end" className="w-64">
            <Popover.Dialog aria-label={t("sidebar.filter.open")}>
              {/* The field clears itself on Escape; here Escape closes the popover and keeps the keyword. */}
              <div
                onKeyDownCapture={(e) => {
                  if (e.key !== "Escape") return;
                  e.preventDefault();
                  e.stopPropagation();
                  onOpenChange(false);
                }}
              >
                <SearchField
                  aria-label={t("sidebar.filter.field")}
                  value={keyword}
                  onChange={onChange}
                  onSubmit={() => onOpenChange(false)}
                  autoFocus
                  fullWidth
                  variant="secondary"
                >
                  <SearchField.Group>
                    <SearchField.SearchIcon />
                    <SearchField.Input placeholder={t("sidebar.filter.placeholder")} />
                    <SearchField.ClearButton />
                  </SearchField.Group>
                </SearchField>
              </div>
            </Popover.Dialog>
          </Popover.Content>
        </Popover>
      </div>
    </div>
  );
}

/** The keyword in force, beside the Projects heading. Cleared from the button beside the search
 * button (`ProjectFilterButton`), not from the chip. */
export function ProjectFilterTag({ keyword }: { keyword: string }): React.ReactElement {
  return (
    <Chip size="sm" variant="soft" className="min-w-0 max-w-full overflow-hidden">
      <Chip.Label dir="auto" className="truncate">
        {keyword}
      </Chip.Label>
    </Chip>
  );
}

/** Whether `name` matches the filter keyword: a case-insensitive substring; an empty keyword
 * matches everything. */
export function matchesFilter(name: string, keyword: string): boolean {
  const needle = keyword.trim().toLocaleLowerCase();
  return needle === "" || name.toLocaleLowerCase().includes(needle);
}
