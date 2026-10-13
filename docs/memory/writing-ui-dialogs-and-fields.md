# Writing UI dialogs and fields

## A confirmation is `ConfirmDialog`, never a dialog assembled for the occasion

Any confirmation in `packages/ui` — including one where the user has to type a word to confirm a deletion — is
`ConfirmDialog` (`packages/ui/src/dialogs/ConfirmDialog.tsx`, whose `typeToConfirm` asks for that word), never a
dialog of its own.

## A single-line field hands its form the trimmed value

Leading and trailing whitespace is never meant, so a form never holds it: every single-line text field in
`packages/ui` goes through `useTrimmedField` (`packages/ui/src/dialogs/useTrimmedField.ts`), which `TextInput` already
does. The form's state, its validation, its submit (Enter included) and any request fired from it see the trimmed value
at once, while the field keeps what was typed until it loses focus, so a space between two words is not stripped on its
way to the next one. A new field that is not a `TextInput` takes the hook's result as its `value`, `onChange` and
`onBlur`, and its form does not call `.trim()` on the value; the project filter's search field does the same. Left
alone on purpose: a free-text area and the tags field's input, which trims what it adds as a tag itself.

## What is wrong with one field shows under that field, from the first submit on

In a `packages/ui` dialog, a message about one field's value — empty, malformed, not allowed — belongs to that
field: `TextInput` (`packages/ui/src/dialogs/TextInput.tsx`) takes it as `errorMessage`, which marks the field
invalid and renders the message right under it. `useDialogAction().setError` and `DialogError` carry only the
failure of the request itself, at the foot of the dialog, and never a verdict on a single field.

Compute each field's message on every render from the current value and pass it through `useSubmitValidation`
(`packages/ui/src/dialogs/Dialog.tsx`): `shown(message)` holds it back until `attempt(...messages)` has gated a
submit, so no field is marked before the user has tried to submit, and from then on the message follows what is
typed. Gate the submit on what `attempt` answers rather than re-checking the values there.

Marking a field invalid yourself carries two conditions wherever that field is built — `TextInput` already meets
both, a field assembled anywhere else has to:

- Pass `validationBehavior="aria"` to the react-aria-components field. It defaults to `"native"`, which feeds a
  controlled `isInvalid` to the browser's own constraint validation (`setCustomValidity`); the browser then refuses
  the `<form>` submit before the dialog's `onSubmit` runs, and since the mark only ever appears from that handler,
  the dialog locks itself out of submitting at all.
- Put the `role="alert"` that announces the message on an element of your own inside `FieldError`, not on
  `FieldError`. react-aria passes a component's props through `filterDOMProps`, which keeps only `id`, the
  `aria-label` / `aria-labelledby` / `aria-describedby` / `aria-details` family, link props, `dir` / `lang` /
  `hidden` / `inert` / `translate`, the global events and `data-*`; anything else — `role` among them — is dropped
  without a warning, and a message that appears only on submit is then never announced at all.

## A field's note is its `Description`; HeroUI typography inside a field throws

A react-aria field that offers description and error slots — `Checkbox`, `Switch`, `TextField`, `Select`,
`RadioGroup`, `SearchField`, `NumberField`, `ComboBox`, a date field or picker, a `ListBox` / `GridList` / `Menu`
item, `TagGroup`, `Toast`; the list is what
`grep -rl "description: descriptionProps" packages/ui/node_modules/react-aria-components/dist/private/` answers —
puts a *slotted* `TextContext` over its whole subtree, and react-aria's `Text` throws outright when it finds one and
was given no `slot`: `A slot prop is required. Valid slot names are "description" and "errorMessage"`. HeroUI's
typography is that `Text` — `Paragraph`, `Heading`, `Typography` and `Code`, which the dialogs' `Path` helper renders
— so a path or any other marked-up fragment in a field's label brings the render down however deep it sits. Write
that fragment as a plain element instead (a `<span>` carrying the same classes), or wrap it in
`<TextContext.Provider value={{}}>`.

A field's explanatory note is HeroUI's `Description`, as a **direct child of the field** and a sibling of the part
that holds the control and label (`Checkbox.Content`). HeroUI styles it only in that position — the rules are keyed
on `& > [data-slot="description"]` (`packages/ui/node_modules/@heroui/styles/dist/components/`), which is what
indents it under the label — and react-aria folds its id into the control's `aria-describedby` on its own. So never
hand-roll the note as an element of your own with a `useId` and an `aria-describedby` you wire up yourself.

## A dialog's body scrolls inside `Dialog`, and fades where more is hidden

A dialog whose content outgrows the window has to say so: when the body overflows, the edge it can still be scrolled
towards fades out, so the user sees there is more below (or above) without having to find a scrollbar. Nobody adding a
dialog thinks to ask for this, so `Dialog` (`packages/ui/src/dialogs/Dialog.tsx`) does it for every dialog, around the
body's own content.

- Put the content in `Dialog`'s children and nothing else. Do not give the dialog a scroll container of your own
  (`overflow-y-auto` on a wrapper, a `max-h-*` list): its overflow would scroll apart from the body and carry no fade,
  and the body would be fading nothing.
- Nothing reaches above the body either. A line that belongs under the dialog's title goes in `Dialog`'s `subtitle`
  slot, never at the top of the children pulled up with a negative margin: the body is the scroller, so a row lifted
  above its padding box is clipped there with nothing able to scroll to it, because overflow at a scroller's block
  start adds no scrollable area. Such a row still renders right while the strip that is cut holds nothing but space,
  and the first background, border, focus ring, icon or taller text in it loses its top silently.
- Where something inside a dialog must scroll on its own, apart from the body, it takes the project's fade itself,
  from `packages/ui/src/components/scrollFade.tsx`, as `DirectoryPicker`'s list does.
