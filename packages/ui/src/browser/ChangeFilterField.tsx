import { SearchField } from "@heroui/react";
import React, { useEffect, useRef } from "react";

import { useFocusHandoff } from "../components/useFocusHandoff";
import { useT } from "../i18n/react";
import { isImeKey } from "../imeKey";
import type { ChangeViewState } from "./changeView";

/**
 * The field whose text narrows a change list to the changes whose file name contains it
 * (`filterChanges`), between the view's selectors and the list itself. It is on screen only while
 * the tab row's filter toggle has it open, and it is a view's own, not the list's: it is there
 * whichever state the view is in, so the toggle always has a field to open, and the text it holds
 * is the Git mode's, shared by both views.
 *
 * It takes keyboard focus as the toggle opens it, but not when the view is mounted again with it
 * already open, where focus would be taken off whatever the user reached. An empty field goes away
 * once focus leaves it, so the only field on screen is one the user can see the effect of; one
 * with text typed in it stays, and the toggle is what drops both.
 */
export function ChangeFilterField({ changeView }: { changeView: ChangeViewState }): React.ReactElement {
  const t = useT();
  // The field is taken away while it may still hold keyboard focus — the toggle closing it is
  // pressed without taking focus itself — and a removal fires no blur, so focus would fall to
  // `<body>` and the terminal would stop receiving keystrokes; the hand-off puts it on the toggle.
  // This component stays mounted for the hook's check to run in, and its wrapper takes no box of
  // its own (`contents`), so the field is a flex child of the view's column.
  const handoff = useFocusHandoff(() => changeView.filterToggleRef.current?.focus());
  const input = useRef<HTMLInputElement>(null);
  const wasOpen = useRef(changeView.filterOpen);
  useEffect(() => {
    if (changeView.filterOpen && !wasOpen.current) input.current?.focus();
    wasOpen.current = changeView.filterOpen;
  }, [changeView.filterOpen]);
  // An input method's keys are stopped here before the field's own handler and before whatever
  // handles keys above it in the React tree: the composition owns them. react-aria's `useKeyboard`
  // ignores only `isComposing`, so a key an engine reports with `keyCode` 229 alone is held back by
  // this guard alone, or its Escape would clear the field. A listener on the window or the
  // document, such as the pane's Escape (which leaves editable targets alone for this), runs before
  // React's and is beyond this guard's reach.
  const guardComposition = (event: React.KeyboardEvent) => {
    if (isImeKey(event.nativeEvent)) event.stopPropagation();
  };
  // Focus moving within the field is not leaving it — nothing in it can take focus today, its
  // Clear button being kept out of the tab order and handing focus back to the input, so that part
  // of the guard is there for whatever the field grows. A `focusout` with nothing to go to is also
  // the window itself losing focus, which leaves the field where it is: the user is coming back to
  // it, and taking it away then would move keyboard focus behind their back as well.
  const onBlur = (event: React.FocusEvent<HTMLDivElement>) => {
    if (!document.hasFocus()) return;
    if (changeView.filter.trim() === "" && !event.currentTarget.contains(event.relatedTarget)) changeView.onFilterOpenChange(false);
  };
  return (
    <div className="contents" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
      {changeView.filterOpen && (
        <div className="shrink-0 border-b border-separator px-3 py-2" onKeyDownCapture={guardComposition} onBlur={onBlur}>
          <SearchField aria-label={t("git.filter.label")} value={changeView.filter} onChange={changeView.onFilterChange} fullWidth variant="secondary">
            <SearchField.Group>
              <SearchField.SearchIcon />
              <SearchField.Input ref={input} placeholder={t("git.filter.placeholder")} />
              <SearchField.ClearButton />
            </SearchField.Group>
          </SearchField>
        </div>
      )}
    </div>
  );
}
