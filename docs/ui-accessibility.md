# UI accessibility standard

The accessibility bar `packages/ui` is held to, and the rules and measured facts that follow from it. It is a standard
to check a control or a view against: a code comment may cite a rule here by this file's path and the heading it sits
under.

## Conformance level and exceptions

Style requirements follow this priority:

1. HeroUI components use the library's default appearance through its supported variants and slots.
2. Application-authored custom styles follow WCAG 2.2 level AA in both the light and the dark appearance.

Do not override HeroUI's default colours, placeholder colours, opacity, borders or other visual treatments solely to
meet an AA threshold. A library-default appearance below that threshold is not a project bug or an exception requiring
approval. The visual thresholds below apply to application-authored custom styles; application-level keyboard access,
accessible names, focus handling and announcements remain required.

The user has kept a few shortfalls on purpose, each marked by a comment at its code saying so. Leave one that is marked
as it is, and do not add a new exception without the user's say.

### Confirmed visual choices

Tabs in Settings, the project's Files / Git switch, the Git view's Uncommitted / Compare switch and the session
dialog's New / Resume conversation switch keep their labels fully opaque on hover and use the foreground colour instead
of dimming. Preserve this shared visual treatment as an explicit product choice, while retaining the component's
layout, selection and keyboard behaviour.

A notice toast (HeroUI's accent variant) tints its indicator with the title's `--accent-soft-foreground`, as HeroUI
does itself for its success, warning and danger toasts, instead of leaving it in the plain foreground under an accent
title.

## Keyboard

Every interactive control is reachable from the keyboard and operable with Enter / Space, with one exception inside a
dialog's form: a control that holds a choice there (a checkbox, a radio, a switch, a closed select, a tab) takes Space,
and the arrow keys where it has them, while Enter submits the dialog from it, as Enter does from a text field. The
platform does this for the native ones (a checkbox, a radio, a switch), and the dialog does it for a closed select, a
tab and a radio that is not a native input, so that having picked an agent the user presses Enter to go ahead rather
than to open the picker again. A button that acts (Cancel, a disclosure) keeps Enter, and so does an open popover's
list. So a dialog whose confirm button carries weight does not also rely on Enter to settle a choice one of these
controls holds.

A confirmation dialog opens with focus on its confirm button, so Enter confirms it at once. Two kinds do not, and keep
focus on the dialog itself, so an Enter pressed before the user has moved focus onto one of its controls does nothing: a
destructive one, and a question the user did not ask for (a folder trust prompt, a session request), which can come up
while they are typing elsewhere. The latter starts on the dialog again for each queued question it moves on to. One that
asks for a word to be typed opens with focus in that field. A dialog's form ignores the repeats of a held Enter or
Space, except Space in a text field and Enter in a multi-line one, so the press that opened it cannot go on to submit
it.

A form dialog opens with focus in its first text field, so the user can type at once and Enter submits; one with no text
field opens with focus on its submit button instead. A field that appears later, after a tab or a choice has changed what
the form holds, does not take focus, so it never pulls it off the control the user is on.

### Reaching a control with Tab

Tab is how a control is reached, except for a control inside the row of a react-aria collection that manages focus
itself (a `GridList`, a `Tree`). There the row is the tab stop and Tab moves past the whole list, while the arrow key
that runs along the row (`keyboardNavigationBehavior` defaults to `"arrow"`) moves between the row and its controls.
That is the grid and tree pattern and it meets this rule. The row actions of the change list and of the Files tree,
and the Files tree's folder icons, all rely on it, so do not give such a control a tab stop of its own.

`preventFocusOnPress` only keeps a *mouse* press from taking focus; the control still has to be reachable.

**Where a float decides the source order, every control of that row goes in the float.** A group that floats to the
end of a row has to come first in the source, because a float only shortens the lines that follow it; the text it
leaves beside it then reads after it. That is only a reading-order cost while the row's other content is inert text,
and it becomes a focus-order one — focus jumping back against the reading direction — the moment a control sits in
that text. So a control belonging to such a row joins the floating group rather than standing among the text: the
file viewer's detail row floats Copy path and the view controls to its end, rather than placing a button beside
the path it copies (`packages/ui/src/viewer/FileViewer.tsx`).

A control inside a collection's **section header** is reached by neither of those: react-aria's keyboard delegate
walks the collection's items alone, so the arrow keys never land on a header, Tab inside the collection jumps to the
last tabbable element and leaves, and focus arriving anywhere in the collection that is not a row is moved to its
first row. The one such control in the window is the change list's section heading, the band that folds the section
away (`packages/ui/src/browser/changeSectionHeading.tsx`), and it is reached by this application extending the
collection's own walk: Up and Down step through the rows **and the headings between them**, while Home and End still
move between rows alone. Such a control takes no tab stop of its own and must not take focus on a press
(`preventFocusOnPress`), or a mouse press on it sends focus to the list's first row.

The band is react-aria's `rowheader` inside a `role="row"`, not an `<h1>`-`<h6>`, so the rule under "A visual heading
is a heading element" does not reach it: it is a row of a grid, where that markup is what the grid pattern asks for,
and a heading element inside the collection would be a second structure over the same text.

### Buttons

Every button is HeroUI's `Button`. A native `<button>` is allowed only where a custom button is needed that HeroUI's
cannot be, with the reason in a comment there. A hand-built `role="button"` element needs `tabIndex={0}` and its own
Enter / Space handling.

### A control inside a rendering library's shadow root

The diff viewer's separators of collapsed lines are drawn by `@pierre/diffs` into a shadow root, where no React tree
reaches, so a control made out of that markup is built by hand and every rule above is met by hand with it: the
element takes `role="button"`, an explicit `tabindex`, an accessible name, a native `title` in place of the HeroUI
tooltip that cannot be rendered there, and Enter / Space handling of the viewer's own
(`packages/ui/src/viewer/rendererDom.ts`, which holds every read and write of that markup). The library draws each
separator more than once — in the gutter and in the code column, and once per side of a split diff — so only the first
copy of a control is a tab stop and is named; the rest take `tabindex="-1"` and `aria-hidden="true"`, or the same
control is announced several times over.

Tab does not reach into a shadow root from the dialog's own focus containment either, so the walk along those controls
is made by hand from the code frame (`focusFirstSeparatorControl`, `moveAlongControls`).

### Leaving the terminal with F6

The terminal keeps Tab and Shift+Tab for the agent, so F6 / Shift+F6 region cycling
(`packages/ui/src/layout/useRegionCycle.ts`) is the only keyboard way out of it. Every new region of the window that
holds controls — a pane, a panel, an overlay — is marked `data-region` and added to that hook's `REGIONS` and `shown`,
or F6 skips it and a keyboard user in the terminal has no way to reach it. `shown` follows whether a region holds a
control, not whether its element is in the document: the terminal counts while a session is selected, and also while
its empty state offers a creation action for the current context, since that button is then the one thing to do
there; with only its empty line it is skipped. A region whose focus goes through a method
of its own rather than its first control (the terminal's) is only a stop when that method actually leaves focus inside
it, which is how the cycle tells the two apart.

## Input placeholders

Every editable text input has a non-empty placeholder, including fields whose current value hides it.

- A required input shows a valid example with the localized example prefix: `例如：XXX` in Simplified Chinese
  and `Ex. XXX` in English. This applies to format examples such as paths, URLs and email addresses too.
- An optional input shows the default that takes effect when it is left empty, without an example prefix.
  Derive the placeholder from the current context: a session title uses its project's name, and a project name
  uses the directory or repository name. Update it when the context changes; a fixed sample is not a default.
- When the empty state means an inherited setting or no value, name that default state accurately rather than
  inventing a concrete value. A default that belongs to each item in a batch must not be presented as one shared
  value.

The placeholder is a hint, never an initial input value or a substitute for the field's label, accessible name,
description or validation. Leaving a field empty retains its existing requiredness and default behavior.

## Visible focus

HeroUI controls draw their own focus ring.

A scrollable tab list leaves enough inner padding for the full focus ring and its offset on every
side. `Tabs.ListContainer` clips its child list through a scroll container, so this clearance belongs
to `Tabs.List` itself; padding outside the scroller does not protect the ring. Each tab also needs matching horizontal
scroll margin so keyboard navigation reveals the ring as well as the tab border. Suppress the tab
scroller’s fade mask while a tab has visible keyboard focus, so it cannot fade out the ring.

An invalid, focused HeroUI `Input` keeps its outline offset at zero so the error outline meets the
focus ring. This overrides WebKit's native inset without changing the component's colours or ring.

### A hand-built focusable element

An element that removes the outline puts a ring back, never `outline-none` alone, and takes it from
`useFocusVisibleProps` (`packages/ui/src/components/useFocusVisibleProps.ts`): spread its props on the element and
style with `data-focus-visible:ring-2 data-focus-visible:ring-app-focus` (`group-data-focus-visible:` from a child).
The application's custom ring uses `--app-focus`; HeroUI controls retain their own default focus token.

### Never style focus with CSS `:focus-visible`

No `focus-visible:` or `group-focus-visible:` variant, no `:has(:focus-visible)` selector. Once the last focus came from
a click, the app's WKWebView does not match `:focus-visible` on an element focused by script, even right after a key
press. So every keyboard move made by script — F6 region cycling, Tab inside a react-aria focus-trapped dialog — lands
without a ring. Playwright's `webkit` matches it in those cases, so only the packaged app shows the miss.

Inside a rendering library's shadow root the app's `useFocusVisibleProps` cannot be spread on an element either, so
the ring is drawn from a `data-focus-visible` attribute set and cleared by hand, styled from the stylesheet injected
into that root (`[data-expand-button][data-focus-visible]` and `[data-separator-content][data-focus-visible]` in
`packages/ui/src/viewer/renderer.tsx`).

### Focus moved by a change the user did not make

A hand-off that runs because what held focus went away — a pane hidden, a region unmounted by a daemon event — also
calls `setInteractionModality("keyboard")` before it focuses the destination. The user did not ask for the move, so
they have to be able to see where focus went, and the previous input was often a pointer press, which leaves the
destination without a ring. `handFocusOff` takes a `showRing` flag for the hand-off whose trigger is a press, where the
ring follows how the press came; `HandOffOnUnmount` always sets it, its trigger being a render, and so does
`useFocusHandoff`, whose trigger is the render that took the control away. That covers the one hand-off through it
whose trigger is really a press — the change filter field closed from the header's toggle, which takes no focus
itself — knowingly: focus lands on a control the user did not aim it at either way.

A separator of collapsed lines taken off the screen is such a move: the bar for the run past a diff's last hunk goes
once the daemon's reply says that hunk reaches the end of the file, and the hand-off to the code frame sets the
modality itself. `handFocusOff` cannot be used there, since it looks at `document.activeElement` and cannot see into a
shadow root.

### Focus scrolled to the edge of a scroller that fades there

A scroller whose edges are faded gives the scroller a `scroll-padding-block` of at least the fade's own size. A
browser scrolls a control only far enough to reach the container's edge, which is exactly where the fade sits, so the
control the user was sent to arrives underneath it: seen here on the first invalid field a submit sends focus to,
which the fade covered at the window heights where that field was the one scrolled to. The padding is on the scroller
rather than on what it holds, so nothing put inside has to know about it. Every scroller that takes the fade gets it,
a dialog's body and a list in a popover alike, and neither shape of the project's own capability leaves it to the
site: the padding comes with the fade. A fade the library draws for its own component is the one exception, and not
one to copy — a HeroUI `Tabs.ListContainer` fades its tab strip over 64 px with no padding at all, so a tab the arrow
keys reach at the strip's edge does land under it.

Where a sticky heading occupies the top of a faded list, the top scroll padding includes both the heading's height
and the fade's size, so a focused row is visible below them. The heading itself stays clear of the fade.

A control inside such a heading sits within that padding while it is pinned, so focusing it would scroll the list by
the padding's depth on every focus move — seen on the Projects heading's Funnel X and Collapse all, the list jumping
as Tab moved through them. The heading's controls are exempted: their scroll area is moved down by the padding's
depth (`scroll-margin-block`), which Chromium honours, and since WebKit's own focus scroll ignores scroll margins and
runs after the focus event, the list's position is held for a moment from that event while focus stays in the heading
and the control was already in view. A heading not yet in view is still revealed.

### A scroller nothing else on screen scrolls takes focus itself

A faded edge says there is more, so the keyboard has to be able to reach it. Where nothing else on the screen would
scroll *this* scroller, it becomes a focusable `region` with an accessible name, and the arrow keys and Page Down then
scroll it. The test is that scroller, not what it contains: a stop inside it carries focus only as far as itself, so
everything past that stop is still content nothing reaches. A dialog whose controls are all in its footer is the case
`Dialog` owns, named by the dialog's own title; the screen shown before there is anything to show is another, named
from the catalog, where the buttons sit after the message and the stack trace between them is a scroller in its own
right, named from the catalog too.

Such a region appears only while the scroller overflows, since a tab stop that scrolls nothing is one the user passes
for no reason — the crash screen's stack trace and the screen's own column both come and go that way. The exception
is a region that is for more than scrolling: the file viewer's reading surfaces are tab stops whether or not they
scroll, because taking focus is also what their Select All and their copy handler hang off. So a region that does
something of its own stays; one that only scrolls comes and goes with the overflow.

### A keyboard handler that moves focus itself

Such a handler calls react-aria's `setInteractionModality("keyboard")` first when the handler sits on the window's
capture phase and stops propagation. react-aria tracks modality from a capture-phase listener on the document, which
the key then never reaches; without the call, what the handler focuses draws no ring when the previous input was a
pointer.

## Accessible names

Every control has an accessible name: its visible text, or an `aria-label` when it has none.

- An icon beside a name is `aria-hidden="true"`.
- An icon that is the only carrier of a meaning gets `role="img"` and an `aria-label`.
- A control carrying visible text *and* a name of its own has the visible text inside the name, verbatim, numbers
  included (WCAG 2.2 A 2.5.3), so that saying what is on screen reaches it by voice. A separator of collapsed lines is
  one instance: its visible text counts the lines it hides and its name is "Show all 17 unmodified lines", which is why
  that message takes the count as a placeholder rather than reading "Show the unmodified lines". Check such a name
  against every state the visible text has, not only the first: the same bar also says it is loading, says a read
  failed, and says the count is unknown.
- A name made of two messages is assembled with `aria-labelledby` over the elements that already carry them, not by
  joining them into one `aria-label`: a catalog's messages are whole sentences and a joined string is a fragment
  nobody can translate. An empty row that is itself the button filling the list (`EmptyPanel`'s compact form with an
  action) is the instance — its visible message, then an `sr-only` span with the action, giving "No projects yet Add
  project". Such a row also sets an empty `aria-describedby` itself, since `TitledControl` only recognizes a tooltip
  that repeats an `aria-label`.
- **A name drawn as several boxes in a flex row is built as one string of its own**, with the drawn boxes hidden from
  the name (`aria-hidden="true"`): WebKit puts a space around every flex item while composing a name, so the boxes
  would be announced as "Modified , Staged : a.ts". The file viewer's title is the instance — an `sr-only` span holds
  the whole name, assembled from the catalog in the order the boxes are drawn in, and carries `select-none` so a copied
  heading does not hold the text twice (`packages/ui/src/viewer/FileViewer.tsx`).
- An element that takes an `aria-label` without being a control — a scrolling `div` or `pre` made focusable with
  `tabIndex={0}` — also gets a role that can be named (`role="region"`). ARIA prohibits naming a generic element, so
  the name may never be spoken. A diagram and a block formula in a Markdown document are such regions
  (`MermaidBlock`, `FormulaView` in `packages/ui/src/viewer/`). The diagram's name is its own title, or the generic
  diagram name, and an inner `role="img"` carries that same name with a text alternative. A formula is not an image:
  its MathML is what is read, and naming the region does not replace that. An inline formula is a region, and a tab
  stop, only while it is wider than the line. While it fits, it is not a tab stop.

### An icon-only control: tooltip and `aria-label`

Every control that shows only an icon — a HeroUI `Button` with `isIconOnly`, a `CloseButton` or `Modal.CloseTrigger`, a
menu trigger — carries both a tooltip and an accessible name. The name is what assistive technology announces, and it
has to tell the control apart from its neighbours even where the tooltip's text cannot, so the two are not always the
same text. Which control's tooltip repeats its name and which one's is the shorter of the two is settled per control in
the "Tooltips on icon-only controls" section of `docs/product/labels-and-tooltips.md`; `ActionMenu` gives its trigger a
name of that shape by default.

The exception is a control inside a react-aria collection row, such as a tag's remove button. react-aria already
composes its accessible name from the button and the row through `aria-labelledby`, so there the `aria-label` is the
bare verb with no placeholder, or the row's own text is announced twice.

### A tooltip is the application's own, never the browser's `title`

Nothing in the window uses the native `title` attribute to show a tooltip. The browser's own draws outside the
application's control — its own timing, its own styling, and a second floating layer wherever the application already
has one. Two places keep it, and both are a case where nothing else can serve:

- The diff viewer's separators of collapsed lines, for the reason in "A control inside a rendering library's shadow
  root" above: no React tree reaches that markup, so no tooltip of the application's can be rendered there.
- A link inside a Markdown document in the file viewer. A `title` on an `<a>` is also the accessible description
  assistive technology reads, which a tooltip that only answers the pointer cannot supply.

That second reason is the rule the replacement has to be read against, for every tooltip but a control's own:
**such a tooltip answers the pointer and nothing else**, and is no tab stop. A control's opens on keyboard focus as
well, and the control is a tab stop either way (see "An icon-only control: tooltip and `aria-label`" above). The
anchored shape (`AnchoredTooltip`, which a run of running text takes, and a mark in a row that walks its focusable
descendants with the arrow keys, since neither can be a trigger of its own) is not associated with what it describes
for assistive technology at all. So whatever a tooltip says must reach a screen reader by another route: a clipped
label's full text is in the document whether it fits or not, a row carries its own accessible name, and an indicator
that means something on its own is a named `role="img"`.

One anchored tooltip is kept without that route: the real target of an OSC 8 hyperlink in a session terminal, whose
visible text may differ from where it leads. Nothing else carries the target, but the terminal's output as a whole
reaches no assistive technology (xterm.js keeps its rows `aria-hidden` and runs without its screen-reader mode), so the
tooltip withholds nothing the text around it would otherwise give.

### Hover marquees belong to one text label

A clipped name, heading or detail scrolls only while the pointer is over its own text region. Independent labels
within a row, card or header have independent hover regions; pointing at neighbouring text, an indicator, padding
or an action button must not start their marquees. A branch and its upstream counts may scroll together when they
form one text line.

Each run scrolls at reading speed, holds at the far edge for three seconds, then returns to its
resting position in about 200 milliseconds. The first reading pass starts after a 600 millisecond
hover delay; subsequent passes wait three seconds at rest. Leaving the hover region resets the label to rest immediately.

An entire menu item, selector, fixed-owner indicator, picked tag or diff's collapsed-line text control may trigger
its single text label as one interaction target. Such a scope must contain only that label; a picked tag's remove
button may share the scope so its text can scroll clear of the button. A row or header containing independent labels
or actions is not a shared marquee target.

The project filter's conditions on the Projects heading are one label in this sense: static tags, none of them a
control, that fade and run as a single marquee over the whole strip, so a long tag scrolls into view with the rest
rather than within its own pill.

Use the shared fading label's own hover region by default. Add `data-marquee-scope` only for the single-target
controls above, never to a general row, card or pane header. Verify long neighbouring labels with actual pointer
input: hover each label separately, hover its neighbouring controls, and check that leaving restores the label's
resting position. Verify reduced motion as well: clipped text stays still and its tooltip remains available.

### A tooltip on a mark that is not a control

**A mark whose meaning nothing else on screen carries gets a tooltip**: without one the meaning reaches assistive
technology alone, and a sighted user who does not already know the mark has no route to it at all. Two things count
as carrying it instead, and each leaves the mark without one:

- **A text beside the mark that already names it.** Focus mode's card shows the status glyph beside the status in
  words, and the branch badge's no-commits-yet state says so in the badge itself. The words are on screen either way,
  and a second route to them is a second thing to keep in step.
- **A tooltip already covering that spot.** The activity badge on a console's avatar in the rail says nothing of its
  own, because the avatar's own tooltip says the activity, and one on the disc would open in the one gap where that
  tooltip is deliberately held back — while hovering the avatar floats the hidden sidebar in instead. It would also
  put a second indicator trigger inside a real `<button>`, which this standard keeps as one exception, below.

One mark keeps its tooltip beside a text that names it: the agent icon, whose tooltip means the account the session
runs under wherever it carries one. A meaning the window has settled does not come and go placement by placement, so
it stays on a focus mode's card and an archive view's row. The text beside the icon names the agent alone; the
account remains in the icon's tooltip and in the session row's accessible name, or in visually hidden text where
there is no row name.

One gap is left open on purpose: no mark on a menu's transient surface carries a tooltip — a clipped path label in a
menu item does carry its own through the anchored shape, as in the Switch account submenu, but no indicator there
does — so the agent icon beside each session in the archive submenu says which agent it is to nobody, its `svg`
being `aria-hidden` as well. The agent is read from the archive view's own rows instead, where that icon carries its
tooltip and the agent is written out beside it.

**A tooltip is worded from the string that already names the mark**, not from one of its own: the same string where
it names the mark on its own, so the two cannot drift apart; its own key only where the only string naming it folds
it into the name of what carries it ("{name}, a session is waiting for you"), since reusing that would make the
tooltip repeat the row's visible name, and composing it out of a fragment is worse to translate.

The anchored shape takes the hover from its caller rather than listening for one itself, so it can be scoped to a
single mark wherever that mark can carry the pointer handlers: the Git mode's change row hands it the hover from the
status letter alone and anchors it to the letter, and the rest of the row — the name, the detail — keeps its own
tooltips.

Where such an indicator carries a tooltip of its own, that name goes **on the tooltip's trigger**, not on an element
nested inside it: react-aria describes the trigger, so a name one level in leaves the description attached to a
wrapper that is nothing to assistive technology. An indicator whose meaning already reaches it some other way leaves
the trigger unnamed instead, adding nothing to the name of the row or trail around it.

What is left over is a shortfall kept on purpose, marked at its code: a sighted user who works from the keyboard
reaches no tooltip, so a name that lives only there is a name they do not get.

One exception to the content model is kept on purpose: the trigger is a `div`, so an indicator inside a real
`<button>` — focus mode's console-switch chip — nests one where no content model allows it. The library offers no
element to put there instead, and the anchored shape, the only alternative, would answer a hover anywhere on the
button rather than on the indicator, which is the whole point of that tooltip. Nothing about the button changes for
it: the trigger cancels only the focus a mouse press would otherwise give it, and the press itself still reaches the
button, so pressing the indicator activates the control exactly as pressing its label does.

Cancelling that press has one cost, which only shows where the trigger wraps text rather than a glyph: a drag starting
inside it selects nothing, so the branch badge's text cannot be selected with the mouse in focus mode's header, where
the surface around it does not forbid selection. It is accepted there because the tooltip says the same text in full.

## Roles and states

### Expanded and selected

- A control that shows or hides a region carries `aria-expanded`.
- The selected row of a hand-built list or tree carries `aria-current` (as the sidebar rows do).
- HeroUI's `Tabs` and `ListBox` mark their selection themselves with `aria-selected` and get no `aria-current` on top.

### Standalone errors use the shared Alert

An error that stands on its own in a page, pane or dialog uses `ErrorAlert`, the shared HeroUI `Alert` with danger
status, its indicator, the danger-soft background and foreground body text. Neutral absence, loading and unsupported
content keep their state presentations. A field's validation stays in its `FieldError`, and so does a request's
refusal that is about one field's value, with focus moved to that field; a failure attached to a row, table cell or
rendered document stays in that content's own presentation. A dialog's own failure line scrolls itself into view, since
on a long form it can appear below the visible part of the body.
Toast errors use the Toast's danger variant and indicator, without nesting an Alert or a second live region inside it.
Warnings use warning status, rather than the error wrapper.

Keep the summary, explanation and recovery readable as separate parts when a failure needs them. A short error that
already says what failed needs no generic title repeating it. An optional title is a heading at the level of its
surrounding content: a startup page's is `h1`, and a subsection under a dialog or pane's `h2` is `h3`. Paragraphs,
lists and diagnostic blocks belong directly in the callout's content; HeroUI's span description cannot contain them.

Inline labels and technical literals use the shared `InlineChip`, backed by HeroUI Chip. Technical literals use
its monospace, left-to-right and wrapping options. A technical literal — a path, a file name, a URL, a branch,
a commit id, an ID, a command or a setting value — stands without quotation marks, as code where the text is rendered and bare in its plain-text form.
A name, title or control label quoted within UI copy uses a non-monospace chip instead of quotation marks.
Plain-text surfaces, including accessible names and system notifications, retain text without chip markup.
Raw diagnostics use `CodeBlock` or the existing source renderer,
preserving their text, direction and indentation; long content remains scrollable and reachable from the keyboard.
Keep structured error metadata separate from the plain error message, so notifications and unknown-error fallbacks
retain usable text.

Choose the chip size by its role, consistently across the UI:

| Role | Size | Typography and geometry |
| --- | --- | --- |
| A value within a sentence, heading, label, description, validation error, tooltip or toast; also a single technical value displayed on its own line in application copy | `inherit` | The surrounding font size and line height, on the text baseline. Typically 12px in field errors or secondary descriptions, 14px in body copy and labels, and 16px in dialog headings. No fixed chip height. |
| Dense metadata: project filter terms and session usage indicators | `xs` | 12px text, 16px line height and height, no vertical padding. |
| Independent status, type or size metadata: agent availability, file-change status, staging state, file size, Git-directory markers and report-history indicators | `sm` | 12px text, 20px height, HeroUI's compact preset. This is the component default for independent metadata. |
| Reserved presets, shown only in the component comparison; no current product use | `md`, `lg` | HeroUI's 24px and 28px heights respectively. Do not use them to enlarge values within copy. |

A path's length, monospace font or position on its own line does not select its size. Every technical literal
within copy follows the same `inherit` rule as a name or control label. Technical literals keep their monospace
code child while the chip's background uses the same font metrics as a name chip. Keep a sentence in ordinary
inline flow rather than making its text runs and chips separate flex items that stretch to different heights.
When a technical value completes a sentence, fill it through that sentence's localized placeholder; do not
retain a separate paragraph or a colon-led block just because the value used to be shown as code.
Long values wrap at slashes or anywhere they need to, within the available width; they do not select a smaller size.
Sizes belong to the shared component rather than caller-level font, line-height or padding overrides. Width
constraints for aligned metadata remain layout concerns.

The `monospace` option preserves a semantic `code` child without introducing a second visual component. Paths and
URLs can wrap after slashes, and longer uninterrupted literals can wrap anywhere. A confirmation word, file size,
status and technical ID use the same chip component. A typed confirmation word inherits the label's font size,
uses italic text, and keeps the ordinary UI font. A single technical value in application copy uses a chip
even on a line of its own, including the folder a trust confirmation asks about. Reserve `CodeBlock` for
multi-line code or raw diagnostics, never as a larger presentation of one path. Structured path displays
are separate from copy: file and directory lists, settings rows whose value is a path, and the viewer's
primary file-path row retain their shared path components, including their icons, clipping and full-path
tooltips. Path input fields and source or Markdown content also retain their own layouts.

Inline chips retain HeroUI's colours, soft fill and shape; the secondary hue is the shared purple extension.
Session usage chips use the same muted text colour as their neighbouring metadata or compact archive title.
`CodeBlock` uses the shared Typography wrapper for multi-line diagnostics. Outside an Alert, both default to neutral
and may choose a colour. `TintedAlert` supplies `CodeToneContext`: default stays neutral, accent maps to primary, and
success, warning and danger use their matching colours. The nearest status context takes precedence over an explicit
colour, including field errors and toast titles. Do not hand-colour individual literals within a status message.
HeroUI's Alert parts remain the content, title and indicator primitives. Syntax-highlighted source and foreign
renderer markup retain their own palettes.

An error appearing after the surface is open has one `role="alert"` owner, supplied by `ErrorAlert`; do not put a
second alert role on its parent or descendants. An initial startup explanation passes `announce={false}` and is
ordinary page content. Follow "Announcing a change" for other live announcements, and keep retry and exit actions
reachable when the error appears.

### A visual heading is a heading element

Text drawn to read as a heading is `<h1>`-`<h6>`, never a `<p>` carrying a heavier weight: the structure a screen
reader navigates by is the element, not the look.

The converse holds as well: a heading element still has to be drawn as one. A heading inside a container whose base
style sets the secondary text size and colour inherits both, and weight alone is too little to read as a heading — so
it carries its own size and colour instead. `EmptyPanel`'s full panel is the case: HeroUI's `.empty-state` puts
`text-sm text-muted` on the whole panel, and the heading overrides both so it leads the line under it.

A heading also never outranks the one above it. `EmptyPanel` takes `headingLevel` for that: its heading stands at the
level of the heading naming whatever it fills, so the two read as one thing. The default `<h2>` is the level of a
pane's own title (`ArchiveView`, `ProjectBrowser`) and of a dialog's, which react-aria sets for every `Frame.Heading`.
The sidebar's empty lists use compact rows instead, which add no heading to the outline.

### An empty state is a glyph, a heading, and what fills it

Every place that has nothing to show takes one shape, through `EmptyPanel`
(`packages/ui/src/components/EmptyPanel.tsx`): a glyph, a heading naming what is missing, optionally a line under it,
optionally the action that fills it and, beside that action, optionally a second way out of the state, in a centred row
that wraps (the whole shape, and which state takes which part, is `docs/product/empty-states.md`). Nothing builds a
second one out of centred text, and a state with no glyph is not one of these — HeroUI's bare `EmptyState` is reached
only through this component.

- The heading is a short phrase carrying **no punctuation at all**, a comma inside it included: a title that needs one
  is a sentence, and the part after the comma belongs in the line under it. "No sessions yet", not "No sessions yet,
  start one from the project row".
- The line under it says only what the heading cannot: why it is empty, where the thing is made, or a bare fact about
  the subject (the size of a body that cannot be drawn). It never restates the heading, and a state that has nothing
  more to say carries no line at all.
- The `compact` row, for a sidebar list in either normal or focus mode and for other nested lists, has no room for
  that line. Its message is plain text rather than a heading; where the list has one creation action, the whole row
  performs it. The main content's full panel carries the current action without a competing full panel in the sidebar.
- A load failure is not an empty state; it keeps the error shape, with its own way out (see the "Errors" section of
  `.claude/skills/knowledge-i18n-copy/SKILL.md`). What the subject *is* is a state of the subject, not a failed load,
  even when the panel warns about it: a project that is in no repository, a worktree that is gone, a repository Git
  itself refuses, are all empty states, since no retry would change the answer.

Where a region elsewhere already speaks the state, the panel takes `silent` and is hidden from screen readers so it is
not read twice. What that region says is then the only way to hear it, so it says the whole state as one sentence, from
a message of its own — a `.sentence` beside the heading and its line, never the two joined in code, which runs together
differently from language to language. The same message serves a state shown as a notice over content that is
displayed anyway, rather than as a panel standing in for it. `silent` cannot be combined with an action: a focusable
element inside an `aria-hidden` subtree keeps its tab stop and loses its name.

### Landmarks

The content panel always holds exactly one `main`, whichever of its views is on screen — the terminal pane, or the
welcome screen that replaces the three panes while there is no console. A view that is a region rather than the
window's main content carries `role="region"` with a name (`<section aria-label>`), per the naming rule above.

### Announcing a change

State that changes without the user acting (connection, a terminal problem) is announced from a `role="status"`
element, an error from `role="alert"`.

- A `role="status"` region meant to announce text that comes and goes stays mounted, empty when there is nothing to
  say, and only its contents change. One rendered conditionally, already holding its text when it is inserted, is not
  reliably announced (VoiceOver in WebKit among others).
- Even a region that stays mounted afterwards is not announced for the text it was first rendered with.
  `StatusAnnouncer` defers its first write for that, so announce a state with it.
- VoiceOver also drops a status written within about 300 ms of a focus move (a dialog opening, say), so a surface that
  takes focus as it opens delays its first status write (`StatusAnnouncer`'s `firstWriteDelayMs`).
- A state whose visible form is durable copy rather than a placeholder — the sentence saying where a macOS permission
  stands, in `packages/ui/src/settings/PermissionsSection.tsx` — announces from a `role="status"` on that copy itself
  instead. `StatusAnnouncer` is `sr-only` and expects the visible form to be `aria-hidden`, which this cannot be: it
  is the row's own description, and a reader going through the row statically has to reach it. Only the first text is
  lost that way, and that is the state as the surface opened, which the user is reading anyway; every later change —
  the one that matters here, made in System Settings and picked up on the way back — lands in a region that has been
  mounted all along. Put the subject inside the region when several of them can change independently: `role="status"`
  is atomic, so a bare "Granted." names nothing.
- A dialog step whose title says what happened announces it by moving focus to the dialog, wherever focus was, so the
  dialog is read out by its new name; it adds no `StatusAnnouncer`, which would say the same thing again. The import's
  result step is the instance: its title becomes "Import finished" and the step focuses the dialog itself
  (`packages/ui/src/configuration/ImportConfigurationDialog.tsx`). That VoiceOver reads the renamed dialog on this
  focus move has not been listened to yet; confirm it once per "Reusing VoiceOver verification".
- HeroUI's `Alert` renders no role at all, its name notwithstanding. An error that can appear in one after the user has
  moved on puts `role="alert"` on an element of your own inside it.

### Reusing VoiceOver verification

Confirm a new announcement mechanism once in the packaged macOS application with VoiceOver. Reusing an existing
mechanism does not require another listening pass for each caller or message. Check each caller's wiring with
automated tests and browser inspection instead: the region's lifetime and text changes, duplicate suppression, focus,
and whether the region or an ancestor is hidden by `aria-hidden` or `inert` while the real overlay is open.

A new mechanism, or a change to its live-region role, mounting or timing behavior that the existing implementation
does not cover, needs one representative real-device confirmation. A new caller or location alone is not a new
mechanism: reusing `StatusAnnouncer` with React Aria's existing live-announcer exemption from modal hiding remains
reuse when the checks above establish the same accessible region and update behavior. Record actual real-device
confirmations with their mechanism, environment and observed result so later callers can reuse that scope. Existing
code and passing DOM tests establish the implementation, not a history of actual VoiceOver listening; do not report
them as a listening result.

The native VoiceOver caption window displaying an announcement is accepted as real-device confirmation of that
announcement. Record it as caption verification, with the displayed text and the state that caused it.

**Confirmed on 2026-10-14:** in the packaged application's WKWebView on macOS 26.2 (25C56), with Simplified Chinese
and the dark appearance, the connection banner's persistent `StatusAnnouncer` and 100 ms delay per outage produced
the reconnecting message in VoiceOver's native caption window after the connection dropped, then the disconnected
message when automatic retries were spent. The connected window produced no connection announcement. This confirms
the banner's announcement mechanism; cancellation before the delay and duplicate suppression are covered by the
component tests and browser inspection.

## Contrast

### Thresholds

- Text: at least 4.5:1 against its background (3:1 for large text).
- An icon or a state indicator: at least 3:1 against what it sits on.
- A boundary: at least 3:1 too, when it alone shows where a control is.

### Do not change how a HeroUI component looks

Keep HeroUI's default appearance, including placeholder colours and hover opacity. A contrast measurement alone does
not justify changing its colours, theme tokens, borders, frames or outlines. Use the component's own supported variants
and slots; layout and sizing adaptations do not authorize replacing its visual treatment.

### Form controls on a surface

A form control on a surface — in a dialog, a popover, a card — takes HeroUI's `variant="secondary"` (`TextField`,
`Input`, `Select`, `Checkbox` alike), HeroUI's own variant for that case. The default variant leaves it blending into
the surface in both appearances.

### Hand-built rows and cards

A hand-built row or card that its content already identifies takes HeroUI's separator colour for its outline
(`border-separator`, as the focus-mode session cards do), even though that is under 3:1. Its selected state still has to
reach 3:1 (`border-accent-glyph`).

A locked session keeps a yellow inset outline without changing its size or position. Its persistent lock button
also communicates the state through its Unlock accessible name and tooltip, so the outline is decorative. Hovering
or keyboard-focusing the button changes the glyph to an open lock; unlocking hands keyboard focus to the row before
the button disappears. The focus ring remains visible alongside the lock outline.

Focus-mode session cards use the opaque `--panel` surface over the sidebar's `--panel-sidebar`, with the existing
hover and selected tints on top. Their titles use the shared single-line fading label, including its full-name
tooltip and hover marquee, rather than an ellipsis or a line clamp.

### A colour mark whose meaning is in words

A colour mark that only echoes what words beside it, its tooltip or its row's accessible name already say is decorative,
and is not held to the 3:1 bar for a state indicator. A console session's colour is the case: its dot on a row and the
swatches it is chosen from carry each palette colour as its name suggests (yellow is yellow, not a darkened olive that
would clear 3:1 against a light surface), since the session's name is always there to tell sessions apart. A pale entry
still has to be seen: the dot and the swatches carry a faint inset ring of the foreground (`.session-colour-ring` in
`packages/ui/src/style.css`), since the picker's swatch has no edge of its own. A mark that is the only carrier of a
state stays at 3:1.

The swatches sit in a submenu of the session's action menu, so the Colour item is announced as opening a submenu
(`aria-haspopup` from react-aria's submenu trigger) while what it opens is a list box of swatches, named by that item.
The mismatch is accepted rather than fought: overriding the trigger's popup type would mean replacing react-aria's
submenu handling. Each swatch is named by the colour description react-aria generates in the window's language, so
the palette is tuned to keep those names distinct in every offered language and both appearances, with the exceptions
the hues cannot reach: react-aria's Arabic names give violet and fuchsia one name, and its Korean names give one word
to green-cyan, cyan and cyan-blue (teal and cyan collide in light, teal and sky in dark). `sessionColours.test.ts`
lists them and fails on any other.

### Row action menu buttons

File, Git, project and session rows, focus cards and pane headers use `ActionMenu`'s shared trigger
for their ellipsis menu. Keep its 24-by-24-pixel target, 16-pixel glyph, rounded corners and focus
ring consistent. A caller changes the trigger's layout or content only when its placement needs it;
it does not choose separate colours for the same ellipsis action.

- At rest, the trigger is transparent with a muted glyph. A row's selection reveals its
  actions without painting each button as active.
- While hovered or while its menu is open, the trigger uses `bg-default` and a foreground glyph.
  Its wrapper supplies the shared `sidebar-fills` family so Files, Git and sidebar menus use the
  same translucent hover tint, independent of the surrounding pane's control fills. Keep this
  subtle tint; do not replace it with a solid muted fill or an inverted glyph.
- The glyph must reach 3:1 against its actual background in every state. The hover tint does not
  need 3:1 against the row: the glyph identifies the control, rather than a filled boundary.
- Keyboard focus keeps the existing focus ring. Revealing an action must not leave it dimmed once
  its visibility transition finishes.

Measure both appearances on the rendered resting, hovered and selected surfaces. Read the final
computed colours after transitions settle, composite translucent ancestor fills, and check the
glyph separately from the button fill. The row's own hover tint is not the button's hover feedback.

### HeroUI's own text colours

HeroUI's stock text and placeholder colours retain the library's default treatment, even where a measurement falls
below the custom-style thresholds. Do not add colour or opacity overrides to make its default components pass those
thresholds. Application-authored text on a custom surface is checked against its actual background in both appearances.

### Where to fix a shortfall

- Fix a shortfall in application-authored styling at its custom part or shared application token.
- When an application-authored surface causes a shortfall, adjust that surface rather than overriding the HeroUI
  component's default appearance.
- Do not treat an existing component override as permission to add another one.

### Finding every rule that draws a colour

A custom token reaches only the rules that read it. Inspect the rendered element and computed styles to find which
rule supplies a colour before changing application-authored styling. HeroUI's compiled stylesheet
(`packages/ui/node_modules/@heroui/styles/dist/heroui.min.css`) is the reference for its defaults, including where an
input inherits a placeholder colour from preflight rather than a component-specific rule. That inheritance does not
make the default placeholder an application-authored style.

### Surface-filled components in a dialog or popover

Choose the component's supported variant or status for a dialog or popover and keep that variant's default fill,
foreground and shadow. Do not replace those defaults solely because their measured contrast or separation falls below
a custom-style threshold. Any additional application-authored surface follows the custom-style requirements.

### Auditing a change to a colour token or a surface

A change to an application-authored colour token or surface can touch every screen, so its audit covers every gallery
scenario it affects in both appearances. HeroUI's default visual treatment is not redefined by that audit. It also
measures what a pass over text and glyphs misses:

- Every fill — a chip, a tertiary button, a ghost button's hover — against the surface it sits on.
- An animated state at its faintest frame. An `animate-pulse` glyph sank to about 2:1; animate such a glyph without
  lowering its opacity.
- A translucent fill (HeroUI's soft chip is 15% of its colour) as drawn: composite it over each surface it actually
  lands on — a selected and a hovered row, a dialog's `--overlay` — and measure the label against that. Measuring
  against the bare surface or the fill's own colour overstated one chip's contrast and missed a 4.5:1 failure on
  selected rows.

A ratio read off a capture of the packaged window carries the display's own colour profile, so it says which side of a
threshold a pair lands on and nothing finer: the selected toggle's fill read 3.26:1 light and 5.0:1 dark against its
siblings where the tokens give 3.67:1 and 4.58:1. Take a recorded ratio from the tokens, never from a capture.

### A region with a surface colour of its own

An application-authored surface must be checked with the text and controls actually rendered on it. Prefer a surface
that works with HeroUI's default controls, and use the component's supported variant for that context. If custom
styling causes a shortfall, fix the custom surface or styling rather than changing the component's default colours or
visual treatment. Application-authored labels, glyphs and state indicators are checked at rest and while hovered or
selected, compositing translucent fills over their actual backgrounds.

### A glyph or tint on the window material

In the macOS app the top bar and the rail show a translucent window material whose colour shifts with whatever is behind
the window (see "The window chrome and the content panel" in `docs/product/window-layout.md`). A glyph or tint added to
that chrome is measured against the material's measured colours, given in the comment on the window's surfaces in
`packages/ui/src/style.css`, not only against `--chrome`, which is just the browser's stand-in for the material. The
dark material is a grey far lighter than dark `--chrome`.

## Pointer target size

A control's pointer target is at least 24 by 24 CSS pixels (WCAG 2.2 AA 2.5.8). HeroUI's own sizes already clear it —
its smallest icon-only button is 32 — so this only bites where a control is drawn smaller than HeroUI would draw it.

Icon buttons in sidebar rows, cards and headers, the right pane and the file viewer share a 24-by-24-pixel target
and a 4-pixel gap, defined by `--row-control-size` and `--row-control-gap` in `packages/ui/src/style.css`.
Use the shared `RowIconGroup` for adjacent
icons and `RowIconSlot` for a marker's matching horizontal column; the marker keeps its own glyph size and does not
increase the text line's height. Leading glyphs use the 16-pixel `--row-leading-icon-size` column through
`RowIconSlot`'s leading placement. In a focus card, the binding dot precedes the title in reading order and shares
its horizontal centre with the status and agent glyphs. All three lines use `RowIconLine` with the same 8-pixel
`--row-leading-icon-gap`, so their text starts in one column; an unbound title keeps the empty leading slot.
`RowControls` keeps persistent pins and markers outside the hidden actions' clip and collapses
the actions' leading gap with them. Do not space individual trailing icons with the text row's gap or add local
offsets. Verify the shared columns with actions hidden, hovered, focused and open, including right-to-left layout.

The criterion is about controls, so a mark that only opens a tooltip is not held to it: a tooltip is not an action,
and a mark too small to aim at reliably costs a hover that can be tried again, not an activation. Several such marks
are drawn well under 24 — the binding badge on a session row, a branch's responsible-session badge, a change row's
status letter — and the press under them, where there is one, belongs to the row and keeps the row's own target.

Neither exception in the criterion is easy to rely on here. *Spacing* needs a 24-pixel circle centred on the target to
touch no other target, which a small control placed over a larger one (a delete badge on a corner) fails by
construction. *Equivalent* needs the same action to be available from another control on the same view, which a lone
affordance does not have.

Where the drawn size has to stay under 24, extend the target rather than the drawing, as HeroUI does for its own
select clear button (`.select__clear-button:after{inset:-2px}`): an absolutely positioned `::after` with a negative
inset enlarges what the pointer hits and changes nothing visible. The avatar field's delete button
(`packages/ui/src/dialogs/AvatarField.tsx`) is drawn at 20 and hit at 24 that way, and so is the pressable part of a
checkbox whose label is one line of `text-sm` (`TrustDirCheckbox` in `packages/ui/src/dialogs/TrustPromptDialog.tsx`),
which HeroUI lays out at the line's own height.

A control whose height is set in units of something else inherits that thing's constraint. The separators of
collapsed lines are sized in multiples of the diff's own code line height rather than in pixels, so that they keep
their proportion to the code, and the rendering library halves a bar carrying two controls between them
(`grid-template-rows: 50% 50%`, under `pointer: fine`). So it is the multiplier, not the drawn glyph, that sets such a
control's target, and it is the thing to check whenever that line height changes.

One shortfall sits here, kept on purpose and marked at its code (`diffCss` in `packages/ui/src/viewer/renderer.tsx`):
those bars take GitHub's own proportions, 1.4 lines for one control and 2 for a pair, which at a 20-pixel line is the
28 and 40 pixels GitHub draws. Halved, the pair are 20-pixel targets, under this floor — as GitHub's own are. The user
chose the proportions over the criterion knowing that. Every other control of a separator clears 24: the one-control
bar at 28, the separator's own text and Show whole file at the full height of their bar.

## Not by colour alone

A status or state that differs in colour also differs in glyph, shape or text.

**A two-state view control carries its state in its icon.** The shape is one icon button whose icon is the state it is
in and whose accessible name and tooltip say what pressing will do, so the name alternates with the state (see
"Two-state icon toggles" in `docs/product/labels-and-tooltips.md`). Nothing distinguishes the pressed button from the
unpressed one by fill: `.icon-toggle` in `packages/ui/src/style.css` resets HeroUI's selected tokens back to the
resting ones, and `aria-pressed` carries the state for assistive technology. The window chrome's two pane toggles were
already built this way; the Git mode's two view toggles — the one that shows and hides the change filter field, and
group by folder — and the viewer's three view controls now are too.

A two-state control that is also what shows and hides a region carries `aria-expanded` beside its `aria-pressed`, the
two saying the same thing in the two vocabularies: the Git mode's filter toggle is pressed while its field is on
screen, and expanded for the same reason (`expanded` on `IconToggle`). The region it shows has no id of its own and so
no `aria-controls`; "Expanded and selected" above asks only for `aria-expanded`. A region taken away this way hands
keyboard focus back to the toggle when it was holding it, the toggle being pressed without taking focus itself; the
hand-off's ring follows how the press came, as "Focus moved by a change the user did not make" above has it.

**A notice carries its severity in a glyph, never in its fill alone.** The file viewer's notices — what is said above
content that is still shown — are one block in three tones (`Notice` in `packages/ui/src/viewer/viewerStates.tsx`):
an explanation of how the content is drawn, a warning about the content itself, and a part of it the viewer could not
show. Each is a HeroUI `Alert` of its own status, which draws the glyph that goes with it — the circled `i`, the
warning triangle, the circled `!` — from `Alert.Indicator`. Notices above a reading surface take the fill named by
the tone. The glyph is what carries the tone: those fills sit at 1.1–1.5:1 against the dialog body and say nothing on
their own. Measured on those filled strips, in both appearances: the sentence is 11.9:1 or better in every tone and
the glyph 5.1:1 or better, against 4.5:1 and 3:1. Block notices embedded in a Markdown document use the same filled
Alert and status glyph, with compact spacing. A notice that sits inside a paragraph
cannot be that Alert: an Alert is a `div`, and a `div` inside a `p` is not valid. `InlineNotice` in
`packages/ui/src/viewer/inlineNotice.tsx` is a `span`. The sentence inherits the document colour, which already meets
the contrast above, and the same three glyphs tell the tones apart. When that notice appears after the first paint,
it is announced by `StatusAnnouncer` rendered as a `span`, with the viewer's own first-write delay; the visible
sentence is `aria-hidden` so it is not read twice. The error block for a subject that cannot be
opened at all uses the shared error Alert with the same danger fill and glyph, so an error is never quieter than a
notice about one part of it. Its reason uses foreground text on that fill, measured at 13.43:1 light and 13.28:1 dark.
A viewer with nothing to draw takes neither: it fills the body with a centred glyph and sentence (`NothingToShow`).

**One kept shortfall is left, and it is a single control**: the settings dialog's three-way appearance choice, which
is a toggle group where the selected button differs by fill colour only. Which button is selected is also carried by
its position and by `aria-pressed`, and the fill difference meets 1.4.11's 3:1 for a state. The reasoning and the
measured ratios sit in the comment above `.toggle-button-group .toggle-button` in `packages/ui/src/style.css`; a
two-state choice does not belong in that group, because the icon form above has no shortfall to accept.

## Text direction

Under a right-to-left language the window mirrors, and what mirrors with it is settled per part in "Right-to-left
layout" in `docs/product/window-layout.md`. What follows is the rule behind that list and how to meet it in code; it is
WCAG 2.2 A 1.3.2, since a value read in the wrong order says something else.

**Prose follows the interface; a value keeps its own order.** A sentence takes the document's direction. A value that
is not prose — a path, a URL, a branch name, a commit id, a shell command, a port, a keyboard shortcut, a count written
as `1 / 12`, a literal to type, a diagram, a formula — reads left to right in every language, and the element that
*draws* it says so with `dir="ltr"`.

An inline formula sits inside a paragraph, so that element is also a bidi isolate: `dir` on it is not always enough
once a utility class resets `unicode-bidi`, and Tailwind's `isolate` does not do this job (`isolation: isolate` is a
stacking context). The formula sets `unicode-bidi: isolate` itself (`FormulaView`).

- Say it on the element that draws the value, not on one inside it. A field whose group holds an affix needs the
  `dir` on the group: with it on the input alone, the affix stays on the mirrored side and the text pulls away from it,
  and the two stop reading as one control (`packages/ui/src/settings/CloneDirSetting.tsx`).
- `dir` sets direction, not alignment. It does not override an inherited `text-align`, so an element that needs both
  keeps its alignment class as well (`packages/ui/src/ErrorBoundary.tsx`, whose crash stack sits inside a centred
  wrapper).

**A value substituted into a sentence is isolated.** Without it the bidi algorithm reorders the value against the
sentence around it: `“/Users/dev/app”` reads as `”Users/dev/app/“`, and a trailing full stop or a closing parenthesis
lands on the wrong side. Two forms, one per kind of text:

- rendered as React children: `<bdi dir="auto">{value}</bdi>`;
- as a plain string — a tooltip, a `title`, a toast, an accessible name: `isolate` from
  `packages/ui/src/i18n/isolate.ts`, which wraps the value in U+2068 / U+2069. The isolates are invisible and ignored
  by assistive technology, so an accessible name carrying them still reads as its text.

Isolate where the value enters the sentence, and prefer the point every value of a kind passes through: `daemonMessage`
isolates every param of a daemon message, which covers a browse error's too. A name the user typed is isolated for the
same reason as a path — it may be in any script.

Two cases take no isolate, and putting one there breaks something:

- **A string that is matched, sorted, keyed or handed to a collection as `textValue` is never isolated.** The isolate
  characters are collation-ignorable, so a comparison stops matching: react-aria's type-to-select compares `textValue`
  with a collator, and a leading U+2068 makes typing `m` skip `main`. Keep the plain value for those and isolate only
  the copy that goes into a sentence.
- **A value rendered as the children of an element that carries a `dir` needs none.** That element already isolates its
  content, so only a value going into a *string* needs `isolate`. A string wholly wrapped in an isolate also has no
  strong character left for `dir="auto"` to read, which can lay the element out left to right.

`packages/ui/src/browser/BranchComparison.tsx` shows the split: the branch name is plain for `textValue`, for the
children, and for the `title` that is the name alone, and isolated only where it enters a sentence — the `title` that
says the branch is gone.

**An icon mirrors only when its meaning is a direction.** Back and Forward, a disclosure or submenu chevron, a pane
toggle, a glyph whose rows hang off the leading side (the fold control, the list and folder-tree layout toggles):
`rtl:-scale-x-100` on the icon. A glyph standing for a physical thing — pause, a rotation, a camera — and a checkmark
keep their shape. A component library's own indicator is worth checking rather than trusting: HeroUI's submenu
indicator is a fixed right-pointing chevron, while react-aria opens the submenu towards the inline start, so
`ActionMenu` flips it itself.

**A badge or overlay placed with physical offsets needs an `rtl:` twin.** Tailwind's logical utilities mirror
themselves, but a hand-placed decoration — and any mask cut for it — does not: give each one its mirrored counterpart,
as `packages/ui/src/components/Rail.tsx` does for the waiting count and its hole. Where a library offers only physical
placements, pick the placement from the language (`textDirection(useCurrentLanguage())`).

Verify a direction change by rendering it, not by reading it: the gallery serves every scenario under `ar`, and a
run of it against `en` at the same width catches a part that fails to mirror as readily as one that mirrors when it
should not (see "The UI state gallery" in `packages/ui/README.md`).

## Motion

An animation or transition that is not essential stops under `prefers-reduced-motion: reduce` (Tailwind's
`motion-safe:` / `motion-reduce:` variants).

### Dynamic collections

When items are added, removed, reordered or folded within a visible collection, surviving items that change
position slide into place using the shared list-motion implementation. This applies to navigation lists, settings
records, selected tags, wrapping chips and virtualized file and change collections. Use stable item identities,
not array indices or translated labels. Horizontal and wrapping collections move on both axes, including under RTL.

Use a 300 ms ease with `cubic-bezier(0.2, 0, 0, 1)` for position changes. In nonvirtualized collections, new items
fade in from the left while removed items fade out to the right as inert, accessibility-hidden visual copies.
These effects move by 25% of each item's own width over 300 ms with `cubic-bezier(0.4, 0, 0.2, 1)`, starting alongside
the surviving items' movement. Removed items stop accepting input immediately; do not delay an action or keep a
removed control reachable to play an exit animation. Preserve keyboard focus on surviving controls and retain the
surface's focus handoff when the focused item is removed.

Initial rendering, scrolling, viewport recycling and container resizing do not trigger item movement animations.
Virtualized collections keep virtualization and animate only surviving rendered identities whose layout changes.
Static choices, a replacement page or directory listing, and fixed result snapshots do not require collection
motion. Collections whose component already provides the same behavior, such as the toast stack, keep that behavior.
With reduced motion requested, update positions immediately, cancel any entry or movement already running and
remove exit copies.

Use `useListMotion` from `packages/ui/src/components/useListMotion.ts`, marking items with `data-list-motion`.
Its descendant mode supports semantic wrappers and `display: contents`; its virtualized mode follows rendered
identities without replacing the collection's layout transforms or focus management. Browser collections use
`useBrowserListMotion` to preserve their existing scroll and focus refs. Keep animation timing and preference
handling in the shared implementation rather than writing a separate transition at each caller.
For a lazily mounted choice popup, use `MotionListBox` inside the popup so the observer mounts with the list and
captures its initial positions before the first live choice update. Apply the same motion rule to choices that
refresh while the popup remains open; a static choice list simply has no movement to animate.
