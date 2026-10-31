import { SearchField } from "@heroui/react";
import { FileSearch, FileX } from "lucide-react";
import React, { useEffect, useId, useRef, useState } from "react";
import { ButtonContext } from "react-aria-components";

import { FieldClearButton } from "../components/FieldClearButton";
import { HandOffOnUnmount } from "../components/HandOffOnUnmount";
import { IconToggle } from "../components/IconToggle";
import { PaneToolbar } from "../components/PaneToolbar";
import { StatusMark } from "../components/StatusMark";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useT } from "../i18n/react";
import { isImeKey } from "../imeKey";

export interface FileFilterState {
  filter: string;
  filterOpen: boolean;
  onFilterChange: (filter: string) => void;
  onFilterOpenChange: (open: boolean) => void;
  /** Whether the file-name search also looks at the files Git ignores (the Files tab's alone). */
  includeIgnored: boolean;
  onIncludeIgnoredChange: (include: boolean) => void;
  filterToggleRef: React.RefObject<HTMLButtonElement | null>;
}

export function useFileFilterState(): FileFilterState {
  const [filter, onFilterChange] = useState("");
  const [filterOpen, setFilterOpen] = useState(false);
  const [includeIgnored, onIncludeIgnoredChange] = useState(false);
  const filterToggleRef = useRef<HTMLButtonElement>(null);
  return {
    filter,
    filterOpen,
    onFilterChange,
    onFilterOpenChange: (open) => {
      setFilterOpen(open);
      if (!open) {
        onFilterChange("");
        onIncludeIgnoredChange(false);
      }
    },
    includeIgnored,
    onIncludeIgnoredChange,
    filterToggleRef,
  };
}

/** Opening the field takes focus; closing it hands focus back to its toggle and drops the keyword and
 * the choice to include ignored files. An empty field closes on blur, while a field holding a filter
 * stays visible. With `ignoredToggle`, the field is followed by the toggle that includes the
 * files Git ignores in the search. With `incomplete`, a warning mark inside the field, before its Clear button, says the results
 * may not be every match. */
export function FileFilterField({ state, label, placeholder, contextKey, ignoredToggle = false, incomplete = false }: { state: FileFilterState; label: string; placeholder: string; contextKey?: string; ignoredToggle?: boolean; incomplete?: boolean }): React.ReactElement {
  const t = useT();
  // The field is taken away while it may still hold keyboard focus — the toggle closing it is
  // pressed without taking focus itself — and a removal fires no blur, so focus would fall to
  // `<body>` and the terminal would stop receiving keystrokes; the hand-off puts it on the toggle.
  // This component stays mounted for the hook's check to run in, and its wrapper takes no box of
  // its own (`contents`), so the field is a flex child of the view's column.
  const handoff = useFocusHandoff(() => state.filterToggleRef.current?.focus());
  const input = useRef<HTMLInputElement>(null);
  // Scopes the hand-off to this field's own mark and its popover, not any mark in the window.
  const markId = useId();
  const previous = useRef({ open: state.filterOpen, contextKey });
  useEffect(() => {
    if (state.filterOpen && !previous.current.open && previous.current.contextKey === contextKey) input.current?.focus();
    previous.current = { open: state.filterOpen, contextKey };
  }, [state.filterOpen, contextKey]);
  // An input method's keys are stopped here before the field's own handler and before whatever
  // handles keys above it in the React tree: the composition owns them. react-aria's `useKeyboard`
  // ignores only `isComposing`, so a key an engine reports with `keyCode` 229 alone is held back by
  // this guard alone, or its Escape would clear the field. A listener on the window or the
  // document, such as the pane's Escape (which leaves editable targets alone for this), runs before
  // React's and is beyond this guard's reach.
  const guardComposition = (event: React.KeyboardEvent) => {
    if (isImeKey(event.nativeEvent)) event.stopPropagation();
  };
  // Focus moving within the field is not leaving it — besides the input, only the incomplete-search mark
  // (while `incomplete`) and the include-ignored toggle (with `ignoredToggle` on) take focus, the Clear
  // button being kept out of the tab order and handing focus back to the input. A `focusout` with nothing
  // to go to is also the window itself losing focus, which leaves the field where it is: the user is coming
  // back to it, and taking it away then would move keyboard focus behind their back as well.
  const onBlur = (event: React.FocusEvent<HTMLDivElement>) => {
    if (!document.hasFocus()) return;
    if (state.filter.trim() === "" && !event.currentTarget.contains(event.relatedTarget)) state.onFilterOpenChange(false);
  };
  return (
    <div className="contents" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
      {state.filterOpen && (
        <PaneToolbar onKeyDownCapture={guardComposition} onBlur={onBlur}>
          <SearchField aria-label={label} value={state.filter} onChange={state.onFilterChange} fullWidth variant="secondary" className="min-w-0 flex-1">
            <SearchField.Group>
              <SearchField.SearchIcon />
              <SearchField.Input ref={input} placeholder={placeholder} />
              {/* The mark stands before the Clear button, which keeps the trailing end it has with or without
                  the mark. The field gives its clear button's behaviour to every button inside it
                  through `ButtonContext`; the mark is not that button. When the search completes the mark
                  goes, and focus it held goes to the input rather than to the page. */}
              {incomplete && (
                <>
                  <HandOffOnUnmount from={`[data-status-mark-id="${markId}"]`} to={() => { if (input.current?.isConnected) input.current.focus(); }} />
                  <ButtonContext.Provider value={{}}>
                    <StatusMark tone="warning" markId={markId} label={t("browser.filter.partial")}>{t("browser.filter.partial.details")}</StatusMark>
                  </ButtonContext.Provider>
                </>
              )}
              <FieldClearButton slot="clear" data-slot="search-field-clear-button" className="me-2 shrink-0" />
            </SearchField.Group>
          </SearchField>
          {ignoredToggle && (
            <IconToggle label={t(state.includeIgnored ? "browser.filter.excludeIgnored" : "browser.filter.includeIgnored")} isSelected={state.includeIgnored} onChange={state.onIncludeIgnoredChange}>
              {state.includeIgnored ? <FileSearch aria-hidden="true" className="size-4" /> : <FileX aria-hidden="true" className="size-4" />}
            </IconToggle>
          )}
        </PaneToolbar>
      )}
    </div>
  );
}
