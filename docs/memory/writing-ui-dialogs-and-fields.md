# Writing UI dialogs and fields

## A confirmation is `ConfirmDialog`, never a dialog assembled for the occasion

Any confirmation in `packages/ui` — including one where the user has to type a word to confirm a deletion — is
`ConfirmDialog` (`packages/ui/src/dialogs/ConfirmDialog.tsx`, whose `typeToConfirm` asks for that word), never a
dialog of its own.

## Raw error text is `DiagnosticBlock`, never a container written for the occasion

Wherever `packages/ui` shows raw error text as a block of its own — a message passed through from the system, Git or
an agent, a diagnostic — in an error alert, an Alert's Details popover, an "Error details" disclosure or an error
screen, render it with `DiagnosticBlock` (`packages/ui/src/components/DiagnosticBlock.tsx`). Never put it in a
`<p className="font-mono">`, a bare `CodeBlock` or a copy of the block's markup. It is the user's design rule that raw
text sits on a surface of its own, never straight on the dialog, Alert or popover around it, and the component also
carries that text's accessibility (a named horizontal scroll region reachable by keyboard, the focus ring drawn on the
surface), which hand-written copies drifted away from. A disclosure's summary or other label stays outside the block,
its id passed as `labelledBy`; without one the region is named "Error details". Two variants, which combine, exist for
text that does not fit the default: `tall` also scrolls vertically (up to 40% of the viewport, a crash's stack), and
`wrap` breaks long lines instead of scrolling sideways (a message in prose-like lines, such as the daemon's startup
failure). On its own `wrap` has no scroll region at all, so wherever the block's height is bounded — any popover,
the Details popover included — a wrapped block takes `tall` too, or long text overflows the popover's surface. A
change to how raw text looks goes into the component, not into one caller.

## A destructive confirmation's options sit apart from its consequences, which are one block under them

This is the user's design rule for every destructive confirmation in `packages/ui` (removing, deleting, archiving)
that offers options, such as taking more along with what is deleted. A confirmation that is not destructive, such as
the trust prompt, has no consequences block: the risk an option brings is a warning Alert under it while it is ticked
(`ChoiceWarning`, see `docs/ui-accessibility.md`, "On the screen: an Alert when it matters, a mark at its point
otherwise").

- A checkbox never goes inside the consequences. It is a plain block of its own: its label says what ticking it
  deletes (and how many), its note is kept short so it does not repeat the consequence line below, and the items it
  covers go under it in a Details disclosure that starts collapsed.
- The consequences are one block, passed as `ConfirmDialog`'s `consequences`, which renders it as body text under the
  options and over the buttons, so a line it gains never moves a checkbox; passing it makes the dialog a warning
  modal (see `docs/ui-accessibility.md`, "Where an error is shown"), never a modal with a warning box in its body. What
  the dialog says it takes along before any option is ticked (the sessions it ends) is that same block, not one above
  the options.
- Every option, whatever it covers, adds its consequence line to that block while it is ticked. If the dialog had
  nothing else to say, the block appears with the first ticked option and goes away when none are.
- A disclosure stays the way the user left it. Never collapse it or remount it because something else in the dialog
  changed.

The block is `ConfirmConsequences` in `packages/ui/src/dialogs/RequestedDialog.tsx`, whose `lines` are the ticked
options' consequences; the remove-project dialog's Trash option, in `packages/ui/src/dialogs/BranchCleanupOptions.tsx`,
shows the option's shape.

## Read the project's placeholder standard before adding or changing a field

For an input in `packages/ui`, follow `docs/ui-accessibility.md`, "Input placeholders", before choosing its
placeholder. That project standard takes precedence over the generic "Input placeholders" guidance in
`.claude/skills/knowledge-i18n-copy/SKILL.md`.

## A choice the user has to make again starts empty, never on a fallback

Applies to a `packages/ui` form that must not go on until the user has picked something themselves — what was asked
for is gone, say. A react-aria single `Select` (so `OptionSelect`) calls `onChange` only when the key changes, so
picking the value it already shows reports nothing: a form that pre-fills a fallback and waits for "the user picked"
can never be satisfied by choosing that fallback. Show the picker with no value and a placeholder until the pick
instead. With `OptionSelect` that is its `placeholder` variant, which is clearable back to `null`: where `null` would
stand for a real choice (the default account), use it only while nothing is picked, then render the required variant
in the same JSX position, a ternary rather than two `&&` slots, so the focused picker is not remounted.

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
invalid and renders the message right under it. Only the form's own check at submit goes there: anything that fails
once the submit has passed it — a daemon refusal about that same field's value included — opens the error alert
instead, as the "Where an error is shown" section of `docs/ui-accessibility.md` sets out.

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

A field's explanatory note is HeroUI's `Description`, as a **direct child of the field** and a sibling of the part that
holds the control and label (`Checkbox.Content`). HeroUI styles it only in that position — the rules are keyed on
`& > [data-slot="description"]` (`packages/ui/node_modules/@heroui/styles/dist/components/`), which is what indents it
under the label — and react-aria normally folds its id into the control's `aria-describedby` on its own. Keep using
`Description`, rather than a hand-rolled note. For a `Select` whose note can first appear after the field has mounted
without one, give `Description` a stable `useId` and set the field's `aria-describedby` to that id only while the note
exists: react-aria's slot association does not recover when only the children change. Preserve the mounted control so
the note appearing does not lose keyboard focus; verify the association through absent → present → absent notes.

The one checkbox that cannot keep it there is one with a mark (`StatusMark`) after its label — a warning about the
state of what it acts on; the consequence of ticking it is no mark but a `ChoiceWarning` under the option. The mark
stays outside react-aria's `<label>` — inside it, it is interactive content in a label and its name joins the
checkbox's — and it has to follow the label's text, which it cannot do beside a checkbox that holds its `Description`:
that checkbox fills the row, so the mark lands at the row's far end. Make the checkbox `w-fit`, put the mark beside it
in an `items-start` row of their own, the mark in `MarkLineBox` (never a hand-picked `min-h`: the checkbox's content
box is taller than its label's line, which sits at its top), and move the note under that row as a `Description` with
its own `useId`, set in the checkbox's `aria-describedby` and indented under the label by hand. A dialog option gets
all of this from `ConfirmOption`'s `mark` (`packages/ui/src/dialogs/ConfirmOption.tsx`).

## A modal whose content is one body is a `Dialog`; one made of panels keeps its own frame

`Dialog` (`packages/ui/src/dialogs/Dialog.tsx`) is a titled frame around a single scrolling body, plus an optional
footer, and every modal in `packages/ui` whose content is one body is built on it. A modal whose content is a set of
panels that each scroll on their own is the other shape: it keeps a frame of its own, assembled from HeroUI's
`Modal.*` parts, and takes the fade inside each panel. The settings dialog
(`packages/ui/src/settings/SettingsDialog.tsx`) is that, deliberately — not a surface left behind waiting to be moved
onto `Dialog`. Do not widen `Dialog` with an absent header, a mode where the content is panels rather than one body,
or a size of its own in order to take such a modal in.

## A dialog's body scrolls inside `Dialog`, and fades where more is hidden

A dialog whose content outgrows the window has to say so: when the body overflows, the edge it can still be scrolled
towards fades out, so the user sees there is more below (or above) without having to find a scrollbar. Nobody adding a
dialog thinks to ask for this, so `Dialog` (`packages/ui/src/dialogs/Dialog.tsx`) does it for every dialog built on
it, around the body's own content.

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
