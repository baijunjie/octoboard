import {
  ComboBox,
  Description,
  Input,
  Label,
  ListBox,
  TagGroup,
} from "@heroui/react";
import React, { useRef, useState } from "react";
import { useFocusVisible } from "react-aria";

import { MotionListBox } from "../components/MotionListBox";
import { Tag } from "../components/Tag";
import { InlineReference } from "../components/InlineReference";
import { AnchoredTooltip } from "../components/AnchoredTooltip";
import { useListMotion } from "../components/useListMotion";
import { Message, useT } from "../i18n/react";

const GROUP_CLASS =
  "relative flex min-h-9 w-full flex-wrap items-center gap-1.5 rounded-field border border-(--field-border) bg-default py-1 ps-2 pe-8 [border-width:var(--border-width-field)] transition-[background-color,border-color,box-shadow] has-[input:focus]:border-(--field-border-focus) has-[input:focus]:ring-2 has-[input:focus]:ring-focus not-has-[input:focus]:hover:bg-default-hover not-has-[input:focus]:hover:border-(--field-border-hover) motion-reduce:transition-none";

/** A veil of the surface colour rather than `opacity`, which blanks the tags' GPU-layer buttons in WebKit. */
const DISABLED_CLASS =
  "pointer-events-none cursor-not-allowed after:pointer-events-none after:absolute after:inset-0 after:rounded-field after:bg-surface/55";

/** A labelled field for a list of free-form tags, in HeroUI's variant for a field on a surface,
 * since it is always in a dialog. The tags picked sit inside the field's box, each removable, ahead
 * of the text input, and wrap onto more lines as they grow, so the suggestions list, which opens
 * below the whole box, never covers them. The field is a combo box: it offers the tags in use
 * (`suggestions`) that are not picked yet, and what is typed and then confirmed with Enter, or by
 * leaving the field, becomes a tag of its own, so a tag that exists nowhere yet is made by typing
 * it. Backspace in the empty input removes the last tag. A tag matching one already there, ignoring
 * case, is not added twice, and one matching a suggestion takes that suggestion's spelling.
 *
 * Built from the combo box rather than HeroUI's multiple-selection `Autocomplete`, whose trigger is
 * not a text input: a tag that is not among the suggestions could not be created by typing it. */
export function TagsInput({
  label,
  value,
  onChange,
  suggestions,
  placeholder,
  description,
  isDisabled,
}: {
  label: string;
  value: string[];
  onChange: (tags: string[]) => void;
  suggestions: string[];
  placeholder?: string;
  description?: string;
  isDisabled?: boolean;
}): React.ReactElement {
  const t = useT();
  const [input, setInput] = useState("");
  const tagsRef = useListMotion<HTMLDivElement>({ descendants: true });
  const inputRef = useRef<HTMLInputElement>(null);

  const has = (tags: string[], tag: string) =>
    tags.some((x) => x.toLocaleLowerCase() === tag.toLocaleLowerCase());
  const add = (raw: string) => {
    // The field is cleared even for a duplicate: the tag already there shows in the box.
    setInput("");
    const typed = raw.trim();
    if (typed === "" || has(value, typed)) return;
    onChange([...value, suggestions.find((s) => has([s], typed)) ?? typed]);
  };

  const needle = input.trim().toLocaleLowerCase();
  const offered = suggestions.filter(
    (s) => !has(value, s) && s.toLocaleLowerCase().includes(needle),
  );

  return (
    <ComboBox
      variant="secondary"
      fullWidth
      allowsCustomValue
      isDisabled={isDisabled}
      // An option picked is added to the tags at once, so nothing stays selected in the field.
      // Nothing being selected is also what keeps the list open after a pick, so several can be added in
      // a row; it closes itself once no suggestion is left.
      selectedKey={null}
      inputValue={input}
      onInputChange={setInput}
      onSelectionChange={(key) => key !== null && add(String(key))}
      items={offered.map((tag) => ({ id: tag }))}
    >
      <Label>{label}</Label>
      {/* The box is drawn here, as HeroUI's secondary field, for the tags and the input together; the
          input inside it has no frame of its own. The suggestions trigger is the group's last child, the
          one HeroUI positions at the end edge, hence the end padding. HeroUI's input-group styles are not
          used for the frame: they key on their own input slot, which the combo box's input is not. */}
      <ComboBox.InputGroup
        ref={tagsRef}
        className={isDisabled ? `${GROUP_CLASS} ${DISABLED_CLASS}` : GROUP_CLASS}
        // A press on the box's own padding, not on a tag or the input, goes to the input, as in a text field.
        onMouseDown={(e) => {
          if (e.target !== e.currentTarget) return;
          e.preventDefault();
          inputRef.current?.focus();
        }}
      >
        {/* `contents`: the tags are laid out in the box's own wrapping row, beside the input. Always
            mounted, and every removal hands focus to the input: the focused tag is unmounted by it, and
            a dialog whose focus falls to <body> loses Escape and Tab. */}
        <TagGroup
          aria-label={t("dialog.project.tagsPicked")}
          size="sm"
          className="contents"
          disabledKeys={isDisabled ? value : undefined}
          onRemove={(keys) => {
            onChange(value.filter((tag) => !keys.has(tag)));
            inputRef.current?.focus();
          }}
        >
          <TagGroup.List className="contents">
            {value.map((tag) => (
              <Tag
                key={tag}
                tag={tag}
                removeTitle={<Message id="dialog.project.removeTag" params={{ tag: <InlineReference dir="auto" wrap>{tag}</InlineReference> }} />}
              />
            ))}
          </TagGroup.List>
        </TagGroup>
        <Input
          ref={inputRef}
          data-list-motion="input"
          placeholder={placeholder}
          // The group's box carries the frame, background and focus ring; these undo the input's own.
          className="h-7 min-w-24 border-0 bg-transparent px-1 py-0 shadow-none hover:bg-transparent focus:bg-transparent focus:ring-0"
          onBlur={() => add(input)}
          onKeyDown={(e) => {
            if (
              e.key === "Backspace" &&
              !e.repeat &&
              input === "" &&
              value.length > 0
            ) {
              onChange(value.slice(0, -1));
              return;
            }
            // With an option highlighted, Enter picks it (the combo box's own handling). Without
            // one, it confirms the typed text instead of submitting the dialog.
            if (
              e.key !== "Enter" ||
              input.trim() === "" ||
              e.currentTarget.getAttribute("aria-activedescendant")
            )
              return;
            e.preventDefault();
            e.stopPropagation();
            add(input);
          }}
        />
        <SuggestionsTrigger label={t("dialog.project.tagsSuggestions")} isDisabled={isDisabled} />
      </ComboBox.InputGroup>
      <ComboBox.Popover>
        <MotionListBox className="relative">
          {(item: { id: string }) => (
            <ListBox.Item data-list-motion={item.id} id={item.id} textValue={item.id}>
              <span dir="auto">{item.id}</span>
            </ListBox.Item>
          )}
        </MotionListBox>
      </ComboBox.Popover>
      {description && <Description>{description}</Description>}
    </ComboBox>
  );
}

/** InputGroup renders its siblings inside the trigger, so a tooltip's focus context there would
 * also reach the input and tags. Anchor the tooltip to the actual button without a trigger wrapper. */
function SuggestionsTrigger({ label, isDisabled }: { label: string; isDisabled?: boolean }): React.ReactElement {
  const anchor = useRef<Element>(null);
  const [isHovered, setHovered] = useState(false);
  const [isFocused, setFocused] = useState(false);
  const { isFocusVisible } = useFocusVisible();
  const focused = isFocused && isFocusVisible;
  return (
    <>
      <ComboBox.Trigger
        aria-label={label}
        aria-describedby=""
        onFocus={(event) => { anchor.current = event.currentTarget; setFocused(true); }}
        onBlur={() => setFocused(false)}
        onHoverStart={(event) => { anchor.current = event.target; }}
        onHoverChange={setHovered}
      />
      <AnchoredTooltip key={String(focused)} tooltip={label} anchor={anchor} hovered={!isDisabled && (isHovered || focused)} immediate={focused} />
    </>
  );
}
