import { ComboBox as HeroComboBox } from "@heroui/react";
import React, { createContext, useContext, useLayoutEffect, useMemo, useRef } from "react";
import { ComboBoxStateContext } from "react-aria-components";

import { useDisabled } from "./busy";
import type { FieldVariant } from "./Field";
import { type DataAttributes, definedProps } from "./props";
import { mergeRefs } from "./refs";

/** HeroUI's combo box, for free text with suggestions: the field around a `Label`, a `ComboBoxInputGroup` holding an
 * `Input` and a `ComboBoxTrigger`, a `ComboBoxPopover` holding a `ListBox` of `items` (keyed by their ids), and a
 * `Description`. What is typed is `inputValue`, reported to `onInputChange`, and need not match a suggestion. A
 * suggestion picked is handed to `onPick` and nothing stays selected in the field: no selection is also what keeps the
 * list open after a pick, so several can be picked in a row, until no suggestion is left.
 *
 * Every combo box keeps its list open when its trigger is pressed while the field is partly out of view (see
 * `useRevealOnTriggerPress`). */
export function ComboBox<T extends { id: string }>({ inputValue, onInputChange, onPick, items, disabled, variant, fullWidth, children }: {
  inputValue: string;
  onInputChange: (value: string) => void;
  onPick: (id: string) => void;
  items: Iterable<T>;
  disabled?: boolean;
  variant?: FieldVariant;
  fullWidth?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  const group = useRef<HTMLDivElement>(null);
  const reveal = useRevealOnTriggerPress(group);
  return (
    <HeroComboBox<T>
      {...definedProps({ variant, fullWidth, isDisabled: useDisabled(disabled) })}
      allowsCustomValue
      selectedKey={null}
      inputValue={inputValue}
      onInputChange={onInputChange}
      onSelectionChange={(key) => key !== null && onPick(String(key))}
      items={items}
    >
      <ComboBoxStateRef onCommit={reveal.onStateCommit} />
      <RevealContext.Provider value={{ group, handlers: reveal.groupProps }}>{children}</RevealContext.Provider>
    </HeroComboBox>
  );
}

type RevealHandlers = Pick<React.DOMAttributes<HTMLDivElement>, "onPointerDownCapture" | "onClickCapture">;

const RevealContext = createContext<{ group: React.RefObject<HTMLDivElement | null>; handlers: RevealHandlers } | null>(null);

/** The box of a `ComboBox` that holds its `Input` and, as its last child, its `ComboBoxTrigger`, which HeroUI positions
 * at the box's end edge. The other props, a `ref` among them, go to the box's element. */
export function ComboBoxInputGroup({ ref, children, ...props }: Omit<React.HTMLAttributes<HTMLDivElement>, "children" | "onPointerDownCapture" | "onClickCapture"> & DataAttributes & {
  ref?: React.Ref<HTMLDivElement>;
  children: React.ReactNode;
}): React.ReactElement {
  const reveal = useContext(RevealContext);
  const merged = useMemo(() => mergeRefs(ref, reveal?.group), [ref, reveal?.group]);
  return (
    <HeroComboBox.InputGroup {...props} {...reveal?.handlers} ref={merged}>
      {children}
    </HeroComboBox.InputGroup>
  );
}

/** The button of a `ComboBox` that opens its suggestions, named `label`. `describedBy` `""` keeps a tooltip that
 * repeats the name from describing it. `onFocus` and `onHoverStart` receive the button. */
export function ComboBoxTrigger({ label, describedBy, onFocus, onBlur, onHoverStart, onHoverChange }: {
  label: string;
  describedBy?: string;
  onFocus?: (button: Element) => void;
  onBlur?: () => void;
  onHoverStart?: (button: Element) => void;
  onHoverChange?: (hovered: boolean) => void;
}): React.ReactElement {
  return (
    <HeroComboBox.Trigger
      aria-label={label}
      {...definedProps({
        "aria-describedby": describedBy,
        onFocus: onFocus && ((event: React.FocusEvent<Element>) => onFocus(event.currentTarget)),
        onBlur: onBlur && (() => onBlur()),
        onHoverStart: onHoverStart && ((event: { target: Element }) => onHoverStart(event.target)),
        onHoverChange,
      })}
    />
  );
}

/** The popover of a `ComboBox` that holds its `ListBox`, below the input group. */
export function ComboBoxPopover({ children }: { children: React.ReactNode }): React.ReactElement {
  return <HeroComboBox.Popover>{children}</HeroComboBox.Popover>;
}

type ComboBoxStateValue = React.ContextType<typeof ComboBoxStateContext>;

const isTrigger = (target: EventTarget) =>
  target instanceof Element && target.closest('[data-slot="combo-box-trigger"]') !== null;

/** Hands the combo box's state, which the combo box keeps to itself, to `onCommit` after every render it changes in. */
function ComboBoxStateRef({ onCommit }: { onCommit: (state: ComboBoxStateValue) => void }): null {
  const state = useContext(ComboBoxStateContext);
  useLayoutEffect(() => onCommit(state));
  return null;
}

/** Pressing the suggestions trigger of a field partly out of view brings the field into view and leaves the list open.
 *
 * The combo box focuses the input and opens the list on the press, and react-aria closes the list as soon as anything
 * holding the field scrolls. Left alone, that focus scrolls a partly hidden input into view, which shuts the list as it
 * opens; kept from scrolling, the list stays open over a field still out of view, and the first key typed scrolls the
 * input to its caret and shuts it then. So the press, before the combo box sees it, scrolls the whole box into view
 * (`nearest`, the least scroll; a faded scroller's scroll padding keeps it clear of the fade), the combo box's own
 * focus is kept from scrolling again, and the list that scroll closes is opened again as the close is committed, before
 * it is painted.
 *
 * A mouse or pen press starts on the primary button's pointerdown; a touch press, and a screen reader's, is the click. */
function useRevealOnTriggerPress(group: React.RefObject<HTMLDivElement | null>): {
  onStateCommit: (state: ComboBoxStateValue) => void;
  groupProps: RevealHandlers;
} {
  const stateRef = useRef<ComboBoxStateValue>(null);
  // Until when a close is the reveal's scroll's, to be undone; set only once that scroll has arrived.
  const reopenUntil = useRef(0);
  const pressedByPointer = useRef(false);
  const input = () => group.current?.querySelector("input") ?? null;

  const onStateCommit = (state: ComboBoxStateValue) => {
    stateRef.current = state;
    if (!state || state.isOpen || performance.now() > reopenUntil.current) return;
    reopenUntil.current = 0;
    const field = input();
    if (!field || document.activeElement !== field) return;
    // The press's own focus strategy, which the close leaves in place: a screen reader's press starts on the first
    // option. The list, kept mounted through the close, applies it only when it mounts, so it is applied here.
    state.open(state.focusStrategy, "manual");
    if (state.focusStrategy === "first") state.selectionManager.setFocusedKey(state.collection.getFirstKey());
  };

  const reveal = (box: HTMLElement) => {
    const field = input();
    if (!field || !stateRef.current || stateRef.current.isOpen) return;
    const focusWithoutScroll = (options?: FocusOptions) => {
      Reflect.deleteProperty(field, "focus");
      field.focus({ ...options, preventScroll: true });
    };
    field.focus = focusWithoutScroll;
    setTimeout(() => {
      if (field.focus === focusWithoutScroll) Reflect.deleteProperty(field, "focus");
    });

    const before = box.getBoundingClientRect();
    box.scrollIntoView?.({ block: "nearest", inline: "nearest" });
    const after = box.getBoundingClientRect();
    if (before.top === after.top && before.left === after.left) return;
    const arm = () => {
      stop();
      reopenUntil.current = performance.now() + 200;
    };
    const stop = () => {
      window.removeEventListener("scroll", arm, true);
      clearTimeout(timer);
    };
    window.addEventListener("scroll", arm, true);
    const timer = setTimeout(stop, 500);
  };

  return {
    onStateCommit,
    groupProps: {
      onPointerDownCapture: (e) => {
        if (e.button !== 0 || e.pointerType === "touch" || !isTrigger(e.target)) return;
        reveal(e.currentTarget);
        // The press's click, wherever it lands, follows the release in the same task; a cancelled press has none.
        pressedByPointer.current = true;
        const release = () => {
          window.removeEventListener("pointerup", release, true);
          window.removeEventListener("pointercancel", release, true);
          setTimeout(() => (pressedByPointer.current = false));
        };
        window.addEventListener("pointerup", release, true);
        window.addEventListener("pointercancel", release, true);
      },
      onClickCapture: (e) => {
        if (!pressedByPointer.current && isTrigger(e.target)) reveal(e.currentTarget);
      },
    },
  };
}
