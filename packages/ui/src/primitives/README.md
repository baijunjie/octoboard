# The wrapper layer (`primitives/`)

The one place in the application that imports `@heroui/react`, `@heroui/styles`, `react-aria-components`, `react-aria`
and the `@react-aria/*`, `@react-stately/*` and `@react-types/*` packages. Everything else — feature code and the
project's own composite components alike — uses what this layer exports. The rule and its reasons are in
[`docs/ui-standard.md`](../../../../docs/ui-standard.md), "Third-party UI components come through the wrapper layer";
lint enforces it (`packages/ui/eslint.layers.mjs`).

It is the bottom layer of `src/`: every other directory, the pure layers included, may import it, and it imports
nothing else of the application — only the third-party libraries and React. What a wrapper needs from the application
(a translated label, a preference) comes in through its props or a context the application provides, never by an
import.

## Using it

Import from the entry point, `primitives` (its `index.ts`), never from a file inside it. A component that
has no wrapper yet gets one here first, in the same change that uses it.

Code that has to find a component's part in the DOM (a focus hand-off, a measurement, a test fixture's target) uses
`partSelector` (`parts.ts`), never the library's `data-slot` names or classes; a part not listed there is added to it.
Where the application writes such a part itself (a tag drawn outside a tag group, a group's error message), the layer
draws it (`StaticTag`, `GroupFieldError`), marks and all.

## Writing a wrapper

- **One wrapper per third-party component.** A wrapper covers one component and the parts it is assembled from (a tab
  strip with its list, tab, indicator and panel), in a file named after the project's component. A composite that
  combines several components, or adds behaviour of the project's own, is not a wrapper: it lives in its kind's
  directory under `components/` and builds on the wrappers.
- **The props are the project's.** Name and shape them for what the application does with the component, and expose only
  what it uses today; add a prop when a call site needs one, not in case one might. A variant is a union of the
  project's own names, even where it matches HeroUI's.
- **No third-party type in the public API.** A wrapper's props, return types and exports name no type from the libraries
  (`ComponentProps<typeof Button>`, `Key`, `Selection`, `PressEvent` and the like) and do not pass a compound API
  through (`Tabs.List`, `Popover.Trigger`): define the type the project needs here, and convert at the wrapper. So
  replacing the library changes only this directory.
- **Appearance stays HeroUI's.** Render HeroUI's own component and pick its look through its variants and slots, as
  `docs/ui-standard.md` requires; a wrapper adds no styling the standard does not already approve.
- **A treatment the standard approves lives in the wrapper.** A fix or approved departure that every instance of the
  component needs (a WebKit compositing treatment, a size the standard confirms) is applied by the wrapper, with its
  CSS in an adjacent stylesheet and a comment naming the heading of `docs/ui-standard.md` or the reason it rests on.
  It is removed from call sites and the global stylesheet's rule bodies. A treatment that only some uses need is a
  prop of the wrapper, not a class each call site repeats.
- **Hooks and utilities.** A `react-aria` hook the application uses is wrapped by a project hook with the project's
  arguments and results (`useKeyboardModality`, `useFocusVisibleProps`, `useObjectRef`, `useTooltipState`). A utility
  that is neither a component nor a hook (`mergeRefs`) is re-exported from the entry point, with its type redefined
  here where it would otherwise bring a third-party type into a consumer's code; only the ones the application uses
  are exported, and one only a wrapper needs (`mergeProps`, `parseColor`) stays inside it.
- **Behaviour that rests on react-aria internals lives here too**, under a project name, so the version note stays
  in one place: `onFocusScopeRestore` (a one-shot capture listener for the cancelable focus-scope restore, to place
  focus when a dialog or popover closes), `MenuItem`'s `shortcut` (renders `aria-keyshortcuts`, which react-aria
  filters out of item props), `activateMenuItem` (runs an item as its keyboard activation does), `useTabPanelLink`
  (makes an element of the caller's own the panel of a tab, under the ids react-aria gave the tab) and
  `settleUnknownModality` (reads react-aria's private interaction modality to settle an unknown one). Application code
  never listens for a react-aria event, sets its attributes on a wrapper's node or synthesises its key events.
- **The library's DOM markers and states are read through the layer.** `itemKeyOf` / `itemKeySelector` (a collection
  item's key), `topLayerProps` / `closestTopLayer` / `clearTopLayer` and `liveAnnouncerProps` (what a modal's hiding of
  the page leaves out), `menuOfTrigger` (the open menu a trigger's button labels), `disabledGroupProps` (marks a group
  of controls disabled for its `Label`'s dimming), `hasKeyboardFocus` (reads the props of `useFocusVisibleProps`,
  whose type is `FocusVisibleProps`), `toastStateAttributes` for a mutation observer and the part selectors that carry a
  state in `partSelector` (`liveToast`, `liveTooltip`, `disabledControl`).
- **State is styled through the layer's variants.** `style.css` defines `ui-keyboard-focus`, `ui-pressed`, `ui-hovered`,
  `ui-selected`, `ui-disabled`, `ui-empty`, `ui-exiting` and `ui-inert` (prefixed so they do not clash with Tailwind's
  own `disabled:`, `empty:` and `inert:`), each matching the attribute the library sets, and they compose as any variant
  does (`group-ui-pressed/name:`, `has-ui-keyboard-focus:`, `in-ui-hovered:`). Application code styles a wrapper's
  element, or one given `useFocusVisibleProps`, with them and never writes `data-pressed`, `data-focus-visible` and the
  like. A state the application's own code sets on its own element (`data-current`, `data-selected` on a `TreeRow`) is
  the application's, and is named as it is with Tailwind's `data-*`.
- **A menu with a filter field is one wrapper, `MenuFilter`** (`MenuFilter.tsx`): react-aria's autocomplete around a
  `SearchField` and a `MenuList`. It draws the field's row, tells the menu's items and popover (`MenuPopover`'s
  `filtered`, `MenuItem`) they are filtered (`useMenuFiltered`; a submenu's content is reset to unfiltered), and
  exports `containsText`, the match it applies, for a caller that must know from outside whether any item remains.
  Feature code gets a filtered menu through `ActionMenu`'s `filter` prop, which builds on it.
- **Class and style helpers.** Where a consumer has to give its own element the look of a wrapped component (a button
  drawn as a link, a scroller with HeroUI's scroll shadow, a tooltip's surface), the layer exports the helper instead
  of the class names: `buttonClassName`, `scrollShadowClassName`, `scrollShadowStripStyle`, `scrollShadowAutoProps`,
  `tooltipSurfaceClassName`.
- **Honour a busy modal.** A `Modal` given `busy` disables every control in it but a `pending` `Button`, and the
  wrappers do it themselves: each interactive wrapper takes its `disabled` from `useDisabled` (`busy.ts`), which reads
  the modal's busy state, and a new interactive wrapper does the same (a pending action passes `exempt`). A container
  (`ListBox`, `Tree`, `TagGroup`, `ColorSwatchPicker`) leaves it to its items and buttons, which take it. A control
  drawn without a wrapper reads `useBusy` and renders itself unavailable. Never `inert`: assistive technology still
  reads the disabled controls. A call site does not disable its own controls for a busy dialog.
- **Tests sit beside the wrapper**, covering what it adds or translates (prop mapping, a treatment, focus or press
  behaviour it must keep); the component's own behaviour is HeroUI's to test. `testSupport.tsx` holds the helpers they
  share (`mount`, `pressWithMouse`), self-contained like the rest of the layer.
- **Internal modules stay off the barrel.** `props.ts` (`DataAttributes`, `definedProps`) is what the wrappers share
  among themselves; a consumer has no use for it. So are `UnfilteredMenu` (`MenuFilter.tsx`) and `setPointerModality`
  (`focus.ts`), which `Menu.tsx` and `MenuFilter.tsx` use between themselves.

Tests and test support, the gallery, the shared fixtures and the website preview may import the libraries directly;
they are not application source.
