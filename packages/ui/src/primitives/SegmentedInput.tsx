import { DateField as HeroDateField } from "@heroui/react";
import React, { useEffect, useLayoutEffect, useRef } from "react";
import { flushSync } from "react-dom";

import { type FieldVariant, LOCKED_FIELD_CLASS, useFieldLocked } from "./Field";
import { partSelector, segmentSelector } from "./parts";
import { definedProps } from "./props";

/** The locked look of a field with a button in it takes the button's glyph back to full strength as the rest of the
 * field's text has (HeroUI dims a disabled button, and a locked field's button is one: it takes no press). */
const LOCKED_TRIGGER_CLASS = "[&_[data-slot=date-picker-trigger]]:opacity-100";

/** HeroUI's date input group around the segments of the date or time field it sits in (the field supplies their
 * values), full width as the text inputs beside it are; `suffix` is what ends the group (a calendar button). Inside a
 * field that is `locked` the group takes the locked look (`LOCKED_FIELD_CLASS`). */
export function SegmentedInput({ variant, suffix }: { variant?: FieldVariant; suffix?: React.ReactNode }): React.ReactElement {
  const locked = useFieldLocked();
  return (
    <HeroDateField.Group fullWidth {...definedProps({ variant, className: locked ? `${LOCKED_FIELD_CLASS} ${LOCKED_TRIGGER_CLASS}` : undefined })}>
      <HeroDateField.Input>{(segment: Parameters<typeof HeroDateField.Segment>[0]["segment"]) => <HeroDateField.Segment segment={segment} />}</HeroDateField.Input>
      {suffix !== undefined && <HeroDateField.Suffix>{suffix}</HeroDateField.Suffix>}
    </HeroDateField.Group>
  );
}

/** What a date or time field does with the value it holds while a segment is blank: React Aria keeps the last complete
 * value while one segment is cleared and leaves the field half empty, so the wrappers report `null` where the user is
 * done with it, and a half-filled entry counts as none. That is once focus leaves the field (`settle`, for the field's
 * `onBlur`) and once Enter is pressed in one of its segments, which acts on the field as it is shown and so counts as
 * the user being done with it. Enter is read on the document in the capture phase (and not during an IME composition,
 * where Enter confirms the text, however the engine reports it) and the report is flushed before the key goes on, so
 * whoever submits or saves on Enter, in a handler on an ancestor in either phase, reads `null` and not the value the
 * half-filled field no longer shows.
 * It is not reported on a cleared segment alone: the user is still typing then, and the half-entered segments stay as
 * they are. Nothing is reported on a field that takes no edit (`readOnly`). The owner reads the new value at once, also
 * where it keeps it outside React state (a ref, a store written in `onChange`); the exception is a listener that runs
 * before this capture listener on the document: one on the window, or a document capture listener registered earlier,
 * which reads the value as it was. `ref` goes on the field's root. */
export function useSettledOnBlank(value: string | null, onChange: (value: null) => void, readOnly: boolean): { ref: React.RefObject<HTMLDivElement | null>; settle: () => void } {
  const ref = useRef<HTMLDivElement | null>(null);
  const settle = () => {
    if (!readOnly && value !== null && ref.current?.querySelector(partSelector.blankSegment)) onChange(null);
  };
  const latest = useRef(settle);
  useLayoutEffect(() => {
    latest.current = settle;
  });
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Enter" || event.isComposing || event.keyCode === 229) return;
      if (event.target instanceof Element && ref.current?.contains(event.target) && event.target.matches(segmentSelector())) {
        flushSync(() => latest.current());
      }
    };
    document.addEventListener("keydown", onKeyDown, true);
    return () => document.removeEventListener("keydown", onKeyDown, true);
  }, []);
  return { ref, settle };
}
