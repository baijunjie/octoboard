# Writing UI dialogs and fields

The rules for dialogs and fields are in `docs/conventions/ui-components.md`, "Shared components to reuse" and "Dialogs
and fields". What they call for checking:

## Keep HeroUI typography out of a react-aria field, and find which fields those are by grepping react-aria

HeroUI typography inside a field's label or subtree throws; use a plain element with the same classes, or wrap it in
`<TextContext.Provider value={{}}>`. A react-aria field that offers description and error slots puts a slotted
`TextContext` over its whole subtree, which makes an unslotted `Text` (HeroUI's `Paragraph`, `Heading`, `Typography`,
`Code`, the dialogs' `Path` helper) throw. The list is more than the obvious fields (list, grid and menu items,
`TagGroup`, `Toast` among them); it is what this answers:
`grep -rl "description: descriptionProps" packages/ui/node_modules/react-aria-components/dist/private/`.

## Verify a late-appearing `Select` note through absent, present and absent again

react-aria's slot association does not recover when only a field's children change, which is why such a note gets its
own `useId` and `aria-describedby`. Check the association, and that the control keeps keyboard focus, as the note goes
from absent to present and back.
