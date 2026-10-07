import {
  ComboBox,
  Description,
  Input,
  Label,
  ListBox,
  Tag,
  TagGroup,
} from "@heroui/react";
import React, { useState } from "react";

import { TitledControl } from "../components/TitledControl";
import { useT } from "../i18n/react";

/** A labelled field for a list of free-form tags, in HeroUI's variant for a field on a surface,
 * since it is always in a dialog. The tags picked sit below the field, each removable. The field is
 * a combo box: it offers the tags in use (`suggestions`) that are not picked yet, and what is typed
 * and then confirmed with Enter, or by leaving the field, becomes a tag of its own, so a tag that
 * exists nowhere yet is made by typing it. A tag matching one already there, ignoring case, is
 * not added twice, and one matching a suggestion takes that suggestion's spelling. */
export function TagsInput({
  label,
  value,
  onChange,
  suggestions,
  placeholder,
  description,
}: {
  label: string;
  value: string[];
  onChange: (tags: string[]) => void;
  suggestions: string[];
  placeholder?: string;
  description?: string;
}): React.ReactElement {
  const t = useT();
  const [input, setInput] = useState("");

  const has = (tags: string[], tag: string) =>
    tags.some((x) => x.toLocaleLowerCase() === tag.toLocaleLowerCase());
  const add = (raw: string) => {
    // The field is cleared even for a duplicate: the tag already there shows below.
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
    <div className="flex flex-col">
      <ComboBox
        variant="secondary"
        fullWidth
        allowsCustomValue
        // An option picked is added to the tags below at once, so nothing stays selected in the field.
        // Nothing being selected is also what keeps the list open after a pick, so several can be added in
        // a row; it closes itself once no suggestion is left.
        selectedKey={null}
        inputValue={input}
        onInputChange={setInput}
        onSelectionChange={(key) => key !== null && add(String(key))}
        items={offered.map((tag) => ({ id: tag }))}
      >
        <Label>{label}</Label>
        <ComboBox.InputGroup>
          <Input
            placeholder={placeholder}
            onBlur={() => add(input)}
            onKeyDown={(e) => {
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
          <TitledControl title={t("dialog.project.tagsSuggestions")}>
            <ComboBox.Trigger
              aria-label={t("dialog.project.tagsSuggestions")}
            />
          </TitledControl>
        </ComboBox.InputGroup>
        <ComboBox.Popover>
          <ListBox>
            {(item: { id: string }) => (
              <ListBox.Item id={item.id} textValue={item.id}>
                <span dir="auto">{item.id}</span>
              </ListBox.Item>
            )}
          </ListBox>
        </ComboBox.Popover>
        {description && <Description>{description}</Description>}
      </ComboBox>
      {/* Always mounted: removing the last tag with the keyboard would otherwise unmount the focused tag, and a dialog
          whose focus falls to <body> loses Escape and Tab. */}
      <TagGroup
        aria-label={t("dialog.project.tagsPicked")}
        size="sm"
        // The gap sits on the group, not between the field and it, so no tags leave no gap.
        className={value.length > 0 ? "mt-2" : undefined}
        onRemove={(keys) => onChange(value.filter((tag) => !keys.has(tag)))}
      >
        <TagGroup.List>
          {value.map((tag) => (
            <Tag key={tag} id={tag} textValue={tag}>
              {/* The function form: TagRoot only finds a remove button among its direct children, and
                  would add a default one beside a wrapped button. */}
              {() => (
                <>
                  <span dir="auto">{tag}</span>
                  {/* The label gives only the verb: the name's tag part comes from the row, through
                      react-aria's `aria-labelledby`, so the tag is not announced twice. */}
                  <TitledControl title={t("dialog.project.removeTag", { tag })}>
                    <Tag.RemoveButton
                      aria-label={t("common.removeTag")}
                      // As the keyword chip's ×: `foreground/10` for hover on a `--default` tag, and
                      // `bg-transparent` so the tag's own hover fill is not left with a pill inside it.
                      className="bg-transparent text-muted hover:bg-foreground/10 hover:text-foreground"
                    />
                  </TitledControl>
                </>
              )}
            </Tag>
          ))}
        </TagGroup.List>
      </TagGroup>
    </div>
  );
}
