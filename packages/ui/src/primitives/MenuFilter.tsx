import React, { createContext, useContext, useEffect, useState } from "react";
import { flushSync } from "react-dom";
import { Autocomplete } from "react-aria-components";

import { setPointerModality } from "./focus";
import { SearchField } from "./TextField";

/** Whether an item's text contains what was typed, ignoring case and the spaces around it: the test a `MenuFilter`
 * applies to its items, for a caller that has to tell from outside the menu whether any item remains. */
export function containsText(text: string, typed: string): boolean {
  return text.toLowerCase().includes(typed.trim().toLowerCase());
}

/** Whether the menu is narrowed by a `MenuFilter`, which a filtered menu's items and their content read. */
const MenuFilterContext = createContext(false);

export function useMenuFiltered(): boolean {
  return useContext(MenuFilterContext);
}

/** Around what a filtered menu opens that the filter does not narrow (a submenu's content, which reads the context
 * through the portal it renders in): its items are those of an unfiltered menu. */
export function UnfilteredMenu({ children }: { children: React.ReactNode }): React.ReactElement {
  return <MenuFilterContext.Provider value={false}>{children}</MenuFilterContext.Provider>;
}

// The filter field's row reads as the menu's own first row rather than a form control set into it, the user's
// confirmed decision (a departure from HeroUI's search field look, see "Conformance level and exceptions" in
// `docs/ui-standard.md`): no fill, border or focus ring (the caret shows where typing goes), its glyph and text on the
// columns of the items' icons and labels, and a rule under it. The popover is the scroller, so the row sticks to its top
// on the popover's own fill. The menu's 6px padding and the items' 10px start padding and 12px gap put the search glyph
// and the text on those columns; the input takes the rest of the row, and the clear button's glyph ends as far from the
// popover's end edge as the search glyph starts from its start edge.
const FILTER_ROW_CLASS = [
  "sticky top-0 z-10 bg-(--overlay) px-1.5 pt-1.5 pb-1 border-b border-(--separator)",
  "[&_[data-slot=search-field-group]]:h-9 [&_[data-slot=search-field-group]]:gap-0 [&_[data-slot=search-field-group]]:ps-2.5 [&_[data-slot=search-field-group]]:pe-1.25",
  "[&_[data-slot=search-field-group]]:bg-transparent [&_[data-slot=search-field-group]]:border-0 [&_[data-slot=search-field-group]]:[box-shadow:none]! [&_[data-slot=search-field-group]]:outline-none",
  "[&_[data-slot=search-field-search-icon]]:ms-0 [&_[data-slot=search-field-search-icon]]:me-3 [&_[data-slot=search-field-search-icon]]:size-4 [&_[data-slot=search-field-search-icon]]:shrink-0 [&_[data-slot=search-field-search-icon]]:text-app-muted",
  "[&_[data-slot=search-field-input]]:flex-1 [&_[data-slot=search-field-input]]:px-0 [&_[data-slot=search-field-input]]:text-sm",
  "[&_[data-slot=search-field-clear-button]]:ms-2 [&_[data-slot=search-field-clear-button]]:me-0",
].join(" ");

/** The popover's scroll padding where it holds a `MenuFilter`. The row is 6px + 36px + 4px + its 1px rule tall: 47px,
 * and 5px more is the usual few pixels below it. */
export const FILTER_SCROLL_PADDING_CLASS = "scroll-pt-13";

/** What a filtered menu's items wear: the item react-aria has focused, the one the field's Enter would choose, is the
 * only one marked, by the hover fill and not the keyboard ring, since focus stays in the field and a ring on an item
 * would read as a second focus. HeroUI's own hover fill is cancelled on the other items: the pointer moves the focus
 * to the item it is over, so a hovered item that is not the focused one is one the keyboard has since moved away from.
 * The focused item keeps its fill under the pointer even where it is disabled (a disabled row is cancelled its hover
 * fill elsewhere), since the arrow keys still walk onto it and it stays what Enter is tried on. Also the user's
 * confirmed decision. */
export const FILTERED_ITEM_CLASS = "ui-focused:bg-(--default) ui-focused:hover:bg-(--default)! ui-focused:[box-shadow:none]! not-ui-focused:hover:bg-transparent not-ui-focused:ui-hovered:bg-transparent";

/** react-aria's autocomplete around a `SearchField` and a `MenuList`, for a menu with a field to narrow its items by
 * the text they show: the field, the menu's first row, keeps focus while the arrow keys move through the items that
 * remain, and Enter chooses the one that is highlighted (marked by the hover fill). `query` and `onQueryChange` hold
 * the field's text, which the caller resets for each opening; `label` names the field, `clearButton` is the layer
 * `Button` that clears it, and `fieldMode` is how the field takes focus as the menu opens: for `"typing"` as any field
 * does, for `"browsing"` read-only, so that focus is on a named control the arrow keys and Enter work from without
 * raising an on-screen keyboard; a tap on the field (or a key that types, an input method's included) lets it take text.
 *
 * The pointer moves the highlight too, so that one row is filled at a time: react-aria leaves a hovered item alone
 * while the keyboard was the last input, which would leave the highlighted row and the hovered one filled alike.
 *
 * The items do not move as the text changes: the list's motion ("Dynamic collections") is left out, the user's
 * confirmed decision. A filtered menu takes no single-letter shortcuts, since react-aria hands each key typed in the
 * field on to the highlighted item; its items show and announce none, and the caller runs none. Escape
 * clears the text first, and closes the menu the second time. */
export function MenuFilter({ query, onQueryChange, label, placeholder, clearButton, fieldMode, children }: {
  query: string;
  onQueryChange: (query: string) => void;
  label: string;
  placeholder: string;
  clearButton: React.ReactElement;
  fieldMode: "typing" | "browsing";
  children: React.ReactNode;
}): React.ReactElement {
  const [readOnly, setReadOnly] = useState(fieldMode === "browsing");
  // A tap raises the keyboard only where it focuses the input, so the input is let go of and focused again once it takes text.
  const startTyping = (input: HTMLInputElement) => {
    flushSync(() => setReadOnly(false));
    input.blur();
    input.focus();
  };
  // Before react-aria reads the hover: it moves the highlight to a hovered item only while the pointer was the last
  // input, and it reads that as the pointer enters the item, ahead of the move that would set it. React raises that
  // enter from `pointerout` when the pointer comes from another element of the page, from `pointerover` otherwise, so
  // both are heard, on the document in the capture phase to come before React's own listeners.
  useEffect(() => {
    const onPointer = (event: PointerEvent) => { if (event.pointerType === "mouse") setPointerModality(); };
    document.addEventListener("pointerout", onPointer, true);
    document.addEventListener("pointerover", onPointer, true);
    return () => {
      document.removeEventListener("pointerout", onPointer, true);
      document.removeEventListener("pointerover", onPointer, true);
    };
  }, []);
  return (
    <MenuFilterContext.Provider value>
      <Autocomplete inputValue={query} onInputChange={onQueryChange} filter={containsText}>
        <div
          className={FILTER_ROW_CLASS}
          onClick={(event) => { if (readOnly && event.target instanceof HTMLInputElement) startTyping(event.target); }}
          onKeyDownCapture={(event) => {
            if (!(event.target instanceof HTMLInputElement)) return;
            // react-aria's field handler forwards each key to the highlighted item as a new event built from the native
            // one; WebKit's copy of Backspace still carries its legacy key identifier, whose default action on an
            // element that takes no text is to go back in the history. Neither key goes on to that handler: an editable
            // field deletes natively, and a read-only one has nothing to delete, so its own default is cancelled too.
            if (event.key === "Backspace" || event.key === "Delete") {
              event.stopPropagation();
              if (readOnly) event.preventDefault();
            }
            const ime = event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229 || event.key === "Process";
            // Ctrl with Alt is AltGr on Windows, and Option alone types on macOS: both give a character to type.
            const types = event.key.length === 1 && !event.metaKey && !(event.ctrlKey && !event.altKey);
            if (readOnly && (ime || types)) setReadOnly(false);
          }}
        >
          {/* The autocomplete feeds the field its value and what is typed. */}
          <SearchField label={label} placeholder={placeholder} clearButton={clearButton} autoFocus readOnly={readOnly} fullWidth />
        </div>
        {children}
      </Autocomplete>
    </MenuFilterContext.Provider>
  );
}
