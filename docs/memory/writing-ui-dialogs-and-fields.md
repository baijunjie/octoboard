# Writing UI dialogs and fields

The rules for dialogs and fields (`ConfirmDialog`, `DiagnosticBlock`, destructive confirmations, empty choices, trimmed
fields, field errors, `Description`, HeroUI typography inside a field, option notes, `Dialog`'s body) are in
`docs/conventions/ui-components.md`, "Shared components to reuse" and "Dialogs and fields". Two checks they call for:

## Find which fields throw on HeroUI typography by grepping react-aria, not by memory

A react-aria field that offers description and error slots puts a slotted `TextContext` over its whole subtree, which
makes an unslotted `Text` (HeroUI's `Paragraph`, `Heading`, `Typography`, `Code`, the dialogs' `Path` helper) throw. The
list is more than the obvious fields (list, grid and menu items, `TagGroup`, `Toast` among them); it is what
`grep -rl "description: descriptionProps" packages/ui/node_modules/react-aria-components/dist/private/` answers.

## Verify a late-appearing `Select` note through absent, present and absent again

react-aria's slot association does not recover when only a field's children change, which is why such a note gets its
own `useId` and `aria-describedby`. Check the association, and that the control keeps keyboard focus, as the note goes
from absent to present and back.
