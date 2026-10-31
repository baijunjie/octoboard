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

The user has kept a few shortfalls and overrides on purpose, each marked by a comment at its code saying so. Leave one
that is marked as it is, and do not add a new exception without the user's say.

One such kept shortfall is the Stop button's label on a process card in the dark appearance, below 4.5:1 (4.47:1 at
rest and 4.2:1 with Stop hovered); see "Hand-built rows and cards". The card is a hand-built surface, so it is a shortfall
of application-authored styling that the user chose to keep, not an approved departure from a HeroUI component's look.

### Confirmed visual choices

#### One definition per semantic colour

Every colour that carries a meaning — neutral, accent, success, warning, danger, and the project's purple help colour —
comes from one definition, used the same way everywhere in the application. HeroUI 3's own semantic tokens are that
definition wherever HeroUI has one; the project defines a token only for a role HeroUI lacks, once, in
`packages/ui/src/style.css`, and every component reads it from there. No component hard-codes a semantic colour, keeps
a second recipe for a role that already has a token, or overrides a HeroUI component's semantic colours locally (a
modal body's text colour, below, is the one exception).

| Role | Source |
| --- | --- |
| Solid fill and the text on it (a primary button, a count badge, a selected segment, the resize line) | HeroUI `--{colour}` and `--{colour}-foreground` |
| Soft fill (an Alert's background, a chip's and a badge's fill) | HeroUI `--{colour}-soft`; the neutral fill of a chip or badge is the one below |
| An icon or glyph of that meaning on a surface (a failure mark, a status dot, a lock, a Git badge, the session context badge's ring and its card's meter fill); a destructive menu item's icon takes its label's `--danger`, as HeroUI colours that label | HeroUI `--{colour}-soft-foreground`, the role HeroUI itself gives an Alert's and a Toast's indicator |
| Text in that colour (a chip's and a badge's text, +N / −M line counts, the diff viewer's added and removed text) | The project's tint text token: HeroUI's soft foreground pulled toward the foreground just enough to keep 4.5:1 on every surface (neutral: HeroUI's `--default-foreground`, which has no token) |
| A hairline edge (a bordered badge) | The project's tint edge token, mixed from HeroUI's solid colour |
| Focus | HeroUI `--focus`, for HeroUI controls and the application's own rings alike |
| Accent, links included | HeroUI `--accent`; link text uses the accent text token where it sits in copy |
| Secondary text | The project's `--app-muted`, HeroUI's `--muted` missing 4.5:1 on a selected sidebar row |
| Help, the project's purple for help and hints | A project token (`--help`), HeroUI having none, with its soft fill, text and edge derived like the others |

Native colours that cannot read CSS — the menu bar icon's status colours — are generated from the same values rather
than written down separately. The terminal's ANSI palette, syntax highlighting, agent brand colours and identity
colours (avatars, session dots) are not semantic and keep their own palettes.

Secondary text is a choice made where the text is written, never a default inherited from a container. A modal's body
text — the message of a plain confirmation, a confirmation's consequences, an error alert's description, a form
dialog's explanatory sentence — is what the user reads to decide, so it is in the foreground colour, in every modal
alike. HeroUI gives its modal and alert-dialog bodies `text-muted`; the shared modal frame (`Dialog` in
`packages/ui/src/dialogs/Dialog.tsx`) sets its body back to the foreground, so nothing in a modal turns grey by
inheritance (the user's decision). Text inside a modal that is secondary — a field's note, an eligibility hint, a
line saying what an option leaves out — takes the secondary text colour above, `--app-muted` (`text-app-muted`),
explicitly on its own element; a HeroUI part that colours itself (`Description`, `Alert.Description`) keeps its own.

The user accepts that HeroUI's accent, used as the solid accent and the focus ring, falls below 3:1 on some light
surfaces — a selected sidebar row, a control fill and its hover, the window chrome — in exchange for one accent colour
across the application. A selected segment of a toggle group keeps HeroUI's own selected treatment.

The window's full-width status strips are the one place a solid semantic fill stands behind a line of copy: the update
notice (solid accent) and the connection banner along the bottom edge while the daemon connection is down (solid
warning, `--warning` with `--warning-foreground` text). A strip is not an Alert and is not drawn as one: it is a band
across the window, the state of the whole application rather than of what is on screen, and its buttons are outline or
ghost buttons retinted to its foreground, none of them a primary. Its content is centred, so its main action comes
before a dismissal (Later, or the close glyph). Beside the strips, a solid warning or danger fill is only a small
glyph-sized mark such as the waiting session's disc; no surface holding copy, apart from a button, takes one.

An Alert's only custom visual treatments are its status background, HeroUI's `--{colour}-soft` for its status, and its
shadow: HeroUI's `--surface-shadow` keeps its shape but takes the status colour instead of grey, so a tinted Alert has
no grey rim. All other styles and component structure stay stock HeroUI 3, as specified under "Errors shown among other
content". The stock `Alert.Description` keeps its colour on that fill; where it measures below 4.5:1 it is HeroUI's own
text on HeroUI's own fill, not recoloured.

Every unselected, enabled HeroUI tab keeps its label fully opaque on hover instead of dimming, by one shared rule in
`packages/ui/src/style.css`; a tab's own opacity (a closing terminal tab's) still applies while hovered. The tab strips
in the app also take the foreground colour on hover. Preserve this shared visual treatment as an explicit product
choice, while retaining the component's layout, selection and keyboard behaviour.

A status label such as "Not added", "Not installed" or "Logged in", and any other qualifier after a name (a session's
account in its agent mark's tooltip where that names only the agent and its account, "Not in the list", a file's change
status or size, "Read-only"), is a badge after the name, never parenthesised text in the name's own style and never an
`InlineReference`. It is the shared `StatusBadge`, the only small badge component. It takes a semantic colour —
`default` (neutral), `accent`, `success`, `warning`, `danger` or `help` (the project's purple, for help and hints) — and
one of two variants:

- **Borderless** (`variant="soft"`), the usual choice for a state: login state, a tool's installed state, a file's
  change status, "Read-only".
- **Bordered** (`variant="bordered"`), for a qualifier that annotates the name before it: the account, "Not added",
  "Same as console" and the value it resolves to, the context size. It adds a hairline edge in its colour.

`StatusBadge` uses HeroUI's `Chip` with `sm`, `md` and `lg` sizes (20px, 24px and 28px tall). The default is `sm`,
with 12px text and 8px of padding on each side, so metadata such as the session context size stays compact. Use
`lg` for the granted-permission state in Settings, with a 16px circle-check icon before its label. A single-character
badge is square at each size; a wrapping badge can grow taller to fit its content.

Badge and inline reference serve different purposes but share one colour configuration: a single set of generic tint
tokens in `packages/ui/src/style.css`, per colour a fill, a text colour (neutral reads HeroUI's `--default-foreground`
instead of a token) and an edge, which both components read, so a chip and a badge of the same colour never differ. A
coloured fill is HeroUI's `--{colour}-soft`, its edge the solid `--{colour}` mixed down (HeroUI has no border token);
the neutral fill is the foreground at 8% in light and 10% in dark, translucent like the coloured ones, so it is close to
HeroUI's `--default` over white and deepens on a grey surface (a hovered or selected row) instead of vanishing
into it, 1.19:1 to 1.20:1 off a light surface and 1.14:1 to 1.31:1 off a dark one. Its edge is the secondary text colour
at 28%. Only the bordered badge draws the edge; a chip never has one.

Every colour and variant keeps at least 4.5:1 for its text on every surface it sits on (fields, dialogs, popovers,
tooltips, sidebar rows and cards), measured as drawn over each surface; where HeroUI's soft foreground falls short on
any surface it sits on, that colour's text token is mixed toward the foreground, once per appearance, rather than
dropping the colour. A badge has a screen-reader-only comma between name and qualifier. A badge never sits in wrapping
running copy: it belongs at the end of a name, row or label line. A user-typed value in it takes `dir="auto"` and a path
`dir="ltr"`; in a tooltip, whose width is bounded, it wraps instead of overflowing. A badge that completes its name
rather than qualifying it ("Same as console" and the value that resolves to) sits inside the name's fade in a select and
is clipped together with it. The session context-size indicator is the same badge on its own, without the comma; where
the agent reports a context window, a small HeroUI `ProgressCircle` in the tone of the share used leads the number
inside the badge, live and archived sessions alike, its exact counts in the trigger's name; an archived session's
history pane shows the same indicator after its heading. An agent row that cannot be picked is dimmed by its name taking
the secondary text colour (still at least 4.5:1) and its icon fading; the text is never faded below AA by opacity.
A Files row Git ignores is dimmed like the sidebar's inactive projects: a veil of the pane's surface at 55% lies over
the row, taking its name below AA (about 2.9:1 in light, 4.2:1 in dark) by the user's decision, and each row lifts its
own veil while it is pointed at, selected, holds keyboard focus or has its menu open. The dimming is only a change of
lightness, so the row's accessible name carries the state (", ignored").

A select option or menu item that cannot be chosen, whether or not it says why (an account whose quota is used up, a
terminal's Copy with nothing to copy while the program takes the mouse), is dimmed as an agent row is: its name takes
the secondary text colour (`text-app-muted`, at least 4.5:1 in both themes) and only its icon fades, with HeroUI's
`--disabled-opacity`. HeroUI's fade of the whole row is switched off (`data-[disabled=true]:opacity-100` on a menu item;
a select option never gets `data-disabled`, see below), and so is its fade of a menu item's `Label`
(`[data-disabled=true] .label`, cancelled with `opacity-100` on the `Label`), so no text is faded below AA by opacity.
The option's badge, checkmark and a reason line under the name stay at full opacity; the reason is the text that
explains the disabled state, so it must stay readable. A menu's note, a line of text disabled only so that it cannot be
chosen, keeps its muted text at full opacity the same way. HeroUI has a disabled menu item's row ignore the pointer; its
name takes the pointer back, so that a clipped name shows its tooltip and runs its marquee while the pointer is on it.
The row then matches `:hover` and `:active` under the name, so HeroUI's hover fill and press shrink are cancelled on it
(`data-[disabled=true]:hover:bg-transparent`, `data-[disabled=true]:active:transform-none`), and the name shows the
arrow the rest of the row shows rather than HeroUI's `not-allowed` (`cursor-default`). These overrides change HeroUI's
default disabled look, an approved exception by the user's decision (see "Do not change how a HeroUI component looks").

A used-up account in the rail's quota panel, which is not a control, recedes the same way without fading anything: its
name and its meters' headlines take the secondary text colour (6.0:1 light, 5.7:1 dark on the panel's popover), a
warning-coloured "Limit reached" badge on its name line says why in words, and its bars are drawn in a neutral grey
(`.quota-meter--used-up` in `packages/ui/src/style.css`), fainter than the agent colours. Settings' accounts table marks
such an account alike, the name muted and the grey bar followed by the same badge in its Usage cell. The menu bar icon's
quota submenus draw such an account's bars in the same grey, `--meter-used-up-fill` read back for the menu's appearance,
while the account's heading says "Limit reached" in words. A veil over the row is not used for that state: it would take
its text below AA.

A disabled account is left out of that panel and of the menu bar icon's quota submenus, except the one the selected
session is running on. That row is drawn like any other, its name, meters and bars unmuted, since the account is in use,
and a warning-coloured "Disabled" badge after its plan says in words why it is listed; the submenu's heading for it
ends in "Disabled" after its quota state. The submenu heading carries the state in words only. In the Settings page's
accounts table a turned-off account's Status cell says "Disabled" in a neutral badge in place of its login state; the
badge opens the cell, so it has no screen-reader comma before it.

A kept shortfall of that panel: while the mouse is on the rail's **Account quota** button, every account row but the
button's own is covered by the sidebar's inactive-project veil (`INACTIVE_VEIL_CLASS` in
`packages/ui/src/components/inactiveVeil.ts`), until the mouse enters the panel or the panel closes. The veiled text
falls below AA by the user's decision, as a passing cue that is mouse-only and visual only; it looks unlike the used-up
state, which recedes without a veil.

A notice toast (HeroUI's accent variant) tints its indicator with the title's `--accent-soft-foreground`, as HeroUI
does itself for its success, warning and danger toasts, instead of leaving it in the plain foreground under an accent
title.

In dark, every floating surface — a dialog, a menu or submenu, a select's or combo box's list, a popover, a tooltip and
what is drawn as one (the touch and hint popovers, the rail's account quota panel), a toast, and a bar floating over the
terminal (the dormant session's and the exited shell's) — is drawn on the project's `--floating`, by the user's
decision, instead of HeroUI's `--overlay` and `--surface`, which are darker than the content panel and sink a menu into
the sidebar under it. The dark levels follow light's ratio for ratio, where every floating surface is white: the surface
is 1.11:1 over the panel and 1.02:1 over the sidebar (white: 1.11:1 over the light sidebar, 1.04:1 over the light
panel); the `--default` family on it is 1.24:1 resting and 1.39:1 hovered (light's control fill: 1.27:1 and 1.39:1);
`--border` and `--separator`, which HeroUI's outline button, tab list and menu dividers draw with, are 1.36:1 (light:
1.35:1); HeroUI's secondary `Surface`, the recessed frame of raw diagnostics, a confirmation's branch list, licence
texts and the file viewer's checkerboard, is 1.14:1 below it (light: 1.15:1). All floating surfaces share the one
colour, as in light, so one over another — a tooltip over a menu or a dialog, a confirmation over Settings, a status
mark's popover over the quota panel — is set apart by its edge: dark `--overlay-shadow` is a hairline and a drop shadow
instead of HeroUI's faint inset line. The rule is the floating-surface rule in `packages/ui/src/style.css`, which lists
the surfaces; a new kind of floating surface joins that list (a bar of the app's own takes its `floating-surface` class,
which in dark also gives it that edge). Light keeps HeroUI's own surfaces and shadows.

The step is held small by HeroUI's `--muted`, which keeps 4.5:1 on a fill holding secondary text (a placeholder, a menu
item's description under the pointer) only up to that resting fill. The user accepted the shortfalls the lighter ground
leaves in HeroUI's own colours: `--muted` on the hovered fill is 4.05:1; `--danger` is 3.33:1 for a destructive menu
item's label and icon at rest and for a field's error text, and 2.69:1 for the menu item under the pointer, where its
label still says what it does; a secondary button's label is 4.32:1 on its hovered fill and a soft danger button's
4.41:1 on its hover.

A tooltip or popover is portalled to the end of `<body>`, so it never inherits the `.control-fills` that the pane or
dialog around its trigger carries, and a control drawn from `--default` inside it, a secondary button or a ghost
button's hover, would sit at about 1.19:1 against its `--overlay` surface. The shared tooltip and popover components
(`IndicatorTooltip`, `AnchoredTooltip`, `TitledControl`, and the touch popover they open, which is drawn as a tooltip)
therefore carry `control-fills` themselves, giving 1.27:1 in light; in dark the floating-surface rule above sets the
fills itself (1.24:1). A new tooltip or popover that can hold a control drawn from `--default` carries `control-fills`
too. In light the project filter's popover keeps HeroUI's own fills: it holds a search field, and the remap would leave
the field's `--field-border` and `--field-focus`, computed from the root's `--default`, no longer matching its fill. In
dark it takes the floating fills like every floating surface: the secondary search field draws every state from the
`--default` family there, `--field-border` is transparent and `--field-focus` shows only on an invalid field.

A dialog's Advanced section opens from a small ghost button (`AdvancedSection` in
`packages/ui/src/dialogs/AdvancedSection.tsx`) whose label is in `--app-muted` at rest and the foreground on hover and
keyboard focus, instead of the ghost variant's `--default-foreground`, by the user's decision: its box lines up with
the fields above, so its label sits in from theirs, and in their colour it would read as a field label out of line.

## Keyboard

Every interactive control is reachable from the keyboard and operable with Enter / Space, except for the collection-row
behavior under "Buttons" below and a choice inside a dialog's form. A control that holds a choice there (a checkbox, a
radio, a switch, a closed select, a tab) takes Space, and the arrow keys where it has them, while Enter submits the
dialog from it, as Enter does from a text field. The platform does this for the native ones (a checkbox, a radio, a
switch), and the dialog does it for a closed select, a tab and a radio that is not a native input, so that having picked
an agent the user presses Enter to go ahead rather than to open the picker again. A button that acts (Cancel, a
disclosure) keeps Enter, and so does an open popover's list. So a dialog whose confirm button carries weight does not
also rely on Enter to settle a choice one of these controls holds.

A confirmation dialog opens with focus on its confirm button, so Enter confirms it at once. Two kinds do not, and keep
focus on the dialog itself, so an Enter pressed before the user has moved focus onto one of its controls does nothing: a
destructive one, and a question the user did not ask for (a folder trust prompt, a session request), which can come up
while they are typing elsewhere. The latter starts on the dialog again for each queued question it moves on to, and
starts on the dialog, not on its first control (its close button), also when it comes up over a dialog that a press
without a pointer opened (an assistive technology's press). A
destructive confirmation (removing, deleting, archiving) is a warning modal: the warning icon in its header, the
consequences as its body text (the sessions it ends or takes along listed in that body; a confirmation sectioned by
topic, as archiving, removing a project and deleting a console are, puts each section's consequences under that
section's options instead, each section an `h3`-headed labelled group), its destructive confirm button
the danger primary at the trailing edge; it is never an ordinary modal with a warning Alert inside it. A
confirmation opened by picking a choice in a select returns focus to that select's trigger however it closes (Cancel,
Escape or the confirm button), and the select shows the old choice until the confirmation succeeds. One that turns
destructive only once an option is ticked opens unticked, as an ordinary one, with focus on its confirm button. A
confirmation raised by another dialog's confirm button opens over that dialog and follows the same rule; Escape or
Cancel closes only it, leaving the dialog beneath open. One that asks for a word to be typed opens with focus in that
field. A dialog's form ignores the repeats of a held Enter or Space, except Space in a text field and Enter in a
multi-line one, so the press that opened it cannot go on to submit it.

A form dialog opens with focus in its first text field, so the user can type at once and Enter submits; one with no text
field opens with focus on its submit button instead. That holds however it was opened, by a press without a pointer (an
assistive technology's press) as much as by the keyboard or the mouse, and for a dialog opened over another one. A field that appears later, after a tab or a choice has changed
what the form holds, does not take focus, so it never pulls it off the control the user is on.

### Reaching a control with Tab

Tab is how a control is reached, except for a control inside the row of a react-aria collection that manages focus
itself (a `GridList`, a `Tree`). There the row is the tab stop and Tab moves past the whole list, while the arrow key
that runs along the row (`keyboardNavigationBehavior` defaults to `"arrow"`) moves between the row and its controls.
That is the grid and tree pattern and it meets this rule. The row actions of the change list and of the Files tree,
and the Files tree's folder icons, all rely on it, so do not give such a control a tab stop of its own.

WebKit's Option+Tab (and Option+Shift+Tab) reaches every control while macOS's Keyboard navigation setting is off,
which is the default, and react-aria leaves a collection in one step on Tab and Shift+Tab only, matching the modifiers
exactly. So a collection whose rows hold tabbable controls also takes `useCollectionOptionTab`
(`packages/ui/src/components/collectionOptionTab.ts`) on its element, which makes the same moves for the Option
variants; the Files tree and the change list do.

A picked tag's remove button (`Tag`) is out of the tab order (`excludeFromTabOrder`), so a tag group is one tab
stop and arrows move between its tags: react-aria's tag group would otherwise send Tab from a tag into its button. The
keyboard removes a tag with Delete or Backspace on the tag, which react-aria describes on it.

`preventFocusOnPress` only keeps a *mouse* press from taking focus; the control still has to be reachable.

A control revealed only on hover or focus has to be reachable by touch too. `RowControls`
(`packages/ui/src/components/RowControls.tsx`), which holds the hover-revealed actions of sidebar and Files rows, shows
them at all times on a device whose primary input cannot hover (`CANNOT_HOVER` in `packages/ui/src/touchInput.ts`,
written `[@media(hover:none)]:` in classes); the archive rows' Reopen and Delete and the avatar field's remove button
carry the same rule, and so does any new hover-revealed control.

**A table of fields is tabbed down its columns when a column is one unit.** In the console dialog's Advanced table each
column is one session kind's pair, and its reasoning effort depends on its model, so Tab runs down a column before
moving to the next, the same order the table takes when it stacks on a narrow window.

**Where a float decides the source order, every control of that row goes in the float.** A group that floats to the
end of a row has to come first in the source, because a float only shortens the lines that follow it; the text it
leaves beside it then reads after it. That is only a reading-order cost while the row's other content is inert text,
and it becomes a focus-order one — focus jumping back against the reading direction — the moment a control sits in
that text. So a control belonging to such a row joins the floating group rather than standing among the text: the
file viewer's detail row floats Copy path and the view controls to its end, rather than placing a button beside
the path it copies (`packages/ui/src/viewer/FileViewer.tsx`).

A control inside a collection's **section header** is reached by neither of those: react-aria's keyboard delegate walks
the collection's items alone, so the arrow keys never land on a header, Tab inside the collection jumps to the last
tabbable element and leaves, and focus arriving anywhere in the collection that is not a row is moved to its first row.
The change list's section heading — the band that folds the section away
(`packages/ui/src/browser/changeSectionHeading.tsx`) — and the mark a heading can carry are such controls, and they are
reached by this application extending the collection's own walk: Up and Down step through the rows **and the headings
between them**, Right from a heading moves to its mark and Left back to the band (mirrored in a right-to-left reading
direction, where the mark sits on the left), while Home and End still move between rows alone. Such a control takes no
tab stop of its own and must not take focus on a press (`preventFocusOnPress`), or a mouse press on it sends focus to
the list's first row; a popover opened from it hands focus back to its heading when it closes.

The band is react-aria's `rowheader` inside a `role="row"`, not an `<h1>`-`<h6>`, so the rule under "A visual heading
is a heading element" does not reach it: it is a row of the change list's treegrid, where that markup is what the
treegrid pattern asks for, and a heading element inside the collection would be a second structure over the same text.

A tab panel with nothing tabbable inside is a tab stop of its own, as the tabs pattern has it, so Tab from the tab
list reaches its content; one holding a tabbable control or scroll region is not, and Tab goes straight into it.

### Buttons

Copy explains; a button acts. Replace an instruction pointing the user elsewhere ("go to Settings → Agent accounts
and …") with a button that takes them there, opening the target dialog when there is one. One view offers one entry
for an action: remove a second entry that does the same thing, such as "Add agent…" beside "Manage accounts…" when
both lead to the same account management view.

**Button order follows the group's alignment.** A trailing-aligned group — a dialog's or a popover's footer, an
Alert's buttons — puts the secondary buttons first and the primary at the trailing edge (Cancel, then Confirm). A
start-aligned or centred group — an empty state's row, a status strip, a startup screen's row — puts the primary first
and the secondary after it. Whatever the alignment, a group has at most one primary button, and a centred row holds at
most two buttons: a third way out becomes a mark or goes.

Every button is HeroUI's `Button`. A native `<button>` is allowed only where a custom button is needed that HeroUI's
cannot be, with the reason in a comment there. A hand-built `role="button"` element needs `tabIndex={0}` and its own
Enter / Space handling.

Files and Git tree chevrons are also an exception to HeroUI's `Button`: they are React Aria's `Button` in
`slot="chevron"`, drawn as a bare glyph with no hover fill, press shrink or tooltip, keeping the collection's
expansion, focus and Tab exclusion (their target size is under "Pointer target size"). A whole row runs its action on
Enter and expands or folds with Left / Right (reversed under RTL); a pointer press on the chevron expands or folds it
too. React Aria 3.52.1's `useSelectableItem` maps Enter to the row's action (`onAction`, which includes the tree's
expansion) and Space to selection; selection is disabled in these trees, so Space on a row does nothing. Keep that
supported collection behavior rather than translating Space locally. The custom Git section-heading band retains its
documented primitive exception and acts on both Enter and Space.

A button that is unavailable for a reason the user needs to learn (the quick-create "+" while every account of its agent
is used up) is not natively disabled, which would take it out of the Tab order and keep its tooltip from ever opening:
it carries `aria-disabled`, keeps focus and its hover tooltip but takes no hover tint or press shrink, looks faded,
gives the reason as its tooltip and as an `aria-describedby` description while its name stays the action, and a press
explains instead of acting. HeroUI's disabled style turns off pointer events on `aria-disabled`, so such a button puts
them back (`pointer-events-auto`), or its tooltip never opens on hover.

A switch unavailable for a reason shown under it (the account's sharing switch while its sessions run) follows the
same rule: it keeps focus and is marked `aria-disabled`, a press or Space does nothing, and the reason is its
`aria-describedby` description. When the control that held focus goes away with the state it belonged to, focus moves
to the control the state was about, unless the user has already moved it.

A warning that a switch's last change did not finish does not block anything (turning the switch again is the retry), so
it is a warning mark right after the switch in its row (see "On the screen: an Alert when it matters, a mark at its
point otherwise"), and the switch is described by the warning's title, kept as visually hidden text beside it. The
mark's popover says what happened and how to try again, then any further explanation and the raw entries: a failure with
a single cause says that cause and its fix, one with several says that some files or folders could not be changed and
lists the causes, and a fix that points at the raw reason says it is below. The switch itself is the way to try again,
so there is no retry button. Only a failure that arrives after a change the view saw run is announced, its title spoken
through the view's `StatusAnnouncer`, unless this window asked for that change, when the error alert dialog it opens
speaks it instead (see "Where an error is shown"); the same mark found already there when the view opens, or after a
restart, is not. When the mark goes while it or its popover holds focus, focus moves to the switch.

Whether a disabled control says why depends on how long it stays disabled. One disabled by a state that can last (a
session in the middle of a turn, a locked session, no other agent to hand off to, a platform without clipboard reading,
an account that is disabled or not logged in, nothing to copy while the program in a terminal takes the mouse) carries a
reason: the user is left looking at it and needs to know what would make it available. (Without mouse reporting a plain
drag selects text, so Copy has nothing useful to add.) One disabled only for the moment an operation takes (a session
still starting, an account switch or a sharing change under way, a shell that is closing, a login under way) carries
none: it comes back on its own before a reason would be read. Nor does one whose cause the screen already shows where it
happens (a terminal that cannot take input, covered by its reconnecting spinner or its disconnected notice). A form
still states a brief hold when it affects the value already chosen, as that field's error or as an alert.

A disabled select option or menu item, with a reason or without, stays in the arrow-key walk (`disabledBehavior:
"selection"` on the select, and on the menu and any single-selection section inside it), so a keyboard user reaches it
and, where it has one, hears why; it still cannot be activated or selected. Pointing at its clipped name still shows
the full name, and pressing it runs nothing. Under that setting react-aria sets no
`aria-disabled` on a select option, so the option gets it by hand.

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
holds controls — a pane, a panel, an overlay — is marked `data-region` and added to that hook's region order and
`shown`, or F6 skips it and a keyboard user in the terminal has no way to reach it. A right pane panel is the exception:
it reaches the hook through its `panels` argument (region `panel-<kind>`, in stacking order), and a kind that belongs to
the selected session rather than to the pane's owner (a project's Report panel) is a stop only while that session has a
panel to show. While the Terminal panel is maximized it comes first among the panels, and the agent area it covers (the
agent terminal or the archive view, made `inert`) is not a stop. A panel floating over the agent terminal in a window
too narrow to dock the right pane covers it the same way: the agent terminal is `inert` and not a stop while the panel
is shown, focus moves into the panel when it is opened and back to the agent terminal when it closes (after a tap, never
into a text field; see "Focus moved by a change the user did not make"), Escape closes it (standing aside inside a shell
terminal, as below), and the dimmed strip beside it is a "Close panel" button. A region lands F6 on its first control
unless a control in it carries `data-region-landing` (a Files panel that failed to load lands on Try again, not on its
header's Close panel). `shown` follows whether a region holds a control, not whether its element is in the document: the
terminal counts while a session is selected, and also while its empty state offers a creation action for the current
context or the Add agent button (agents are installed but none is added), since that button is then the one thing to do
there; with only its empty line (nothing installed) it is skipped. An archived session's terminal is covered by its
history and made `inert`, so the region lands on the history instead: its message list, or while there is none, its
first control below its header (the header's link to the session's owner is never the landing), and failing that the
floating bar's Reopen. A control in the pane that goes away while it holds focus (the owner link once the owner is
shown, Reconnect, a creation action) hands focus on as focus meant for the agent terminal, so a panel its press brings
up over the agent area takes it rather than the inert terminal. A region whose focus goes through a method of its own
rather than its first control (the terminal's) is only a stop when that method actually leaves focus inside it, which is
how the cycle tells the two apart.
Below the `phone` breakpoint the rail is out of the row and comes in only inside the sidebar's drawer card: while the
drawer is closed the rail is `inert` and not a stop, and focus inside it when the drawer closes is handed on before the
browser can drop it to `<body>` (a layout effect, since a swipe closes the drawer from a touch listener React does not
flush before paint).

A shell terminal in the Terminal panel keeps Tab the same way, so the same holds for it: the Terminal panel is its own
region, and F6 lands on the shown terminal's input, on its cover's Reconnect while that is offered, or on New terminal
when the panel has no tab. Unlike the agent terminal, a shell terminal also keeps Escape, in a floating pane too
(vim, less and fzf need it): the floating-pane Escape handling in `packages/ui/src/layout/usePaneToggles.ts` stands
aside for a target inside a shell terminal, and the pane is dismissed with Close panel or by moving the pointer away.
A control in a shell terminal's view that xterm's Tab handling puts out of reach (the exited notice's Close terminal)
needs an equivalent reachable another way; Close terminal in the panel's tab-row menu and the tab's context menu is it.

### A floating panel that does not take the window

A panel that floats in from a button and leaves the rest of the window usable (the rail's account quota panel, and the
popover of an error or warning mark) is a large tooltip with a body: one component (`Hint`, kind `popover`) gives both
the same input logic. It is not a region: it is reached through its button, the way a menu is. Its dialog is named by
what the panel is (Quota, the mark's name, a tone's word for a mark of one line), never by the text it holds: VoiceOver
reads both a dialog's name and its content as focus enters it, so a dialog named by its own text is read twice.

**The input matrix**, by what the input can do:

| Input | Result |
| --- | --- |
| Mouse resting on the trigger (a mouse pointer only; touch and pen never hover) | Opens it after a short delay **without moving focus**: the hover form. It stays open while the pointer is on the trigger or inside it and closes a short grace after the pointer has left both |
| Mouse click | **Opens nothing and pins nothing**: where a pointer can hover, the surface is the hover's. A click on a hover-opened surface leaves it as it is, and a click on a closed trigger does not cancel a pending hover-open. Its buttons and text are reached by moving the pointer into it |
| Touch or pen tap, assistive technology's activation (virtual press) | Opens it with focus moved in: the press form |
| Keyboard Enter or Space | Opens it with focus moved in: the press form |

No press takes focus on its own (`preventFocusOnPress`): a click does nothing, and a tap must not pull focus off the
terminal or an editor before the surface decides where focus goes.

- **The hover form** is `role="dialog"` with `aria-modal="false"`. The window's check for an open modal matches every
  dialog without it, and would hold F6, window shortcuts, Back / Forward and floating panes while the panel is merely
  open. It is not a dialog that focuses itself on mount. It is focusable without being a tab stop (`tabIndex={-1}`),
  so a mouse press on its text leaves focus in it: otherwise focus would fall to the page, and a focus-containing
  dialog it floats over (react-aria's `FocusScope contain`) would take focus back a frame later, ending the drag that
  selects the text.
- **The press form** is a modal popover: focus is inside, an outside press or Escape closes it, focus goes back to what
  held it, and the window's shortcuts and F6 wait as for a menu. A press of this kind on a hover-opened surface turns it
  into the press form, remounted so that it behaves as if press-opened from scratch (focus returns to the trigger).
- **Escape.** While hover-opened the surface marks itself `data-escape-topmost`, so the drawer and floating-pane Escape
  handling stands aside while it exists, and its Escape and outside press are watched on the document, since
  react-aria's non-modal popover does neither. Opened by hover it holds no focus, so the key belongs to where focus is:
  Escape is left alone when focus is in a terminal (the agent's, whose program takes Escape as an interrupt, or a
  shell's) or a text field holding text, and while an input method is composing. A press-opened surface took focus, so
  it takes Escape (react-aria's own).
- **Closing.** After a close that is not the grace's own (Escape, an outside press, an action such as Try again) a
  pointer still resting on the trigger does not open it again: the hover is armed again once the pointer has moved off
  the trigger and the surface and comes back.
- **A surface that holds another.** The rail's panel holds status marks, whose popovers are portalled out of it but
  belong to it: an outside press inside such a popover is not outside, the panel stays open while the pointer crosses
  into it (React raises pointer enter and leave along the component tree, portals included), and Escape goes to the
  popover first where the popover would take it (focus in it, or its hover form with focus somewhere that does not keep
  Escape); otherwise the panel's own rule applies.

The rail's quota panel adds that its button has no tooltip: hovering it opens the panel, which shows everything a
tooltip would, so the button carries only its accessible name (the selected session's remaining quota, every window it
draws included). A modal opened by something other than a press on the entry (Settings from the native menu)
closes the panel in either form, rather than leaving it up under the modal.

### A touch gesture is never the only way

The phone-width swipes (`packages/ui/src/layout/useSwipe.ts`) only add a path: every pane a swipe opens, closes or
switches is also reached with the top bar's toggles, by pointer and keyboard alike. A swipe goes through the same open
and close paths as those toggles, so it moves focus as they do: a panel it opens takes focus on its tab, a pane it
closes hands focus on as any close does, and opening the drawer leaves focus where it was. It stands aside for what owns
a horizontal drag of its own — a scroller that can still scroll the way the finger moves, a separator or a slider — and
for an open dialog, menu or popup.

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
Derive a name hint using the submission's host-home, lexical path and repository naming rules; keep the entered
value and raw submission unchanged.

## Visible focus

HeroUI controls draw their own focus ring.

A scrollable tab list leaves enough inner padding for the full focus ring and its offset on every side.
`Tabs.ListContainer` clips its child list through a scroll container, so this clearance belongs to `Tabs.List` itself;
padding outside the scroller does not protect the ring. Each tab also needs matching horizontal scroll margin so
keyboard navigation reveals the ring as well as the tab border. Suppress the tab scroller’s fade mask while a tab has
visible keyboard focus, so it cannot fade out the ring.

An invalid, focused HeroUI `Input` keeps its outline offset at zero so the error outline meets the focus ring. This
overrides WebKit's native inset without changing the component's colours or ring.

### A hand-built focusable element

An element that removes the outline puts a ring back, never `outline-none` alone, and takes it from
`useFocusVisibleProps` (`packages/ui/src/components/useFocusVisibleProps.ts`): spread its props on the element and
style with `data-focus-visible:ring-2 data-focus-visible:ring-focus` (`group-data-focus-visible:` from a child).
The application's custom ring uses HeroUI's `--focus`, the same token HeroUI controls use.

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

A hand-off that runs because what held focus went away — a pane hidden, a panel closed, a region unmounted by a daemon
event — also calls `setInteractionModality("keyboard")` before it focuses the destination. The user did not ask for the
move, so they have to be able to see where focus went, and the previous input was often a pointer press, which leaves
the destination without a ring. `handFocusOff` takes a `showRing` flag for the hand-off whose trigger is a press, where
the ring follows how the press came; `HandOffOnUnmount` always sets it, its trigger being a render, and so does
`useFocusHandoff`, whose trigger is the render that took the control away. That covers the one hand-off through it whose
trigger is really a press — the change filter field closed from the header's toggle, which takes no focus itself —
knowingly: focus lands on a control the user did not aim it at either way.

Selecting another archived session while focus is in the history on screen is not such a move: a sidebar row's press
keeps focus where it is, so the history it replaces drops focus to `<body>`, but the user made the switch, whether
from a row, Back and Forward, the rail or the owner link of an archived owner. `TerminalPane` hands focus on, ahead of
`useFocusHandoff`, the way focus meant for the agent terminal goes (the new history, a covering panel, or the top bar
after a tap) without setting the modality, so the ring follows the input that selected: none after a pointer press,
shown after a key. A switch from the history to a live session, or to none, still goes through `useFocusHandoff` with
the ring on.

After a tap (`interactionIsTouch` in `packages/ui/src/touchInput.ts`: the last press was a finger; a mouse, a trackpad
or any key counts as not, so an iPad with a keyboard keeps the desktop rules) two things change. An automatic move does
not turn the ring on — every such move asks `ringForAutomaticFocus`, F6 being the one exception, as a key — since there
is no keyboard to follow it with, and a ring left on a top-bar control would also open its tooltip and leave it
standing. And the window does not move focus into a terminal on its own initiative, since a terminal's text field
raises the on-screen keyboard over the content the user tapped to see: a floating panel lands on its first control that
takes no text (`focusLandingWithoutText`; the Report panel lands on its page, marked `data-touch-landing`), focus meant
for the agent terminal goes to a covering panel's first such control or else the top bar's first control, so it never
falls to `<body>`, and a tapped session pick does not focus the terminal. A tap on a text field still focuses it, and
form dialogs still focus their first field, since the user opened them to fill in.

A separator of collapsed lines taken off the screen is such a move: the bar for the run past a diff's last hunk goes
once the daemon's reply says that hunk reaches the end of the file, and the hand-off to the code frame sets the
modality itself. `handFocusOff` cannot be used there, since it looks at `document.activeElement` and cannot see into a
shadow root. The redraw after an expansion that takes the pressed control away, with no separator control left to take
focus, hands focus to the code frame the same way (`handFocusToCode` in `packages/ui/src/viewer/rendererDom.ts`), so
focus never falls to the page.

A focus restore after a press decides by whether the pressed control had focus, not by whether the press was a key:
VoiceOver presses with a click (an accessibility press), so a restore keyed to Enter or Space misses the screen reader's
press and leaves focus on the page when the control goes. The instance is `Expansion.activate` in
`packages/ui/src/viewer/expansion.ts`, which is told whether the control used was the focused one; a mouse press on a
control that did not have focus still takes none.

A dialog opened from a control that never took focus, is disabled, or is gone by the time it closes cannot always hand
focus back to it: React Aria restores focus to what held it when the dialog opened, which is not the control after an
accessibility activation or a `preventFocusOnPress` press, and finds nothing that takes it when the control is disabled
or gone, so focus ends on `<body>`, where VoiceOver reads the whole web area instead of the control the user was on.
Three hand-offs repair it. The login hand-off (`useLoginFocusReturn` in
`packages/ui/src/settings/AccountLoginButton.tsx`): the row's own Log in button is shown only while the account reads as
signed out and is not turned off, and is disabled while a login runs, so after a success focus goes to the row's
actions, and after a cancel to the Log in button once it is enabled again, else to the actions. The console dialog's
Settings (`packages/ui/src/dialogs/ConsoleDialog.tsx`) is handed the exact opener that opened it, so closing Settings by
its backdrop returns focus there even when Manage accounts was activated without taking focus. A Shell terminal closed
through its busy confirmation is repaired once the confirmation goes (`focusAfterConfirmedClose` in
`packages/ui/src/terminal/ShellTerminalPanel.tsx`): when the terminal's tab or view that the dialog gave focus back to
has gone with it, focus goes to the shown tab after a close asked from a menu, or to the shown terminal after one asked
from the terminal itself; the ring is turned on only as it moves focus, so a close the daemon refuses leaves focus where
the dialog put it back, ringed or not as the press was. All three wait at least a frame past React Aria's restore and
leave a restore that landed right alone; they act where focus is on `<body>`, and the login hand-off also where it is on
its going button.

A dialog that the Agent accounts section opens by itself as Settings opens (the account settings dialog for View in
Settings, a login found running, Add agent) saves its restore target before Settings' tabs exist, so on closing it hands
focus on itself rather than leave it to React Aria: to the section's tab, or for Add agent to the strip's Add agent
button, else the empty panel's own. A row's account dialog goes back to the row's actions button, else the section's
tab. These run ahead of React Aria's restore, wait until the target can take focus (the dialog has gone and nothing
covers Settings), ring it as focus the user did not aim, and take it back once if a give-back lands after them
(`handFocusOnceClosed` in `packages/ui/src/settings/AgentAccountsSection.tsx`). Settings itself, closing with no dialog
under it, goes to the selected session's terminal, else to the control that opened it while that is still there, even
one that never took focus (the press it ended on is recorded by `usePressedControl` in `packages/ui/src/components/`),
else to the window's fallback (`focusAfterModalCloses` in `packages/ui/src/components/regionFocus.ts`). Closing over
another dialog, focus the restore left on `<body>` goes to the control in that dialog that opened Settings, else the
control that dialog opens on. A return to the
control that opened a dialog is a restore, not a hand-off: its ring follows the input that closed the dialog.

A dialog opened by a press inside the sidebar's drawer is a third such case: the drawer is not modal and a tap does not
move focus, so React Aria would restore focus to whatever held it behind the drawer. The window remembers the drawer
control the press ended on (a menu's trigger, for a dialog opened from its menu) and focuses it as the dialog opens, so
the restore lands back on it. Below the `phone` breakpoint the rail is part of the drawer's card, so a dialog opened
from the rail there (Settings included, by press or by key) is the same case and returns to the rail's control.

A dialog's submit or confirm button that becomes disabled while it holds focus (a quota reading landing under an
open dialog) hands focus to Cancel, also when WebKit has already blurred the button to `<body>`; a dialog that opens
with its submit refused puts its autofocus on Cancel instead of the disabled button.

### Focus scrolled to the edge of a scroller that fades there

A scroller whose edges are faded gives the scroller a `scroll-padding-block` of at least the fade's own size. A browser
scrolls a control only far enough to reach the container's edge, which is exactly where the fade sits, so the control
the user was sent to arrives underneath it (the first invalid field a submit sends focus to, for one). The padding is on
the scroller rather than on what it holds, so nothing put inside has to know about it. Every scroller that takes the
fade gets it, a dialog's body and a list in a popover alike, and neither shape of the project's own capability leaves it
to the site: the padding comes with the fade. A fade the library draws for its own component is the one exception, and
not one to copy — a HeroUI `Tabs.ListContainer` uses its own 64 px fade. Suppress that mask while a tab has visible
keyboard focus, as specified under "Visible focus", so the revealed tab and its full ring stay visible.

Where a sticky heading occupies the top of a faded list, the top scroll padding includes both the heading's height
and the fade's size, so a focused row is visible below them. The heading itself stays clear of the fade.

The same holds sideways for a table with a pinned column: the Settings accounts table pins its Actions column to the
end of a horizontal scroller, so the scroller's end scroll padding is the fade's size plus the pinned column's measured
width (`FadedScroller`'s `endInset`), or a focused switch would scroll to just under the pinned column. That scroller
draws no fade mask (`fade={false}`), since a mask would dim the pinned column itself.

A control inside such a heading sits within that padding while it is pinned, so focusing it would scroll the list by the
padding's depth on every focus move (the Projects heading's Funnel X and Collapse all are such controls). The heading's
controls are exempted: their scroll area is moved down by the padding's depth (`scroll-margin-block`), which Chromium
honours, and since WebKit's own focus scroll ignores scroll margins and runs after the focus event, the list's position
is held for a moment from that event while focus stays in the heading and the control was already in view. A heading not
yet in view is still revealed.

### A scroller nothing else on screen scrolls takes focus itself

A faded edge says there is more, so the keyboard has to be able to reach it. Where nothing else on the screen would
scroll *this* scroller, it becomes a focusable `region` with an accessible name, and the arrow keys and Page Down then
scroll it. The test is that scroller, not what it contains: a stop inside it carries focus only as far as itself, so
everything past that stop is still content nothing reaches. A dialog whose controls are all in its footer is the case
`Dialog` owns, named by the dialog's own title; the screen shown before there is anything to show is another, named from
the catalog, where the buttons sit after the message; the crash screen's stack trace, on the details surface under its
lines, is a scroller in its own right, as is the daemon's message in the daemon-failed screen's, each named "Error
details" as any diagnostic block is; a pane's whole-subject failure taller than the pane is another, named by its
heading.

Such a region appears only while the scroller overflows, since a tab stop that scrolls nothing is one the user passes
for no reason — the crash screen's stack trace and the screen's own column both come and go that way. The exception
is a region that is for more than scrolling: the file viewer's reading surfaces are tab stops whether or not they
scroll, because taking focus is also what their Select All and their copy handler hang off. So a region that does
something of its own stays; one that only scrolls comes and goes with the overflow.

### A keyboard handler that moves focus itself

Such a handler calls react-aria's `setInteractionModality("keyboard")` first when the handler sits on the window's
capture phase and stops propagation. react-aria tracks modality from a capture-phase listener on the document, which
the key then never reaches; without the call, what the handler focuses draws no ring when the previous input was a
pointer. A handler for a chord with ⌘ or Ctrl (or Alt outside macOS) calls it too, whether or not it stops the key:
react-aria does not count such a chord as a keyboard interaction. It calls it only once it knows the shortcut will act, so
a shortcut with nothing to do does not turn a ring on where focus already is.

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
- **A control in a table of fields is named by its column and row headers.** The headers stay visible but are hidden
  from assistive technology, and each control is named with `aria-labelledby` over the two header ids, column first;
  the control then carries no `aria-label` of its own, which react-aria would add to the name and read twice. The
  console dialog's Advanced table is the instance: "Console sessions Model" over the session-kind column and the
  setting row (`ConsoleLaunchFields`, with `OptionSelect`'s `hideLabel` and `labelledBy`).
- **A name drawn as several boxes in a flex row is built as one string of its own**, with the drawn boxes hidden from
  the name (`aria-hidden="true"`): WebKit puts a space around every flex item while composing a name, so the boxes
  would be announced as "a.ts : Staged , Modified". The file viewer's title is the instance — an `sr-only` span holds
  the whole name, assembled from the catalog in the order the boxes are drawn in, and carries `select-none` so a copied
  heading does not hold the text twice (`packages/ui/src/viewer/FileViewer.tsx`).
- **A field whose visible label shows less than its name** keeps both in its `<label>`: the visible text
  `aria-hidden="true"`, then an `sr-only select-none` span with the whole name, which contains the visible text
  verbatim. It never adds an `aria-label` beside the label: react-aria names the input by its label element and joins
  the two, so "Directory for Notes Directory" would be read. The import review's Another directory field is the
  instance — visible "Directory", named "Directory for {project}", so each row's field stays distinct (`TextInput`'s
  `labelContent`).
- An element that takes an `aria-label` without being a control — a scrolling `div` or `pre` made focusable with
  `tabIndex={0}` — also gets a role that can be named (`role="region"`). ARIA prohibits naming a generic element, so
  the name may never be spoken. A diagram and a block formula in a Markdown document are such regions
  (`MermaidBlock`, `FormulaView` in `packages/ui/src/viewer/`). The diagram's name is its own title, or the generic
  diagram name, and an inner `role="img"` carries that same name with a text alternative. A formula is not an image:
  its MathML is what is read, and naming the region does not replace that. An inline formula is a region, and a tab
  stop, only while it is wider than the line. While it fits, it is not a tab stop.

### A modal's title is a plain name

Every modal's title — a form dialog, a confirmation, a warning modal, the error alert dialog, a failure modal — is a
short plain phrase naming what the modal does or what failed ("Switch account", "Delete session", "Cannot open a
session"), the same words whatever it is opened on. It holds no variable content: no session title, account, project
or branch name, no path, count or quoted value, and no reference chip (`InlineReference`). What the modal is about goes
in its body, its first sentence naming it there ("Fix search will be switched to Work."), with everything else that
follows. The title is the modal's accessible name, read whole each time it opens and each time focus comes back into
it; a long or variable one wraps into a block that no longer reads as a heading, and puts the name of a subject ahead
of what is being asked about it.

The rule is for these general modals — forms, confirmations, warnings, errors and failures. A dedicated component
that shows one piece of content in a modal layer, such as the file viewer, is not one of them: its title is the name of
the content it shows (the file's name and its status), which is what the viewer is, and it keeps it.

### An icon-only control: tooltip and `aria-label`

Every control that shows only an icon — a HeroUI `Button` with `isIconOnly`, a `CloseButton` or `Modal.CloseTrigger`, a
menu trigger — carries both a tooltip and an accessible name. The name is what assistive technology announces, and it
has to tell the control apart from its neighbours even where the tooltip's text cannot, so the two are not always the
same text. Which control's tooltip repeats its name and which one's is the shorter of the two is settled per control in
the "Tooltips on icon-only controls" section of `docs/product/labels-and-tooltips.md`; `ActionMenu` gives its trigger a
name of that shape by default. The exception is a button whose hover opens a floating panel that shows what
its tooltip would say — the rail's Account usage button (above), a status mark, the help tip's "?": it carries only
its accessible name. So does a Files or Git tree chevron, whose whole row does what it does (see "Buttons").

A tap does not open that tooltip, and on such a control it must not: a tap on an icon-only button or a menu trigger runs
its action, so `TitledControl` has no touch popover. A control whose only action is to show its text is a popover hint
instead (the help tip's "?").

A copy button (`CopyButton`) takes its accessible name from its tooltip, which changes to "Copied" once the text is on
the clipboard. A control that keeps focus is not reliably re-read when its name changes, so the button also announces
the copied text through a `role="status"` region beside it. Its tooltip stays open through the press (`keepOpenOnPress`
on `TitledControl`), and the icon changes, both ways, are played with `motion-safe:` only, so under reduced motion the
icon simply appears. It is drawn where the platform has no clipboard too, copying through a hidden text it selects
(`copyText`), which never moves focus: a focus change would blur the button and close its tooltip.

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
anchored shape (`AnchoredTooltip`, which a run of running text takes, a mark in a row that walks its focusable
descendants with the arrow keys, and a tab that carries a mark inside it, since none of them can be a trigger of its
own) is not associated with what it describes for assistive technology at all; the tab, being a control, opens it on
keyboard focus as well. So whatever a tooltip says must reach a screen reader by another route: a clipped
label's full text is in the document whether it fits or not, a row or a tab carries its own accessible name, and an
indicator that means something on its own is a named `role="img"`.

**One component, one look.** Every floating hint of the application is `Hint` (`components/Hint.tsx`), whatever it
carries: a plain tooltip, the touch popover, a status mark's popover with or without a body, the rail's quota panel.
`kind` says what carries the trigger — `control` (a react-aria control as the child), `indicator` (a non-control mark),
`anchored` (the caller's hover) or `popover` (the caller draws the trigger from the wiring `Hint` hands it) — and all
four are drawn with one definition, HeroUI's `tooltip` class plus `control-fills` (`TOOLTIP_SURFACE`): the same surface,
shadow, radius and offset, and for text hints (a tooltip, a mark's popover) its padding and text size too (measured 8
px, 12 px, 12 px radius). Width is what the content needs, up to the class's 320 px cap. The quota panel is a panel, not
a hint: it shares the surface, shadow and radius, and keeps its own width, edge-to-edge padding and text sizes. The
element underneath is chosen by whether the content is interactive, which the caller says with `kind` and which is never
sniffed from the content: the first three are tooltips (`role="tooltip"`, react-aria's `Tooltip`, described to the
trigger, opened by hover and keyboard focus, nothing on a mouse click), and `popover` is a dialog popover, because a
tooltip can hold neither focus nor a button, so content with a Try again or raw text to select cannot be one. It follows
the input matrix of "A floating panel that does not take the window". `TitledControl`, `IndicatorTooltip` and
`AnchoredTooltip` are `Hint` of the first three kinds under shorter names; "one wrapper" below is `Hint`'s three tooltip
kinds.

**One wrapper, and the input chooses the surface.** Every tooltip in the window goes through `Hint` (none through
HeroUI's `Tooltip` directly), in one of three kinds: the control kind (`TitledControl`, a react-aria control as the
child), the indicator kind (`IndicatorTooltip`, a non-control indicator) and the anchored kind (`AnchoredTooltip`, the
caller's hover). What shows follows the input of the interaction, not the device, so a touch-screen laptop or a tablet
with a trackpad gets what the input it used answers:

- a mouse hover and keyboard focus open the tooltip, as always; a mouse press opens nothing extra;
- a touch or pen press opens the same content in a **touch popover**, where the shape allows it. The popover is drawn
  like the tooltip, from the one definition: it carries HeroUI's own `tooltip` class (surface, padding, text size, width
  cap, word breaking) and the same `control-fills`, rather than a copy of them. It is modal, as a popover is: an outside
  tap or Escape closes it, and focus, which it takes, goes back to what held it. Being modal it hides its trigger from
  assistive technology while open, so it is not linked to the trigger as a description, as the tooltip is; its dialog is
  named by the trigger's name, or by the generic word Hint where the trigger has none or its name is the tooltip's own
  text: a dialog is never named by the text it holds, which VoiceOver would read twice as it takes focus. The tooltip is
  closed and held back while it is open, as a pen can hover before it presses and an open tooltip would swallow Escape
  ahead of the popover.

Whether a shape opens it is a parameter, not a guess. Recognising from the DOM or from props whether a trigger "is a
button" is unreliable: a react-aria control hides its press handling in its own hooks, a mark's own trigger is a
`role="button"` element, and a row's action lives on an ancestor. So the default follows the shape's trigger, which the
wrapper knows statically:

- the control shape has none, since a tap on a control is its action; a control whose only action is to show its text
  is a popover hint instead;
- the mark shape's `touchPopover` is **on**, since a tap on a mark does nothing else; it is turned off, with the reason
  in a comment, for a mark inside something that handles the same tap (a tree row, a button, an option), or the popover
  would open over what the press does;
- the anchored shape has no such parameter and never opens one: it has no trigger to tap, and a tap on its row is the
  row's action.

The mark shape is also the one trigger that fits a control whose hover is sent to a hidden input, so it takes no tooltip
trigger from context (focus mode's include-console-sessions switch). That use turns the popover off, as a tap on the
switch toggles it, and the shape's mouse-press cancel does not stop the switch: react-aria's press takes its own focus
and toggles on the click.

A mark that opens a touch popover is a touch target (see "Pointer target size").

One anchored tooltip is kept without that route: the real target of an OSC 8 hyperlink in a session terminal, whose
visible text may differ from where it leads. Nothing else carries the target, but the terminal's output as a whole
reaches no assistive technology (xterm.js keeps its rows `aria-hidden` and runs without its screen-reader mode), so the
tooltip withholds nothing the text around it would otherwise give.

**A value tooltip keeps its label's font family.** A tooltip that is a value in full rather than a sentence — a clipped
label's whole text, a field's expanded path, a terminal link's URI — shows in the font family of the element that draws
the value: a session ID, a path or a terminal's text in monospace keeps monospace in its tooltip, and a name in the UI
font stays in it. Only the family is carried over; the tooltip keeps its own size and weight. The tooltip is portalled
out of the label and inherits nothing from it, so the value is drawn with `ValueTooltipText`, which reads the computed
font family off the element its required `label` names; no call site states a font. `FadeOverflow` draws every
plain-string tooltip that way, the string being the label's own text in full, and a sentence goes in as a node, which
keeps the tooltip's own font. Where the element under the pointer does not itself draw the value (the box laid over a
terminal link), that box carries the value's font for the tooltip to read.

A tooltip of any shape closes once what it belongs to turns inert: a dialog or a modal popover opening over it, or a
region the window makes inert itself. An overlay that covers the control under a resting pointer sends no
`pointerleave`, so without this the tooltip would stay drawn over the overlay, naming a control that can no longer be
reached, until the pointer next moved. A tooltip of a control inside the overlay is unaffected. The surface every
tooltip shape draws on does this (`hintTooltips.tsx` in `packages/ui/src/components/`), so a new tooltip needs nothing
of its own.

Escape dismisses a shown tooltip without the pointer or focus moving (WCAG 2.2 SC 1.4.13). Inside a popup that holds
focus — a menu, a submenu, a select's list — that Escape closes the tooltip alone, leaving the popup open with focus
where it was, and the next Escape closes the popup as usual. A tooltip with a trigger gets this from react-aria, which
takes the key while it is open; the anchored shape, which has none, takes it itself, but only while it is shown and
only when focus is inside the react-aria popover its anchor sits in, so a tooltip still waiting out its delay leaves
the key to the popup. Outside such a popup the key goes on: a dialog closes with whatever tooltip is open in it (see
`useEscapeWhileTooltipOpen` in `packages/ui/src/dialogs/`), and the agent's terminal still takes it as an interrupt.
A combo box keeps focus in its input, outside its list, so an anchored tooltip in the list would close with it; none
of its options carries one.

### A help tip: an explanation behind a "?"

An explanation moved off the screen into a "?" uses `HelpTip` (`packages/ui/src/components/HelpTip.tsx`), never a
tooltip on a bare glyph: it is the one control whose only job is what it says, so it has to be reachable every way the
text it replaced was.

- It is a tab stop with an accessible name of its own ("About the default account"), not the explanation itself, so
  the name stays short; the same name names the popover the explanation is shown in.
- The explanation is in a popover, `Hint` of kind `popover`, as a status mark's content is, not in a tooltip: it can
  run to several lines, and a mouse has to be able to select and copy it, which a tooltip, closing the instant the
  pointer leaves its trigger, never allows. The popover follows the input matrix of "A floating panel that does not
  take the window", with focus returned to the "?" when it closes.
- It sits beside the label it explains, never inside it: a button nested in a `<label>` is nested interactive
  content, and its name would become part of the label's.
- The explanation may carry inline references (a file name such as `CLAUDE.md`); it reaches assistive technology the
  same way as plain text.

What it does not give: the explanation is not part of the surrounding row's accessible name, so a screen reader hears
it only on reaching the "?". That is why only a principle-level explanation, which operating the control does not
need, or the extra line one item in a set of like items carries, moves there; every other description stays visible
text, a status among them; a warning, an error or a note takes its mark or Alert (see "On the screen: an Alert when it
matters, a mark at its point otherwise"), never a "?".

The permission-approval explanation in Settings' Agent accounts section is an exception: the explanation of how
requests are handled, when the choice takes effect and Codex's possible extra token cost sits in a `HelpTip` beside
the Permission approval label of the agent's own setting row, above its accounts table; the row's visible description
says only that the choice applies to every account of the agent.

So is the Claude compatibility row in the same tabs (Codex and Grok Build): what the agent's sessions load and follow
when it is on or off, how Octoboard brings it about for that agent, and when a change takes effect sit in a `HelpTip`
beside the row's label; the visible description says only that the switch applies to every account of the agent. That
line states the scope, not the switch's state, so it stays the same whichever way the switch is set.

So is the share-default-config explanation in an owned account's settings dialog: what sharing links, and that idle
sessions are ended and resumed by a change, sit in a `HelpTip` beside the row's visible label, under which a one-line
description stays visible. The reason the switch is locked is a status, so it stays visible text under the row while
the lock lasts: the reason a control is unavailable is its state, not a warning or a note.

Another deliberate exception: an added agent that is not installed is marked in the Settings agent strip by a warning
glyph after its tab's name, and the explanation sits in a tooltip anchored to the tab rather than in a status mark's
popover or a line of text. The status itself stays visible as the glyph; the tooltip opens on hover and on the tab's
keyboard focus, in the latter case once any scroll that focus set off to reveal the tab has settled, since a scroll
under an open tooltip closes it; and since nothing links it to the tab, the tab's accessible name carries "not
installed" and each owned account's visible "not usable" line carries the rest.

The import dialog's region tabs follow the same pattern: a region whose panel needs attention (a project not found, an
account that cannot be created as it is) has a warning glyph after its tab's name, the tab's accessible name ends with
its needing attention, and a tooltip anchored beside the tab says so, opening on hover and on the tab's keyboard focus
(by input modality, so a mouse press that focuses the tab opens nothing). That tab strip does not scroll, so the
tooltip does not wait for a reveal scroll to settle. The marks inside the panel carry the specifics.

### Hover marquees belong to one text label

A clipped name, heading or detail scrolls only while the pointer is over its own text region. Independent labels
within a row, card or header have independent hover regions; pointing at neighbouring text, an indicator, padding
or an action button must not start their marquees. A branch and its upstream counts may scroll together when they
form one text line, as may a changed file's name and the folder or rename detail after it.

Each run scrolls at reading speed, holds at the far edge for three seconds, then returns to its resting position in
about 200 milliseconds. The first reading pass starts after a 600 millisecond hover delay; subsequent passes wait three
seconds at rest. Leaving the hover region resets the label to rest immediately.

An entire menu item, selector, fixed-owner indicator, picked tag or diff's collapsed-line text control may trigger
its single text label as one interaction target. Such a scope must contain only that label; a picked tag's remove
button may share the scope so its text can scroll clear of the button. A row or header containing independent labels
or actions is not a shared marquee target. A disabled menu item's row ignores the pointer, so its name runs only while
the pointer is on the name itself.

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

The agent icon is such a mark wherever the text beside it does not name the agent. On a sidebar session row, a focus
mode's archived row, a focus mode's card and an archive view's row its tooltip names the agent, then lists the account,
the model and the effort the session runs with, each under its label: on the two former it is the only route on screen
to all four, and on the card and the archive row, whose line beside the icon writes out the account, model and effort,
the only one to the agent. The list is the same in every placement: a meaning the window has settled does not come and
go placement by placement. The agent and the account are also in the session row's accessible name, or in visually
hidden text where there is no row name, and the accessible name of each of these four ends with its model and effort
as well, since the row is one button and nothing inside it is announced.

One gap is left open on purpose: the agent icon beside each session in the archive submenu carries no tooltip, so it
says which agent it is to nobody, its `svg` being `aria-hidden` as well. The agent is read from the archive view's own
rows instead, where that icon carries its tooltip and the agent is in the row's accessible name. The relative time at
the end of those archive items is the one indicator on a menu's transient surface with a tooltip of its own (the
session's archive and creation dates, `SessionRelativeTime`); a clipped label in a menu item, whether or not the item
can be chosen, carries its own through the anchored shape, as in the Switch account submenu. The time keeps its
`role="img"` name, which the menu item's own name takes in after the title, so the dates are still read there.

**A tooltip is worded from the string that already names the mark**, not from one of its own: the same string where it
names the mark on its own, so the two cannot drift apart (a session's agent mark shows that string's two parts, the
agent and its account, as text and a `StatusBadge`, reading the same once the badge's comma is spoken; where it also
lists the model and the effort, the account joins them as a labelled list under the agent, since a bare value such as
"high" does not say what it is, and the labels are the tooltip's own keys); its own key only where the only string
naming it folds it into the name of what carries it ("{name}, a session is waiting for you"), since reusing that would
make the tooltip repeat the row's visible name, and composing it out of a fragment is worse to translate. The session
context mark's card is the one accepted exception: it follows a card design with its own title ("Context window") and
its own compact "used / limit", "used" and "left" lines, while the trigger's name is one sentence of exact counts; the
"used" line is the one string the two share.

The anchored shape takes the hover from its caller rather than listening for one itself, so it can be scoped to a
single mark wherever that mark can carry the pointer handlers: the Git mode's change row hands it the hover from the
status letter alone and anchors it to the letter, and the rest of the row — the name and the detail, as one line —
keeps its own tooltip.

Where such an indicator carries a tooltip of its own, that name goes **on the tooltip's trigger**, not on an element
nested inside it: react-aria describes the trigger, so a name one level in leaves the description attached to a
wrapper that is nothing to assistive technology. An indicator whose meaning already reaches it some other way leaves
the trigger unnamed instead, adding nothing to the name of the row or trail around it.

What is left over is a shortfall kept on purpose, marked at its code: a sighted user who works from the keyboard reaches
no tooltip, so a name that lives only there is a name they do not get. (A status mark, a note included, is not one of
these: it is a popover on a button, which the keyboard reaches; see "How a mark is drawn and opened".)

On touch, a mark opens its tooltip's content in a popover when tapped (the mark shape's `touchPopover`, on by default;
see "A tooltip is the application's own"). It is turned off for every mark that sits inside something answering the
same tap, since the popover must not open over what the press does: the marks of a session row in the sidebar, in a
focus mode (card and archive preview) and in the archive view (status glyph, agent icon, context size and binding
badge, plus a focus mode's relative time), the archive submenu's relative time (a menu item), the project row's Git
badge, the activity marker (a project row, the console-switch chip), a branch's responsible-session badge (an option),
and the switch behind focus mode's include-console-sessions tooltip. The same mark where it stands alone, such as the
title bar's status glyph, the requested-sessions dialog's status and agent glyphs, a focus mode's header Git badge and
the history header's context badge, opens it. The rest of what a row's marks say reaches touch through the row's
accessible name alone, a kept shortfall: the row's press is its own, and no second route to the marks is built for it
yet.

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

- A control that shows or hides a region carries `aria-expanded`. One that opens a floating panel also carries
  `aria-controls` pointing at the panel's element, but not the pane toggles' `data-pane-toggle` marker.
- A control that shows or hides one panel of the right pane (a panel kind's toggle in the top bar) carries
  `aria-expanded` and, only while that panel is in the document, `aria-controls` pointing at it. It carries the right
  pane's `data-pane-toggle` marker, since it is what floats the pane in, so the pane's pointer watch counts it as part
  of the pane.
- A button whose name and icon swap with its state (the Terminal panel's Maximize / Restore) names the action and
  carries no `aria-pressed` or `aria-expanded`, or assistive technology would announce the state twice.
- The selected row of a hand-built list or tree carries `aria-current` (as the sidebar rows do).
- HeroUI's `Tabs` and `ListBox` mark their selection themselves with `aria-selected` and get no `aria-current` on top.

Every tab strip, including agent tabs, uses its content width and is start-aligned in the current reading direction.
Constrain the painted strip as well as the list and individual tab allocation; tabs do not stretch to fill the row.
The strip is bounded by its available width and scrolls when its content does not fit. A surrounding pinned header
and its end actions, or the divider drawn under the strip (the agent tabs of the rail's quota panel), may span the
section while the transparent tab list keeps its content width. Keep the scrolling and focus-ring clearance specified
under "Visible focus".

### Progress of a long operation

An operation that reports how far it has got (an account's sharing change) shows HeroUI's `ProgressBar`, which is a
`progressbar` role. Its accessible name says what is in progress, not the count; the count is its value text ("3 of
9"), and what is being processed now is visible text beside it. While the operation cannot report a count yet the bar
is indeterminate, with no value, and `aria-describedby` points at the text saying what it waits for. The bar is not
announced on every step: a `StatusAnnouncer` speaks the operation when it starts and again only when it moves into a
new phase (ending sessions, waiting for another change, resuming; see "Announcing a change"), and the result is
announced by its own alert.

### The menu bar icon and its native menu

A line of the menu bar icon's menu that only shows information (a heading, an account's header, a note, a quota
window) is a menu item with a view of its own, not a plain disabled item: AppKit dims a disabled item's text whatever
colour it names, so the view draws its own text in the system's label colours, and it never highlights.

The menu item is the one accessibility element: its title is the whole phrase, read once ("5h 82% remaining · resets in
1h 57m"); a line that draws two windows as one meter (5h over Weekly) has the two phrases joined as a list in the UI's
language for its title, still read once. It stays a disabled menu item, so assistive technology reports it as dimmed.
Nothing inside the view is an element: the view, its texts, its bar and a heading's picture are all marked non-element,
and the texts are drawn by a view rather than set in `NSTextField`s, since marking a text field non-element leaves its
cell exposed as a static text of its own. The view takes no role or label and the item keeps its title, or the phrase
would be exposed twice, or the line would have no title for menu search.

A decoration drawn over the menu bar icon itself, such as the raised hand while a session waits for the user, is
likewise marked non-element, view and cell: the menu's **Waiting for you** heading is what tells assistive technology
that state.

### Where an error is shown

Where an error or a warning is shown follows from how it arises, not from the screen it arises on:

1. **An error the user's own action directly causes** — a submitted form, a Save or a confirm, a switch, a menu or row
   action, copying, opening — opens the dedicated error alert dialog, below. That includes a submit or a save failing
   inside a dialog: the error alert dialog opens over it, and the dialog keeps its input.
2. **An error located at one input field** is the exception to the first and to the third: its error text stands
   under its field, in the field's `FieldError`, and no dialog opens. That is input validation — a check run
   synchronously the moment the user submits (empty, malformed, too long, a duplicate of a known name) — and equally a
   lookup the form makes on its own that finds the value in the field wrong (a path that does not exist or is not a
   folder, a conversation ID found nowhere) or a value the field holds that cannot be used (an account that is
   disabled, used up or changing its sharing). A lookup that could not run at all says nothing about the value and
   stays under the third rule.
3. **An important error from automatic behaviour** — one the application ran into by itself (a load, a lookup a form
   makes on its own, a refresh) that blocks what follows, or that a component cannot do its job without — is an Alert
   on the screen; in place of a whole page, pane or list, the empty-state failure ("A whole subject that failed").
4. **An error or a warning that is not important** — it does not block what follows — is a mark at the point it
   concerns; one with no point it can be located at is an Alert in a shared place (the top of a dialog's body, the top
   of a section). A warning about the consequence of a choice the user is making is not one of these: it is an Alert
   under that choice while the choice is selected.
5. **A result view showing a failed result** (an import's result whose application settings could not be applied)
   shows that result as an Alert, as the view's own content.
6. **A destructive confirmation** (removing, deleting, archiving) is a warning modal, as "Keyboard" sets out.

Rules 3 and 4 are "On the screen: an Alert when it matters, a mark at its point otherwise" below.

The Processes panel keeps a failed refresh visible as a warning Alert above its existing rows and actions. The list
may be stale, so the warning stays until a refresh succeeds; polling continues while it is shown.

**An error alert is for the exceptional, never for a refusal the window could have foreseen.** An action that cannot go
ahead in a state the window knows, or can look up quickly, before it is confirmed — a confirmation's option that cannot
be carried out, a submit whose subject is in use — is prevented, not refused: its confirm or submit button is disabled
while that state holds, and the reason stands where the action is, as an Alert under the choice that causes it or a
`FieldError` under the field — or, where the reason is a list of what is in the way (sessions still running), as a line
of a destructive confirmation's consequences with that list after it. Either way the choice that brought it about names
the reason in its description and it is announced as it appears. Letting the user confirm only to answer with an error
alert is not allowed. The error alert dialog is left to what could not be known beforehand: a state that changed between
the lookup and the confirm, the operation itself failing, and what only a slow check would find. Only quick checks run
ahead: one that would keep the user waiting (a look that outruns a short deadline, as on a volume that answers slowly)
is left to the operation, whose refusal then is an error alert. A quick lookup still running holds the button pending,
quietly, with no error shown. A refusal that comes back from the operation is looked up again once its alert closes, so
the reason then stands where the action is and the button is disabled, rather than the same alert answering a second
confirm.

Once a submit has passed the form's own validation, anything that goes wrong afterwards that the window could not
foresee — a refusal from the daemon included, even one about a single field's value — is a failure of the user's own
action and opens the error alert dialog; the form stays open with its values intact, under it. Field validation never
opens the dialog, and neither does a failure shown on the screen or as a mark: those are the error's only presentation,
with two exceptions: a check the user pressed for (Settings' Check for updates) that does not finish is shown in the
dialog and still leaves the setting's mark; and a long-running change the user started that ends unfinished, and which
its own control retries (an account's sharing switch), opens the error alert dialog in the window that asked for it and
still leaves the mark at that control as its lasting state, the dialog alone speaking it. The same change seen ending
unfinished from another window, or found so later, is the mark alone. A navigation the user starts in a picker (the
directory picker's Go, and its Try again) that fails is a failure of the user's own action too, even before any listing
has been shown: it opens the error alert dialog, and the picker's first-listing Alert stays as it was.

A control that records a key combination rather than taking text (Settings' Keyboard shortcuts rows) refuses a
combination the way a field's validation does: synchronously, in a line drawn as a `FieldError` under the row,
referenced from the row's controls by `aria-describedby`. Having no field to carry an invalid state, it speaks the
refusal through `StatusAnnouncer` instead, again on a repeated refusal in the same words.

#### The error alert dialog

Every asynchronous refusal or failure reported in reply to an operation the user started opens the dedicated error alert
dialog and remains there until the user presses its acknowledgement button. Errors never use toasts, timeouts, an
outside press, Escape or a close button for dismissal. Use HeroUI's `AlertDialog` with its `Icon` set to
`status="danger"` and a danger acknowledgement button, retaining the library's default visual treatment. The alert takes
focus and announces its title and body through `role="alertdialog"`, without a second live announcement inside
it; the body's opening sentence is optional, never restating the title, so an alert without one announces its title and
remedy; focus lands on the alert itself, not on its first control, also when it opens over a dialog that a press without a
pointer opened (an assistive technology's press), where Enter on a first control such as Manage accounts… would act.
Distinct failures wait in a queue; repeated copies of a pending failure do not multiply it. An error above a form leaves
that form available after acknowledgement, with its values and retry action intact. Acknowledging it returns focus to
where it was; where that is gone, or the dialog under the alert never had focus (the alert came in the same moment as
the press that opened it), focus goes where that dialog's own opening focus goes — never to the page (see "The error
alert" in `docs/product/error-alerts.md` for the full order). The control a dialog opens
on is marked for this with `openingFocus` (`packages/ui/src/components/regionFocus.ts`), which `Button` and `TextInput`
set from their `autoFocus`; a dialog that puts its opening focus anywhere else by hand marks that control itself. A
pending question (a trust prompt, a session request) that closes with nowhere to return focus to lands the same way, and
so does Settings closing with no opener left to return to.

The alert, and a pending question, stay usable whatever comes up with them. One that comes up in the same moment as
another modal — a dialog that a dialog opens as it mounts, or the alert and a question together — is on top and holds
focus, the other waiting under it (an alert over a question). A dialog opened later, while the alert or question is
up, goes on top of it instead: the alert or question waits under it, out of reach of the pointer, the keyboard and
assistive technology alike, and closing that dialog returns focus to it. The alert and the question mark themselves
as a react-aria top layer for the commit they mount in only (`topLayerAtMount` on `Dialog`), which keeps another
modal mounting in that commit from hiding them and leaves them to be hidden by any modal opened after.

Distinguish failures the user can resolve from unexpected failures. A recoverable failure explains the specific
remedy, such as freeing disk space, unlocking a session or correcting a folder's access. An unexpected failure
preserves its diagnostics, asks the user to file an issue describing the steps that led to it, and provides the issue
entry; an entry the application opens itself carries the diagnostics with personal information removed, while an
address the user opens by hand asks only for the steps. It never asks for a screenshot or for the raw diagnostics,
which can capture what the user would not share.
Do not ask the user to fix a likely application defect or present a known environmental problem as a bug.
An error alert dialog's recovery action is a secondary button in its footer beside the acknowledgement button, never a
button inside its body. The footer is trailing-aligned, so Report issue and the recovery action come first and the
acknowledgement button, the dialog's primary, stands at the trailing edge. Its body is the dialog's own prose in the
application's own words: the explanation, the remedy and any further explanation follow one another without an Alert
inside the dialog. What is not written for the reader — a message passed through from the system, Git, the daemon or an
agent, a stack, a diagnostic block — is never part of that prose, not even as a reference chip: it sits behind an
"Error details" disclosure under the body, each block on a surface of its own. A failure the application has no words
for opens with a sentence for its cause where the raw text names one it recognises (a full disk, a refused access, a
failed connection, or that the operation could not be completed) and with "Something unexpected went wrong." only where
it is unexpected, its raw text behind Error details either way.

A state that must not happen is prevented rather than shown as a screen: disable an entry whose action cannot
succeed, such as New session when there is no agent to pick. If that state arises while its dialog is open, the dialog
becomes a failure modal, below, explaining why, with only acknowledgement and no recovery action. This
acknowledgement-only presentation retains structured error metadata and rich name references.

**A failure that ends a dialog's purpose** — the dialog can no longer do what it was opened for — turns the dialog
itself into a failure modal in the error alert dialog's form, never an ordinary modal with a danger Alert inside it:
the danger icon in its header, a title naming the failure, the explanation as sentences in its body, the raw text
behind **Error details**, and its footer trailing-aligned like the error alert dialog's, secondary buttons first and
the primary at the trailing edge. It replaces the dialog's content in the same modal; no second dialog opens over a
dialog that has nothing left to offer. A login that failed or timed out is one: it has no close button and no Escape,
and its footer is Close then Log in again, which takes focus. The alert dialog's mounting and the focus move announce a
failure modal, with no further live announcement.

The error alert dialog owns the announcement of the failures it shows; nothing else announces them again. A failure
shown on the screen announces itself, once, when it appears after the view has first painted (`role="alert"` on its
title and first description only, never on its details or actions); the same failure already there when the view opens
is not announced, except a startup screen's, which is the whole page and announces itself as it opens (see "A whole
subject that failed"). An Alert that stands with the marks of the same list (the Git view's uncommitted changes that could
not be listed) does not announce itself either: the view speaks its title once through its own `StatusAnnouncer`, with
the marks'. A failure that appears together with a focus move (a dialog opening, a menu handing focus to its Try
again) holds its alert until the focus move has been spoken, as "Announcing a change" asks of any write near a focus
move. A failure shown beside other content (in a dialog or a section) stays in place while its retry is out, its Try
again pending; a retry that fails again is announced again even in the same words, and one that succeeds hands focus
on as the failure goes, so focus never falls to the page. A failure that stands in place of a pane's or menu's subject
that has a loading state of its own gives way to it when its retry starts, focus handed first to the pane's or menu's
own control; one with no loading state to give way to (the report panel's whole-card failure) stays in place like the
first kind. An initial startup explanation is ordinary page content. Follow "Announcing a change" for other live
announcements.

#### Errors shown among other content

An error shown on a screen together with other UI — a form dialog, a settings section, a pane, a popover, a list's
header — that does not block what follows is a mark at the point it concerns (see "On the screen: an Alert when it
matters, a mark at its point otherwise"); an important one — one that blocks, or that the component cannot do its job
without — and one with no point is one compact Alert: its title says what happened, its description why, a second line
the one main thing to do, and everything else is behind **Details**. Nothing about the error sits beside the Alert as
loose text, and no part of it is a bare line of danger-, warning- or muted-coloured text, an empty-state panel, or a
disabled menu item: colour and position alone do not mark an error, it needs the Alert's indicator and fill. A failed
lookup a form makes on its own that stops the form from going on (detecting a project folder, looking up a
conversation ID) and could not run sits in an Alert directly under the field it is about — one that ran and found the
field's value wrong is that field's `FieldError` (rule 2) — while one that leaves the form usable (reading the model
list) is a warning mark after the field's label; a reason a dialog cannot proceed that lies outside its fields sits in
an Alert at the top of its body.

The Alert holds, beside `Alert.Indicator`: an `Alert.Title` saying what happened, then at most two sentences in the
application's own words: an `Alert.Description` saying why, when that is known, and a second one with the one main thing
to do about it. At its end, in the place HeroUI's own Alert puts its action button, stand at most two buttons:
**Details** when there is more to show, then the recovery action when there is one (Try again, or the action that fixes
it) at the trailing edge, where the primary button of a trailing-aligned group goes (see "Button order follows the
group's alignment" under "Buttons"). Details opens a popover holding the rest: first any further explanation in the
application's own words (a reason per cause, each cause's own fix, further guidance), then what is not written for the
reader — a message passed through from the system, Git or an agent, a diagnostic block, raw entries, each block on a
surface of its own (see "References in copy use `InlineReference`"). An Alert never shows raw text outside Details (a
whole subject that failed shows it on the page, see "A whole subject that failed"), and the one-sentence description and
main remedy are never hidden in it; an error with nothing beyond those two sentences has no Details button. A catalog
message keeps the same split: its visible summary and remedy carry no raw parameter (no system text, no `{detail}`), and
any further explanation is a `.details` entry of its own that Details shows first. Try again appears only where trying
again can change the answer; a failure that is the subject's state (no common commit, not a repository, a file outside
the scope) has none. The buttons take the Alert's status colour, as HeroUI's own Alert examples do: the recovery action
is the solid button of that colour (HeroUI's `danger` variant in a danger Alert; the same solid treatment from
`--warning` / `--warning-foreground` in a warning one, HeroUI having no warning button), and Details is the soft button
of that colour (`danger-soft`, or its warning counterpart), so neither reads as a neutral control dropped onto a
coloured box. A Details shown outside an Alert, on a line of a result, takes that line's status the same way: the
warning's beside a warning, danger under a failure (`AlertStatusContext`).

Every Alert — an error, a warning — has a title. HeroUI centres the indicator on the title's 24px line, so an Alert
that opens with its 20px description draws its first line off the glyph's centre; the title is what keeps them
aligned, not a CSS exception. The title does not merely repeat the description. The Alert wrappers require it in their
types, and a lint restriction keeps HeroUI's `Alert` from being used outside them.

An Alert holds everything that belongs to it: its title, its description, its remedy and, behind **Details**, the items
it is about, such as the entries a failure lists, with further explanation and raw text. Nothing of the Alert sits
outside it as loose text, and nothing beyond its two sentences is drawn in its body. An error-only screen's instructions
are not part of the Alert and stand around it, as below.

The Alert is HeroUI's `Alert`, through `ErrorAlert`, for danger and for a warning alike. It takes its status background
fill and status-coloured shadow (see "Confirmed visual choices") as its only custom visual treatments;
indicator, typography, spacing, radius and component structure stay stock HeroUI. A danger title uses `Alert.Title` with
its original status colour; `Alert.Description` keeps its default colour. Do not apply `text-foreground` or another
colour override to either part, override text sizes, or replace `Alert.Title` with a heading element. Neutral absence,
loading and unsupported content keep their state presentations.

#### A whole subject that failed

A page, pane or list whose whole subject could not be loaded has nothing else to show, so its failure takes the empty
state's shape (see "An empty state is a glyph, a heading, and what fills it") rather than an Alert: a danger glyph, a
heading saying what could not be loaded, the line under it in the application's own words (why, and how to recover),
then what an Alert's Details holds — further explanation in the application's own words, then the raw text — on a
`Surface` of its own under those lines, bordered with the separator colour so it stands apart on a dialog or a viewer as
on a pane, start-aligned and outside the announcement, its raw blocks wrapped and `tall` so they neither scroll sideways
in that narrow box nor push the row off the page, and in its centred row the recovery action (Try again) as the primary
button. Nothing else marks it as an error: no status mark beside the heading and no Details button, since the page has
room for the whole failure. The cap on a raw block is a share of the window, so where the panel fills a pane (which does
not scroll) or a dialog's body, it scrolls in its place, a region named by its heading (see "A scroller nothing else on
screen scrolls takes focus itself"). An Alert is for a failure shown among other content.

A startup screen whose whole subject failed — the daemon that failed to start, and the crash screen — is a page whose
whole subject failed like any other: it takes the empty state's shape above, its heading the page's `h1`, the line
under it in the application's own words and the remedy after that, and no error alert dialog opens over it: the page
carries everything such a dialog would. It is the failure's only presentation, so it announces itself as it opens, the
heading and the first line as `role="alert"` (the empty state's `announce`), inserted after the first paint by a delay
of zero so that it is a change to the page rather than part of its first render. Nothing takes focus by itself, there
being no dialog to. Tab reaches any region that scrolls, then the row's buttons; on the crash screen focus has fallen
to the document, as what held it went with the crashed view and there is nothing to restore it to.

Every error screen — the daemon that failed to start, the crash screen and the screen with no daemon address — offers
Report issue and has no Quit button. Report issue is the error alert dialog's own prefilled link (`useIssueReport`),
built from the screen's own text: the daemon's message, the crash's stack, or the no-address screen's sentence. It
reads the record names to remove from the daemon in context, so that daemon is provided around the crash boundary too.
The row holds at most two buttons: the recovery as the primary (Try again, on the crash screen) and then Report issue,
or Report issue alone where there is no recovery (the daemon screen). Where the platform cannot open links the link is
left out and a sentence names the address to report at instead.

The crash screen is the one failure that may be a bug: its remedy line already asks for the report, so the sentence
about it (that this may be a bug) stands, first on the surface above the stack, only where there is no link, and the
Issue is titled "Unexpected error: " followed by the first line of the error. The daemon that failed to start and the
missing address are failures the application knows and says what to do about, so neither is called a bug: their sentence
says to report it if the steps given do not resolve it, stands with the link (first on the daemon screen's surface,
after the instructions on the no-address screen), and their Issues carry the screen's English heading as the title.

Quit is left out because the window still quits through Cmd+Q, the app menu, the menu bar icon's Quit and, where it
quits rather than hides the window (not while the user runs in the background with a live daemon), the close button.
Only the crash screen registers for the quit flow, since the application had registered before it crashed and the
shell cannot unregister it: it answers a quit at once, with no confirmation, asking about no sessions and shutting no
daemon down.
The other two register nothing, so the shell quits natively. The raw text — the daemon's message or the crash's stack —
sits on the surface under the lines, as on any other such page. A screen whose only content is an explanation with no
raw text in it — the startup screen with no daemon address, and its instructions for giving one — is the error's own
surface, as the error alert dialog is, kept apart on purpose: it is no failure of something loaded but a configuration
still to be given, so it is not the empty state's shape, and its heading and instructions may stand around the Alert.
The heading is centred over the column, as is the Report issue button under the instructions; the Alert, the
instructions and any list in them are start-aligned, so a list's bullets stay beside their text.

#### On the screen: an Alert when it matters, a mark at its point otherwise

Of what is shown on the screen rather than in the error alert dialog (rules 3 and 4 of "Where an error is shown"),
where an error, a warning or a note is shown follows from whether it matters — not from how much it has to say or how
busy the layout around it is. The Processes panel's refresh warning is the explicit exception in "Where an error is
shown":

- **An important error from automatic behaviour** — one that blocks what follows (a lookup a form makes on its own
  failing, which stops the form from going on; a reason a dialog cannot proceed), or one a component cannot do its job
  without (a list part that could not be read while the rest of the view still shows, such as the Git view's
  uncommitted changes) — is an Alert even where it can be located, so that it is said outright rather than one press
  away: a failed lookup directly under the field it is about, a reason that lies outside a dialog's fields at the top
  of its body, a list part at the top of that list.
- **A warning about the consequence of a choice the user is making** — a risk the user takes on by selecting an option
  in a form, such as an export that will carry full paths of the user's machine — is a warning Alert directly under that
  option, shown while the option is selected and gone once it is not, never a mark. A mark keeps what it says one hover
  or press away, and a risk the user is opting into is said outright at the moment of choosing. Its title says what the
  choice brings about, its description what that exposes, and its remedy the one thing to do about it. It reaches
  assistive technology with the choice: while it is shown it is in the option's `aria-describedby` after the option's
  own description, and as it appears on a selection its title and description are announced once (`ChoiceWarning`, an
  `ErrorAlert` that announces), since focus stays on the option and the Alert is no tab stop. The option's own
  description still follows its state (see `.claude/skills/knowledge-i18n-copy/SKILL.md`, "Descriptions under toggles")
  and does not repeat the Alert. A warning about the state of something — a check that failed, what could not be read —
  stays a mark, even beside a checkbox. In a destructive confirmation an option's consequence is instead a line of the
  warning modal's consequences, or of its section's in a sectioned one (see "Keyboard"); this Alert is for a form or a confirmation that is not destructive.
- **Any other error or warning, and a note** — one that does not block what follows — never takes an Alert where it has
  a point. It is a mark at the point it concerns: `StatusMark`, its glyph right after the name, heading or line it
  qualifies — a row or an item, a name, one section's heading in a list, a page's heading or a pane's or view's title
  (the listing or the file it names, while its content is still shown), a line naming what it is about (the Git source
  block's Worktree line), a checkbox's label — and for a field, after the field's label when it is about the whole field
  (its list of options that could not be read), and inside the field's group at its trailing end, before the field's own
  trailing buttons (Clear), when it is about the field's current value or input; for a setting, in the row's control
  area beside the setting's control — before a button, after a switch. What it has to say is in its popover, opened as
  "How a mark is drawn and opened" describes; a popover taller than the room beside its mark scrolls inside itself. A
  warning takes the triangle in the warning colour, an error the circled `!` in the danger colour, and a note, which
  only informs, the circled `i` in the note's accent colour; a circled `i` is never drawn in the warning colour, nor a
  triangle in a note's. Nothing of it is written beside the mark as loose text, muted or coloured.
- **Several of those at one point** are one mark, never marks side by side: its popover lists each as a paragraph of
  its own, and its name covers them all. An error and a warning at one point are one mark in the more severe tone, the
  error's.
- **One of those with no point it can be located at** is an Alert in a shared place: the top of a dialog's body, the
  top of a section.

Here and in "How a mark is drawn and opened", a mark is a status mark (`StatusMark`): a button that opens a popover,
or, with a word and nothing more to say, that glyph and word as plain text. The glyphs elsewhere in this standard that
carry only a tooltip (an agent icon, a badge, the not-installed agent's warning glyph, an import region tab's warning
glyph, the branch badge's failure glyph) are indicators, not marks in this sense, and are not placed by this rule.

A mark that appears by itself after the view has first painted, as a change of state with nothing else changing around
it (a setting's last check failing while the view is open), is announced once, its name or the sentence it stands for
and nothing more: a mark has no live region of its own, so the view speaks it through its own `StatusAnnouncer`, and
says nothing for a failure the error alert dialog already speaks. A mark that arrives with the content it qualifies (a
listing, a search's results, a step of a dialog) is read with that content and is not announced apart from it; nor is
a mark found already there when the view opens.

A whole page, pane or list whose subject failed is not placed by this rule: see "A whole subject that failed".

A menu has no room for a mark: a limit a menu hit is its last item, a note in the menu's muted text, lined up with the
items' labels and carrying no glyph of the items' kind, that says why the rest is missing ("There are more branches
than this list can show"), never worded as a failure.

#### How a mark is drawn and opened

This section sets how every mark is drawn and opened, wherever "On the screen: an Alert when it matters, a mark at its
point otherwise" places it: its popover, its name, the input that opens it, and its in-heading and inline forms.

A failure, a warning or a note belonging to one item of a collection, one part of a compact layout (one section of a
side pane, such as the Committed section of the Git change list or the Git pane's branch list, with the rest of the
pane still shown), or one spot in rendered content does not break that row, section or sentence into an error block.
The item shows a minimal mark (`StatusMark`): a danger, warning or note glyph with an accessible name, placed **right
after the name on a row that has one** (a tree folder's, the account name line's), not at the row's trailing edge: the
name keeps its clipping and fade, so a long name clips and the mark stays visible after its visible text, and a hover
marquee, if the name has one, stays the name's alone ("Hover marquees belong to one text label"). Elsewhere it takes
the place of its own layout: a status column of a table, beside the section's heading for a part of a compact layout,
inline with the text in a document. A failed Git check of a project stands at the start of its
Git badge, in place of the badge's branch glyph, outside the badge's tooltip trigger and as a tab stop of its own; while
a check, pull or push runs the refresh glyph takes its place, and the mark comes back when that run ends unless a check
in it succeeded. The row's accessible name adds the mark's name ("Git check failed") as its own phrase; Git's text is
in no name and no tooltip, only in the mark's popover. A press on the mark keeps to itself and does not reach the
pressable row around it, and when the mark goes away while it or its popover holds focus, focus goes to the row (or
focus mode's header). No sentence of error text is written into the row, except in a report of outcomes (below).

A mark placed after a line of text is centred on that line by construction: the box holding it is one line of that
text's own size and line height (`MarkLineBox`, `h-[1lh]` under the text's size class), the mark centred in it, so it
follows the text's first line when the text wraps or changes size. Never align it by a fixed pixel height, a `min-h`, or
the height of the box around the text, which need not be the height of its line (a checkbox's content box is taller than
its label's line, its text at the top).

**A mark is always a popover, and never also a tooltip.** What an error or a warning carries can hold a button (Try
again) and raw text to select, which a tooltip cannot, and one mark wiring both surfaces would put two floating things
on one glyph. A mark whose content is one short line is the same popover with that line as its content, which may be
drawn richer than the mark's plain-text name (a path as a reference). Its dialog is named by the tone's word (Error,
Warning or Note), not by the line, which VoiceOver would otherwise read twice. It is drawn on the **tooltip's surface**
like every floating surface of the application. A mark with a body, a button or raw text is drawn on it too, its content
laid out inside. The exception is a mark that draws a word beside its glyph and has nothing more to say: the word
already says it on screen, so it stays the glyph and the word as plain text, with no button and no popover. The popover
holds the reason in the application's own words, then the main remedy, any further explanation, then the raw text, then
the recovery action (Try again, at the popover's trailing edge). A mark whose content is one line and raw text holds
that line, then the raw text in a diagnostic block under it; raw text in a mark is always such a block, never inline
text or a chip. The glyph is a button named for what it is about, with no tooltip (the popover says everything one
would). A note (an informational mark, circled `i`) is the same button and the same popover with nothing to press in it:
marks of every tone side by side look alike, take the pointer alike, are each reachable from the keyboard (a tab stop,
or a step of their collection's own walk) and open the same way, so they differ only in glyph and colour. Glyphs per
tone: circled `!` for danger, a triangle for a warning, circled `i` for a note.

The popover is `Hint` of kind `popover` and follows the input matrix of "A floating panel that does not take the
window": **a mouse click opens nothing**, and its buttons and text are reached with the mouse by moving the pointer into
the hover-opened popover, which stays open.

Where the row's other outcomes are each a glyph and a word in a status column, the failed outcome is the same shape
(its glyph and the word Failed) and that pair, where there is detail to show, is the one trigger of the popover, named
by the row's own label. In a section's heading (the Git
change list) the trigger takes no tab stop of its own; the application's own walk reaches it (Right from the heading's
band), and when a press-opened popover (touch, keyboard, assistive technology) closes, focus goes to the heading's band,
which takes it only when focus was on the mark or the page body, as for a press from scratch. A hover-opened one moved
no focus, so closing it moves none.

In a tree, a folder whose listing failed carries the mark on **the folder's own row**, after its name, and is drawn
**collapsed** (`aria-expanded="false"`, the arrow closed) with nothing beneath it: react-aria decides a row's expanded
state from the children it holds, so an expanded row with none would be a leaf, without `aria-expanded` or Left to
collapse, and nothing is ever drawn that way. Opening it again (a press or Enter on the row, the arrow) asks for the
listing again, as Try again in the mark's popover does, and a press on the mark does not. Right (Left, right to left) on
the row walks to the mark instead of expanding, taken before react-aria's row handler (a capture listener on the
document, since React's root listener runs ahead of anything on the row), then on to the row's actions button, and the
opposite key goes back, as along any row; Tab leaves the tree, as it does for every tree row. The folder stays among
what the user expanded, so it is open again once it lists. A failure arriving while the folder is open, and Try again,
take rows or the mark away, so focus that fell to the page or the tree goes to the folder's row.

**The inline form** is the mark inside running text (a Markdown image that could not load, a formula that could not be
drawn): the glyph takes the text's size, the trigger is a button whose box is square (sized from the glyph, so its round
shape is a circle as everywhere else) with negative block margins that leave the line's height as it was, and a button
is phrasing content, so valid inside a paragraph. It is a tab stop like any other mark, so a sighted keyboard user
reaches the reason, which the tooltip it replaced never gave them; the document's links and text are untouched, the mark
is one more stop, named by the sentence, and its popover is named by its tone's word. Inside a link (a linked image that
failed) the mark is drawn right after the anchor, not in it, so no button nests in a link and a press on the mark is not
a press on the link. Its target is exempt from "Pointer target size" as inline text is. A note in running text is the
same button.

**A report of outcomes** is the one place a failure's reason is written into its row: the import dialog's result,
whose whole purpose is to say what happened to each console, project and account (`ResultCards.tsx` in
`packages/ui/src/configuration/`). Each row says its outcome as a glyph and a word, and a failed row writes the reason
in the application's words and the main remedy (in the secondary text colour) under the row as text; the further
explanation and the raw text stay behind a **Details** button that opens the same popover a mark's would, so raw text
is still a diagnostic block and never inline. A row that succeeded with a warning (an account created whose sharing
could not be turned on) writes the warning's sentence under the row the same way, its glyph in the warning colour,
followed by the remedy and Details. A failed row without a reason is the glyph and the word alone. Anywhere else, a
failure in a row is a mark.

### References in copy use `InlineReference`

`InlineReference`, backed by HeroUI Chip, has one job: marking a reference mixed into running copy, where Markdown would
write `` `xxx` `` and plain prose would use quotation marks. It is the only component for that; do not use HeroUI's
`Typography.Code`, a raw `code` element, HeroUI `Chip` directly or a hand-built pill for a reference in copy. It is not
a badge: a status or qualifier after a name is a `StatusBadge` (see "Confirmed visual choices"), and a name shown on
its own is plain text.

- **A reference within copy takes a chip.** A name, title, control label or technical literal referenced within
  visible prose, headings, descriptions, Alerts, field errors, tooltips or toasts uses an `InlineReference` instead of
  quotation marks. This includes fixed agent and product names when they are references within copy. A name or
  title uses the non-monospace chip with `dir="auto"` and wrapping. A technical literal — a path, a file name, a URL,
  a branch, a commit id, an ID, a command or a setting value — uses its monospace, left-to-right and wrapping options,
  stands without quotation marks, and is bare in its plain-text form.
- **A name shown on its own takes no chip.** A name or value displayed by itself rather than inside a sentence —
  a row's title, a list entry, a heading that is just the name, a label naming its control, a value on a line of its
  own — is plain text in the style of where it sits (a technical value keeps the
  monospace font, without a chip). So is a run of names joined only by a connector such as `・` or `｜`, with no
  words around them. When such a joined run is itself referenced within a sentence, it is one reference and takes
  one chip around the whole run, never one chip per name.
- Whole entity and control labels, structured entity/path rows, and foreign source, terminal and report layouts keep
  their established presentation. A focus card's model line — the account, then the model and reasoning effort, beside
  the agent mark — is such a whole metadata label: plain text without chips. Its model and effort are the same isolated
  string the card's accessible name ends with; the account is not repeated there, the name already carrying it with the
  agent. So is the agent's name heading the agent mark's tooltip: it names the mark rather than being referenced within
  prose.

Project filter conditions are tags rather than references in copy: the heading summary and its overflow tooltip
use the same `Tag` component as picked project tags, in its read-only form without remove buttons, selection or
keyboard focus. The keyword takes the neutral grey tone and categories take the accent tone. The heading keeps one
line and owns clipping and its marquee as a whole; the tooltip wraps tags and breaks a label longer than its width.
Read-only tags keep the resting appearance of picked tags without a disabled appearance or interactive hover states.

Plain-text surfaces, including accessible names, native labels, announcements and system notifications, retain readable
text without chip markup. Isolate references at the sentence boundary where needed, as specified under "Text direction";
collection keys, `textValue`, sorting, defaults and submitted values stay raw. Raw diagnostics shown as a block of their
own sit on a secondary `Surface` of their own, never straight on the dialog, Alert or popover that holds them, with the
summary or label naming them outside that surface; they preserve their text, direction and indentation, and long content
remains scrollable and reachable from the keyboard, the focus ring drawn on the surface. A block that may be taller than
its room (a crash's stack) is `tall`: it scrolls vertically as well, with the same named region and ring. A `wrap`ped
block breaks long lines instead, so it has nothing to scroll sideways and, unless it is `tall` too, no region to name. A
`tall` block is a region and fades only for its height, so one whose lines can be wider than its box (a crash's stack, a
daemon's startup failure) also wraps. Keep structured error metadata separate from the plain error message, so
notifications and unknown-error fallbacks retain usable text.

#### A chip's size follows the line it sits in

A chip always takes the font of the text it sits in — its size, weight, line height and family (a technical literal
keeps its monospace `code` child at the same size) — and sits `inline` on that text's baseline. The four sizes differ
only in padding, the tint drawn around the text:

| Size | Padding (block, inline) | Drawn height |
| --- | --- | --- |
| `xs` | 0, 4px | The text's content area, about 1.2 times its font size |
| `sm` | 1px, 6px | Content area + 2px |
| `md` | 2px, 8px | Content area + 4px |
| `lg` | 3px, 10px | Content area + 6px |

The drawn height must stay below the copy's line height, so that chips on wrapped lines never touch and no line is
stretched: a size larger than the largest that fits is never used. In 12px text on 16px lines (field errors, secondary
descriptions, tooltips) only `xs` fits; in 14px text on 20px lines (body copy, labels, Alert descriptions) up to `sm`;
in 16px text on 24px lines (dialog headings) up to `md`; in 14px text on 24px lines (Alert titles) up to `lg`. `xs` is
the default, safe in every copy style of the UI, and is what copy uses unless it has a reason for more. No size changes
the chip's font.

A chip that moves to the next line whole (`unbroken`, an `inline-block`) is as tall as its line box, so it touches the
next line's chip unless it tightens its own line height to the content area, which the component does for it.

A path's length, monospace font or position does not select its size. Technical literals keep their monospace code child
while the chip's background uses the same font metrics as a name chip. Keep a sentence in ordinary inline flow rather
than making its text runs and chips separate flex items that stretch to different heights. When a technical value
completes a sentence, fill it through that sentence's localized placeholder; do not keep a separate paragraph or a
colon-led block for the value. Long values wrap at slashes or anywhere they need to, within the available width; they do
not select a smaller size. Sizes belong to the shared component rather than caller-level font, line-height or padding
overrides.

The `monospace` option preserves a semantic `code` child without introducing a second visual component. Paths and
URLs can wrap after slashes, and longer uninterrupted literals can wrap anywhere. A typed confirmation word referenced
in its label inherits the label's font size, uses italic text, and keeps the ordinary UI font. Reserve `CodeBlock`
for multi-line code or raw diagnostics, never as a larger presentation of one path. Structured path displays are
separate from copy: file and directory lists, settings rows whose value is a path, and the viewer's primary
file-path row retain their shared path components, including their icons, clipping and full-path tooltips. Path
input fields and source or Markdown content also retain their own layouts.

Inline references retain HeroUI's colours, soft fill and shape; the help hue is the project's purple extension. Their
fill and text come from the tint tokens shared with `StatusBadge` (see "Confirmed visual choices"); the neutral chip is
the neutral fill rather than HeroUI's `--default-soft`. `CodeBlock` uses the shared Typography wrapper for multi-line
diagnostics. Outside an Alert, both default to neutral and may choose a colour. The Alert wrappers supply
`CodeToneContext`, passing the Alert's status, warning or danger, straight through as the chip's colour. A chip drawn
off that fill — in the Alert's Details popover — leaves the tone and keeps its own colour. The nearest status context
takes precedence over an explicit colour, including field errors and toast titles. Do not hand-colour individual
literals within a status message. HeroUI's Alert parts remain the indicator, content, title and description primitives.
Syntax-highlighted source and foreign renderer markup retain their own palettes.

### External links

Application-authored links that leave the app use `ExternalLink` with a real destination and a visible trailing
external-link icon. The destination remains available to keyboard users and the browser's link actions. The icon
is decorative; the localized accessible description explains that the link opens in the browser. Browser links
open a new tab with `noopener noreferrer`; the desktop app uses its platform link adapter. An unavailable or failed
opener produces the dedicated actionable error alert rather than navigating the desktop window.

Ordinary prose links use the shared text appearance: the surrounding font size and line height, link colour and an
underline. A control label mentioned within copy uses the shared chip appearance, without quotation marks. A chip that
only names a separate control remains non-interactive; a chip is never itself a link. A footer may instead provide one
external action using the shared button appearance, which applies HeroUI's secondary-button styles to the anchor; its
reference in the body is a plain chip. A startup screen's Report issue takes the same appearance, in the empty state's
row or under the no-address screen's instructions (see "A whole subject that failed"). Do not make both the reference
and the separate control interactive. All appearances retain a visible keyboard focus ring, and a pending open keeps
the label and icon visible while preventing repeated activation. Callers may not remove the icon or substitute
arbitrary local colours, padding or typography.

Native menu items that open a browser use the same external-link indicator within their menu primitive. Links rendered
from Markdown retain the document's text and title, with the external indicator added to outbound destinations and the
document renderer's measured link palette. Relative paths and document fragments keep their file-viewer navigation. Raw
terminal output and foreign renderer markup retain their renderer's content model.

### A visual heading is a heading element

Text drawn to read as a heading is `<h1>`-`<h6>`, never a `<p>` carrying a heavier weight: the structure a screen
reader navigates by is the element, not the look. HeroUI's `Alert.Title` keeps its original element and styling as
part of the stock Alert structure; it is not a page or section heading. Put a page or section heading outside the
Alert when the surrounding content needs one.

The converse holds as well: a heading element still has to be drawn as one. A heading inside a container whose base
style sets the secondary text size and colour inherits both, and weight alone is too little to read as a heading — so
it carries its own size and colour instead. `EmptyPanel`'s full panel is the case: HeroUI's `.empty-state` puts
`text-sm text-muted` on the whole panel, and the heading overrides both so it leads the line under it.

A heading also never outranks the one above it. `EmptyPanel` takes `headingLevel` for that: its heading stands at the
level of the heading naming whatever it fills, so the two read as one thing. The default `<h2>` is the level of a
pane's own title (`ArchiveView`, `ProjectBrowser`) and of a dialog's, which react-aria sets for every `Frame.Heading`.
The sidebar's empty lists use compact rows instead, which add no heading to the outline. Markdown rendered inside a
view that has its own heading (an agent reply in a session's history) is rendered `embedded`: its headings start one
level below that view's, and it sets no heading ids, since several such documents share one page.

### An empty state is a glyph, a heading, and what fills it

Every place that has nothing to show takes one shape, through `EmptyPanel`
(`packages/ui/src/components/EmptyPanel.tsx`): a glyph, a heading naming what is missing, optionally a line under it,
optionally a failure's details on a surface under that line, and optionally a centred row of at most two buttons that
wraps — the action that fills it, then a second way out of the state (on a startup failure, Report issue as a link in the
shape of a button, in place of a secondary action) — putting its primary first, as "Button order follows the group's
alignment" under "Buttons" has it. The whole shape, and which state takes which part, is
`docs/product/empty-states.md`. Nothing builds a second one out of centred text, and a state with no glyph is not one of
these — HeroUI's bare `EmptyState` is reached only through this component.

- The heading is a short phrase carrying **no punctuation at all**, a comma inside it included: a title that needs one
  is a sentence, and the part after the comma belongs in the line under it. "No sessions yet", not "No sessions yet,
  start one from the project row".
- A normal absence, such as no agent having been added, uses a neutral or purpose glyph, never a warning glyph.
- The line under it says only what the heading cannot: why it is empty or a bare fact about the subject (the size of a
  body that cannot be drawn). It never restates the heading, and a state that has nothing more to say carries no line at
  all. A button takes the user where the thing is made; the line does not give navigation instructions. A second way out
  is a different action, not another entry for the same destination.
- The `compact` row, for a sidebar list in either normal or focus mode and for other nested lists, has no room for
  that line. Its message is plain text rather than a heading; where the list has one creation action, the whole row
  performs it. The main content's full panel carries the current action without a competing full panel in the sidebar.
- A load failure that leaves the whole place with nothing to show takes this shape too, as its error presentation: a
  danger glyph, a heading saying what could not be loaded, its details on a surface under the line, and Try again in the
  row (see "Errors shown among other content"); its copy follows the "Errors" section of
  `.claude/skills/knowledge-i18n-copy/SKILL.md`. It is told from an absence by its danger glyph and its retry. What the
  subject *is* is a state of the subject, not a failed load, even when the panel warns about it: a project that is in no
  repository, a worktree that is gone, a repository Git itself refuses, are all empty states, since no retry would
  change the answer.

Where a region elsewhere already speaks the state, the panel takes `silent` and is hidden from screen readers so it is
not read twice. What that region says is then the only way to hear it, so it says the whole state as one sentence, from
a message of its own — a `.sentence` beside the heading and its line, never the two joined in code, which runs together
differently from language to language. The same message serves a state shown as a notice about content that is
displayed anyway, rather than as a panel standing in for it. `silent` cannot be combined with an action: a focusable
element inside an `aria-hidden` subtree keeps its tab stop and loses its name.

### Landmarks

The content panel always holds exactly one `main`, whichever of its views is on screen — the terminal pane, or the
welcome screen that replaces the three panes while there is no console. A view that is a region rather than the
window's main content carries `role="region"` with a name (`<section aria-label>`), per the naming rule above.

### A context menu has a visible trigger too

A menu that opens on right-click also opens from the keyboard with the Menu key or Shift+F10 on the focused element,
with focus on its first item, as a menu opened from its trigger with the keyboard has, so Enter acts at once (a
right-click's menu takes focus itself, as react-aria's does for a pointer); a visible `ActionMenu` trigger offers the
same items, since nothing on the element tells a keyboard or screen-reader user that a context menu exists. The
Settings agent strip is the instance: each tab's menu, plus a "More actions for {agent}" button at the strip's end
acting on the selected tab. Reordering by dragging has a keyboard counterpart in that menu, labelled by reading order
("Move earlier" / "Move later", never left and right, which swap in a right-to-left layout), and the moved item gets
focus back once it lands.

### Two sentences in one paragraph

VoiceOver drops a text node that is only whitespace, so two sentences separated by one run together when read
("permanently.The project's", in `docs/voiceover-cases.md`). Where the UI places two whole sentences side by side in
one paragraph, the space between them is part of the text node of the sentence after it, never a node of its own;
`<Message>` (`packages/ui/src/i18n/react.tsx`) keeps a separator standing alone between two placeholders out of a node
of its own where the parameter after it allows. What separates the sentences is the language's to say (none after 。 in
Chinese), so it comes from the catalog, never a hard-coded space.

### Announcing a change

State that changes without the user acting (connection, a terminal problem) is announced from a `role="status"`
element, an error from `role="alert"`.

- A `role="status"` region meant to announce text that comes and goes stays mounted, empty when there is nothing to
  say, and only its contents change. One rendered conditionally, already holding its text when it is inserted, is not
  reliably announced (VoiceOver in WebKit among others).
- Even a region that stays mounted afterwards is not announced for the text it was first rendered with.
  `StatusAnnouncer` defers its first write for that, so announce a state with it.
- VoiceOver also drops a status written within about 300 ms of a focus move (a dialog opening or closing, say), so a
  surface that takes focus as it opens delays its first status write (`StatusAnnouncer`'s `firstWriteDelayMs`, with
  `AFTER_FOCUS_MOVE_DELAY_MS`).
- A status written while its region sits under an open modal is effectively written into a hidden region: the modal
  hides everything outside it from assistive technology, except an element marked `data-live-announcer="true"` or
  `data-react-aria-top-layer`, and closing the modal moves focus, which replaces what was written. A status about what
  the modal's own action produced (a login that ends by closing its dialog) is therefore written after the close and
  the focus move, a delay past both, not at the moment the state changes.
- A status about something that can happen while the user is out of the window (a login finishing in the browser) waits
  until the window is attended (`useWindowAttended`: the document is visible and the window has focus), then
  `AFTER_FOCUS_MOVE_DELAY_MS`: on the way back VoiceOver speaks the window's focused control, and an earlier status is
  never spoken. A record that clears itself counts only unbroken attended time toward its lifetime, restarted on each
  return, so it is not gone before the user is back to hear it. A success already present when the row mounts is not
  said. The instance is `AccountLoginAnnouncer` with `useClearSucceededLogin` in
  `packages/ui/src/settings/AccountLogin.tsx`.
- A state whose visible form is durable copy rather than a placeholder — the sentence saying where a macOS permission
  stands, in `packages/ui/src/settings/PermissionsSection.tsx` — stays readable in place, not `aria-hidden`, and is
  announced from a `StatusAnnouncer` the section mounts itself, through `AttendedStatusAnnouncer`, `sr-only`, beside the
  rows. A `role="status"` on the copy itself is not enough: the copy is present at the region's first render, as focus
  moves onto the tab, and that text is not said. So every text, the first included, is written
  `AFTER_FOCUS_MOVE_DELAY_MS` of attended time after it arrives, the wait started over each time the user comes back (a
  change made in System Settings is picked up on the way back, when VoiceOver is speaking the focused control), and a
  text replaced while it waits is never said. Put the subject in the text when several states can change independently,
  as whole sentences per state rather than fragments joined in code: `role="status"` is atomic, so a bare "Granted."
  names nothing. Reading the rows statically meets each state twice, once in place and once in the status; that is
  accepted.
- The file viewer's notices are announced only when they arrive after the viewer has painted, once, from a
  `StatusAnnouncer` with the viewer's first-announcement delay; one present from the first paint is not said, and a
  surface that already speaks the same state (an unchanged change, an expanded diff's notes) says it in place of the
  notice. Each such region passes `clearAfterRead`, which empties it `READ_TIME_MS` after the write: the mark is named
  by the same sentence, and reading the page line by line meets it only on the mark. A conflicted file's failed read is
  an error and is announced by its `Failure` as an alert, like any read failure.
- A status region for content that a disclosure folds away is mounted outside the folded body, and only while the
  disclosure is open. The body stays mounted while folded, so a region inside it would be past its first write delay
  when the disclosure opens again, and the text kept from before would be written in the very render that shows the
  body: text that appears with its region, which VoiceOver does not say. Mounted with each opening, the region writes
  after its delay with the trigger still focused. The instance is `AdvancedSection` in
  `packages/ui/src/dialogs/AdvancedSection.tsx`.
- A dialog step whose title says what happened announces it by moving focus to the dialog, wherever focus was, so the
  dialog is read out by its new name; it adds no `StatusAnnouncer`, which would say the same thing again. The import's
  result step is the instance: its title becomes "Import finished" and the step focuses the dialog itself
  (`packages/ui/src/configuration/ImportConfigurationDialog.tsx`). Whether VoiceOver reads the renamed dialog on this
  focus move is an open case in [VoiceOver verification cases](voiceover-cases.md).
- A status region inside a laid-out row (a grid or flex row) stays mounted while empty by being `sr-only`, never
  `display:none` or `hidden`, which takes it out of the accessibility tree and so loses the first text it receives. An
  account row's login announcement is the instance (`AccountLoginAnnouncer` in
  `packages/ui/src/settings/AccountLogin.tsx`, mounted by `AccountTableRow` in
  `packages/ui/src/settings/AccountsTable.tsx`): a `StatusAnnouncer` that lives as long as the row and says once that
  the account is logged in, while the row shows the same result visibly in its "Logged in" badge; it names the account,
  since nothing around it does. It follows the modal rule and the unattended-window rule above: it is written
  `AFTER_FOCUS_MOVE_DELAY_MS` after the success, a wait that outlasts the dialog closing and focus returning to the row,
  and only while the window is attended. A region written under the closing modal, or while the window is unattended, is
  not spoken; see the failing patterns in [VoiceOver verification cases](voiceover-cases.md).
- A dialog whose footer changes with its state puts focus on the state's main action each time the state changes. The
  login dialog is the instance (`packages/ui/src/settings/AccountLoginDialog.tsx`): focus is on Cancel login while the
  login runs and on Log in again once it ends or is refused. Its progress text is a `StatusAnnouncer` with
  `firstWriteDelayMs` set to `AFTER_FOCUS_MOVE_DELAY_MS`, mounted with the dialog, while the visible line is
  `aria-hidden`, so the text is spoken once and not twice. The rule is about a state the user waits on, not a footer
  button relabelled by what the user is typing: there focus stays in the field, and a `StatusAnnouncer` says what the
  new state is. The new-session dialog's Resume tab is the instance
  (`packages/ui/src/dialogs/ResumeConversationFields.tsx`): the lookup of the typed Session ID turns the submit into
  Open, Reopen, Resume or Go to session while focus stays in the ID input.
- HeroUI's `Alert` renders no role at all, its name notwithstanding. An error that can appear in one after the user has
  moved on puts `role="alert"` on an element of your own inside it.

### Reusing VoiceOver verification

Whether a new announcement needs a listening pass in the packaged macOS application is decided by [VoiceOver
verification cases](voiceover-cases.md): a caller that matches a passing case, and passes the automated and browser
checks listed there, needs none. The same document holds the recorded results, including the connection banner's
confirmation and the patterns known not to work.

## Contrast

### Thresholds

- Text: at least 4.5:1 against its background (3:1 for large text).
- An icon or a state indicator: at least 3:1 against what it sits on.
- A boundary: at least 3:1 too, when it alone shows where a control is.

### Do not change how a HeroUI component looks

Keep HeroUI's default appearance, including its text and placeholder colours, hover opacity and the default fill,
foreground and shadow of the variant or status chosen for a dialog or popover; the Alert's status background fill and
the other departures recorded under "Confirmed visual choices" (the dimming of a disabled menu item or select option,
and the hover and press feedback cancelled on a disabled menu item, among them) and a modal body's foreground text
colour (see "One definition per semantic colour"), are approved exceptions. A contrast measurement alone does not
justify changing its colours, theme tokens, borders, frames or outlines, so add no colour or opacity override to make a
default component pass the custom-style thresholds. Use the component's own supported variants and slots; layout and
sizing adaptations do not authorize replacing its visual treatment. Application-authored text on a custom surface is
checked against its actual background in both appearances.

The Terminal panel's tabs are an approved departure by the user's decision: compact rectangular tabs with modest
corner rounding, consistent spacing and always-visible close marks. The selected tab uses an accent-soft fill and
an accent outline, and hover uses the default fill. A close mark has a separate foreground-tinted background on
hover, deepened while pressed, so it stays distinct from the tab beneath it. These styles apply only to the Terminal
panel.

### Form controls on a surface

A form control on a surface — in a dialog, a popover, a card — takes HeroUI's `variant="secondary"` (`TextField`,
`Input`, `Select`, `Checkbox` alike), HeroUI's own variant for that case. The default variant leaves it blending into
the surface in both appearances.

### Clear buttons

The clear action inside a search field or an optional select uses the shared `FieldClearButton`: the application's
`Button` with `isIconOnly`, `size="xs"` and `variant="ghost"`, and a localized Clear tooltip and accessible name. It
stays transparent at rest. Since a filled field uses the standard ghost button's hover colour itself, the clear button
overrides only the button's hover and press fill tokens: the local foreground blended at 16% and 24% over the actual
field, respectively. This gives visible feedback on grey fields in both themes, including dialogs and popovers; the
glyph, shape and focus feedback stay standard. A select's clear button sits beside its trigger, never inside that
button. It keeps the text's trailing gap, returns focus to the trigger after clearing, and retains Backspace/Delete on
the trigger. Search fields keep their native clear action and return focus to the input.

### Hand-built rows and cards

A hand-built card that its content already identifies has no outline of its own: it is set apart by the short two-layer
panel shadow (`shadow-(--panel-shadow)`, as the focus-mode session cards are), with a transparent border holding the
place a selected state's border takes. That edge is decorative and is not held to 3:1; a panel card's hairline
(`panel-edge`) and a groove divider (`groove-b` and its other sides) are decorative in the same way. Its selected state
takes the accent icon colour (`--accent-soft-foreground`, see "One definition per semantic colour"), which reaches 3:1.

A card that sits on a dialog's own surface has no other surface to stand off from, and the panel shadow alone is
invisible on white (and on the dark dialog, whose shadow is black on near-black). The import dialog's cards, in its
review and its result alike (`CARD_CLASS` in `packages/ui/src/configuration/importParts.tsx`), are framed as a table
instead: the `panel-edge` hairline round the card and a groove between rows, with no shadow, and the rows on the
dialog's own background. A card's header row (`CARD_HEADER_CLASS`, the same file) takes the fill of the fields in the
rows (`--control-fill`, the dialog's floating fill), so the heading reads apart from the rows under it in both
appearances; no field sits in a header, so the shared fill hides nothing. That fill and the hairline are decorative and
not held to 3:1 (about 1.25:1 off the dialog in both appearances); the header's text and glyphs are held to their usual
ratios against it, the muted project count being the closest at about 4.6:1. These fills stay in the import dialog: the
dialog's surface and HeroUI's controls are not restyled for them.

An unarchived locked session communicates its state through a persistent warning-coloured lock button with an Unlock
accessible name and tooltip. Hovering or keyboard-focusing the button changes the glyph to an open lock; unlocking
hands keyboard focus to the row before the button disappears. Keyboard focus keeps its own visible ring.

Focus-mode session cards use the opaque `--panel` surface over the sidebar's `--panel-sidebar`, with the existing
hover and selected tints on top. Their titles use the shared single-line fading label, including its full-name
tooltip and hover marquee, rather than an ellipsis or a line clamp.

The Processes panel's process cards are hand-built on `--panel-row`: the surface (white) in light, and in dark
`oklch(0.29 0.006 286)`, one step above the panel's `oklch(0.27)` so the card still reads as raised, with the panel
shadow. In dark that leaves a `danger-soft` button's label (the row's Stop) short of 4.5:1 on the card: 4.47:1 at rest and
4.2:1 with Stop hovered (its fill then deepens to 20% of the danger colour from 15%). A card dark enough to reach 4.5:1
at rest (about `oklch(0.275)`) no longer stands apart from the panel, and HeroUI's own surface is darker than the panel.
Keeping the raised card is a kept shortfall by the user's decision (see "Conformance level and exceptions"), marked at
`--panel-row` in `packages/ui/src/style.css` and at the card in `ProcessesPanel.tsx`; the buttons themselves keep
HeroUI's look.

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

A usage bar — the compact one in front of an account option and in the accounts table, and the rail panel's and the
details' meters — carries its meaning in words, so its colours are decorative in the same way. The compact bar is hidden
from assistive technology, and the option's description or the cell's hidden text states the remaining usage; a meter
states it in its own accessible name and value, while its drawn headline is hidden. Its track is `--default`, which
inside `.control-fills` (Settings, the dialogs, the rail panel) is `--control-fill`; in dark the compact bar's
`.quota-bar-track` is a step lighter than its `--default`, so it can be seen on a dark popover (1.86:1 off it). The
track is not held to 3:1 against its surface. The fill's agent colour is kept at 3:1 or better against every track it is
drawn on at full opacity, in both appearances, though nothing requires it: a bar's state never rests on its colour
alone. The grey of a used-up account is not: it is meant to recede, a step fainter than the agent colours, and its badge
says why.

### Row action menu buttons

File, Git, project and session rows, focus cards and pane headers use `ActionMenu`'s shared trigger for their ellipsis
menu. Keep its 24-by-24-pixel target, 16-pixel glyph, rounded corners and focus ring consistent. A caller changes the
trigger's layout or content only when its placement needs it; it does not choose separate colours for the same ellipsis
action.

- At rest, the trigger is transparent with a muted glyph. A row's selection reveals its actions without painting each
  button as active.
- While hovered or while its menu is open, the trigger uses `bg-default` and a foreground glyph. Its wrapper supplies
  the shared `sidebar-fills` family so Files, Git and sidebar menus use the same translucent hover tint, independent of
  the surrounding pane's control fills. Keep this subtle tint; do not replace it with a solid muted fill or an inverted
  glyph.
- The glyph must reach 3:1 against its actual background in every state. The hover tint does not need 3:1 against the
  row: the glyph identifies the control, rather than a filled boundary.
- Keyboard focus keeps the existing focus ring. Revealing an action must not leave it dimmed once its visibility
  transition finishes.
- Closing a menu that was opened from the focused trigger by the keyboard returns focus to that same trigger while its
  row survives. A menu opened with the Menu key, Shift+F10 or a right-click goes through the context-menu path and
  returns focus to where it stood when the menu opened instead: the row, for a key pressed on the row; the Actions
  button, for one pressed while the button had focus.

Measure both appearances on the rendered resting, hovered and selected surfaces. Read the final computed colours after
transitions settle, composite translucent ancestor fills, and check the glyph separately from the button fill. The row's
own hover tint is not the button's hover feedback.

### Where to fix a shortfall

- Fix a shortfall in application-authored styling at its custom part or shared application token.
- When an application-authored surface causes a shortfall, adjust that surface rather than overriding the HeroUI
  component's default appearance.
- Check an application-authored surface with the text and controls actually rendered on it, at rest and while hovered
  or selected, compositing translucent fills over their actual backgrounds, and prefer a surface that works with
  HeroUI's default controls.
- Do not treat an existing component override as permission to add another one.

### Finding every rule that draws a colour

A custom token reaches only the rules that read it. Inspect the rendered element and computed styles to find which
rule supplies a colour before changing application-authored styling. HeroUI's compiled stylesheet
(`packages/ui/node_modules/@heroui/styles/dist/heroui.min.css`) is the reference for its defaults, including where an
input inherits a placeholder colour from preflight rather than a component-specific rule. That inheritance does not
make the default placeholder an application-authored style.

### Auditing a change to a colour token or a surface

A change to an application-authored colour token or surface can touch every screen, so its audit covers every gallery
scenario it affects in both appearances. HeroUI's default visual treatment is not redefined by that audit. It also
measures what a pass over text and glyphs misses:

- Every fill — a chip, a tertiary button, a ghost button's hover — against the surface it sits on.
- An animated state at its faintest frame. A glyph is animated without lowering its opacity (an `animate-pulse` glyph
  sinks to about 2:1).
- A translucent fill (HeroUI's soft chip is 15% of its colour) as drawn: composite it over each surface it actually
  lands on — a selected and a hovered row, a dialog's `--overlay` — and measure the label against that; measuring
  against the bare surface or the fill's own colour overstates the contrast.

A ratio read off a capture of the packaged window carries the display's own colour profile, so it says which side of a
threshold a pair lands on and nothing finer: the selected toggle's fill reads 3.26:1 light and 5.0:1 dark against its
siblings where the tokens give 3.67:1 and 4.58:1. Take a recorded ratio from the tokens, never from a capture.

### A glyph or tint on the window material

In the macOS app the top bar and the rail show a translucent window material whose colour shifts with whatever is behind
the window (see "The window chrome and the content panel" in `docs/product/window-layout.md`). A glyph or tint added to
that chrome is measured against the material's measured colours, given in the comment on the window's surfaces in
`packages/ui/src/style.css`, not only against `--chrome`, which is just the browser's stand-in for the material. The
dark material is a grey far lighter than dark `--chrome`.

## Pointer target size

A control's pointer target is at least 24 by 24 CSS pixels (WCAG 2.2 AA 2.5.8). HeroUI's own sizes already clear it —
its smallest icon-only button is 32. The application uses `components/Button.tsx` for every Button, extending
HeroUI with `size="xs"`: 24px height, a 24px square for icon-only buttons, 12px text and 14px icons.
Compact actions use that size rather than per-call sizing styles.

Icon buttons in sidebar rows, cards and headers, the right pane and the file viewer share a 24-by-24-pixel target and a
4-pixel gap, defined by `--row-control-size` and `--row-control-gap` in `packages/ui/src/style.css`. Use the shared
`RowIconGroup` for adjacent icons and `RowIconSlot` for a marker's matching horizontal column; the marker keeps its own
glyph size and does not increase the text line's height. Leading glyphs use the 16-pixel `--row-leading-icon-size`
column through `RowIconSlot`'s leading placement. In a focus card, the binding dot precedes the title in reading order
and shares its horizontal centre with the status and agent glyphs. All three lines use `RowIconLine` with the same
8-pixel `--row-leading-icon-gap`, so their text starts in one column; an unbound title keeps the empty leading slot.
`RowControls` keeps persistent pins and markers outside the hidden actions' clip and collapses the actions' leading gap
with them. Do not space individual trailing icons with the text row's gap or add local offsets. Verify the shared
columns with actions hidden, hovered, focused and open, including right-to-left layout.

WCAG 2.2 SC 2.5.8 also excepts an **Inline** target: one "in a sentence, or its size is otherwise constrained by the
line-height of non-target text". A status mark standing in running text (the inline form in "How a mark is drawn and
opened") is such a target: it is drawn at the text's size and cannot be larger without growing its line.

The criterion is about controls, so a mark that only opens a tooltip with a mouse is not held to it: a tooltip is not an
action, and a mark too small to aim at reliably costs a hover that can be tried again, not an activation. Several such
marks are drawn well under 24 — the binding badge on a session row, a branch's responsible-session badge, a change row's
status letter — and the press under them, where there is one, belongs to the row and keeps the row's own target.

A mark whose tap opens a touch popover (`IndicatorTooltip`, `touchPopover` on) is an activation on touch, so it is held
to it there: `.touch-target` in `style.css` grows its hit area to 24 by 24 on a device that has a coarse pointer
(`any-pointer: coarse`, which a hybrid device's mouse falls under too) with an `::after` that never shrinks a larger
mark, changes nothing drawn and sits behind the mark's own content so it takes no hover from it. Where two such marks
sit closer than 24 pixels, their grown areas overlap and the later one wins the overlap. The marks that keep the row's
own target instead have the popover off.

Neither exception in the criterion is easy to rely on here. *Spacing* needs a 24-pixel circle centred on the target to
touch no other target, which a small control placed over a larger one (a delete badge on a corner) fails by
construction. *Equivalent* needs the same action to be available from another control on the same view, which a lone
affordance does not have. The Files and Git tree chevrons are the one case that relies on *Equivalent*: a press
anywhere on a folder or commit row expands or folds it, as the chevron does, so the 16-pixel chevron takes no 24-pixel
hit box.

A Terminal panel tab's close mark is drawn at 16 and hit at 24 through such an `::after`. It is a pointer-only
shortcut, hidden from assistive technology, with its tooltip anchored beside it since a tab cannot hold a trigger; the
keyboard reaches the same action as Close terminal in the panel's tab-row menu and the tab's context menu.

The right pane's resize handles (the width handle and the separator between stacked panel cards) take a 24-pixel
pointer target centred on their gap, reaching into the cards' edges where the gap is narrower; the sidebar's width
handle stays narrower, as a wider one would take the sidebar's scrollbar and its rows' trailing buttons.

Where the drawn size has to stay under 24, extend the target rather than the drawing: an absolutely positioned `::after`
with a negative inset enlarges what the pointer hits and changes nothing visible. The pressable part of a checkbox whose
label is one line of `text-sm` (`TrustDirCheckbox` in `packages/ui/src/dialogs/TrustPromptDialog.tsx`) extends its
target this way because HeroUI lays it out at the line's own height.

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
resting ones, and `aria-pressed` carries the state for assistive technology. The window chrome's pane toggles (the
sidebar's among them), the Git mode's two view toggles — the one that shows and hides the change filter field, and
group by folder — the Files mode's filter toggle and include-ignored toggle, and the viewer's three view controls
are built this way.

A two-state control that is also what shows and hides a region carries `aria-expanded` beside its `aria-pressed`, the
two saying the same thing in the two vocabularies: the Git mode's filter toggle is pressed while its field is on
screen, and expanded for the same reason (`expanded` on `IconToggle`). The region it shows has no id of its own and so
no `aria-controls`; "Expanded and selected" above asks only for `aria-expanded`. A region taken away this way hands
keyboard focus back to the toggle when it was holding it, the toggle being pressed without taking focus itself; the
hand-off's ring follows how the press came, as "Focus moved by a change the user did not make" above has it.

The right pane's panel kind toggles are the exception: each keeps one icon, and its open state is a filled chip behind
the icon (the rail's current-console fill), a shape rather than a tint, with `aria-expanded` for assistive technology. A
grip on a resize handle is a hover cue like a hover tint: light while hovered or focused, below 3:1, with the focus ring
carrying keyboard focus, and the strong colour, at 3:1, only while pressed. Its "Resize" tooltip opens on pointer hover
only and is not tied to the separator, whose own accessible name already says what it does; a focused grip shows itself
and its ring instead.

**A notice carries its severity in a glyph, never in its fill alone.** The file viewer's notices — what is said about
content that is still shown — are status marks (`StatusMark`) in three tones (`packages/ui/src/viewer/notices.tsx`): an
explanation of how the content is drawn (a note, the circled `i`), a warning about the content itself (the triangle),
and a part of it the viewer could not show (danger, the circled `!`). A read that fails while what the viewer shows is
still true is a warning, not a failure: a change with no differences whose content could not be read for display keeps
"No differences" and carries the triangle, since only the preview of its content is lost. The glyphs differ in shape, so
the tone does not rest on colour. The viewer is a compact layout and holds no Alert: a notice about the whole file or
change is one mark after the file path in the viewer's header, ordered failed, warned, then notes; in a compared image
change a side's own notice stands beside that side's heading instead; a notice about one spot of a Markdown document
stays at that spot, inside a paragraph (a button is valid there) or before a block's fallback source, lined up with its
first line. Each mark is named by its sentence and opens it, as "How a mark is drawn and opened" says. A subject that
cannot be opened at all fills the body with the danger empty-state (`Failure` in
`packages/ui/src/viewer/viewerStates.tsx`) and no mark, a conflicted file whose read failed included. A conflict is said
by the title's status chip, not by a mark: what kind of conflict it is is the chip's tooltip, which a tap opens too, and
part of the heading's name, built from the catalog with the rest of it, since the drawn chip is hidden from assistive
technology. A viewer with nothing to draw takes neither: it fills the body with a centred empty state
(`CenteredEmptyPanel` in `packages/ui/src/components/EmptyPanel.tsx`).

**One shortfall is kept, and it is a single control**: the settings dialog's three-way appearance choice, which
is a toggle group where the selected button differs by fill colour only. Which button is selected is also carried by
its position and by `aria-pressed`, and the fill difference meets 1.4.11's 3:1 for a state. The reasoning and the
measured ratios sit in the comment above `.toggle-button-group .toggle-button` in `packages/ui/src/style.css`; a
two-state choice does not belong in that group, because the icon form above has no shortfall to accept.

## Text direction

Under a right-to-left language the window mirrors, and what mirrors with it is settled per part in
`docs/product/right-to-left-layout.md`. What follows is the rule behind that list and how to meet it in code; it is
WCAG 2.2 A 1.3.2, since a value read in the wrong order says something else.

**Prose follows the interface; a value keeps its own order.** A sentence takes the document's direction. A value that is
not prose — a path, a URL, an email address, a branch name, a commit id, a shell command, a port, a keyboard shortcut, a
count written as `1 / 12`, a literal to type, a diagram, a formula — reads left to right in every language, and the
element that *draws* it says so with `dir="ltr"`. `TextFieldDescription` puts only the count/limit ratio inside an LTR
`bdi`, leaving the translated unit outside it.

An inline formula sits inside a paragraph, so that element is also a bidi isolate: `dir` on it is not always enough
once a utility class resets `unicode-bidi`, and Tailwind's `isolate` does not do this job (`isolation: isolate` is a
stacking context). The formula sets `unicode-bidi: isolate` itself (`FormulaView`).

- Say it on the element that draws the value, not on one inside it. A field whose group holds an affix needs the
  `dir` on the group: with it on the input alone, the affix stays on the mirrored side and the text pulls away from it,
  and the two stop reading as one control (`packages/ui/src/settings/CloneDirSetting.tsx`).
- `dir` sets direction, not alignment. It does not override an inherited `text-align`, so an element that needs both
  keeps its alignment class as well. The default `text-align: start`, though, resolves against the element's own
  direction, so a value drawn on a line of its own with `dir="ltr"` inside right-to-left UI lines up on the opposite
  edge from its siblings; it takes `align-match-parent` (in `packages/ui/src/style.css`) to line up with its parent,
  which `FadeOverflow` adds whenever it is given a `dir`.
- A tooltip is portalled out of the element it describes, so it inherits the window's direction, not the value's. A
  tooltip that is the value itself in full — a clipped path's or branch name's, a field's expanded path, a terminal
  link's URI — says `dir="ltr"` again on the value inside it, and on a block of its own (`ValueTooltipText`): as a run
  inside the tooltip's right-to-left paragraph, a value long enough to wrap starts its second line at the right, so it
  no longer reads as one left-to-right value. As a block every line starts at the left. `FadeOverflow` does this for a
  plain-string tooltip on an `ltr` label, which is why a sentence goes in as a node there; a sentence tooltip keeps
  the window's direction and alignment. The same block carries the label's font family (see "A value tooltip keeps
  its label's font family" under "A tooltip is the application's own, never the browser's `title`").

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

`packages/ui/src/browser/BranchSelect.tsx` keeps the branch name plain for `textValue` and lays it out LTR on the trigger and in every option alike.
Gone status is a separate badge; the clipped tooltip uses a technical chip in its sentence.
`packages/ui/src/browser/GitView.tsx` likewise uses a technical chip in the visible comparing notice, while its
plain announcement isolates the branch name where it enters the sentence.

**An icon mirrors only when its meaning is a direction.** Back and Forward, a disclosure or submenu chevron, a pane
toggle, a glyph whose rows hang off the leading side (the fold control, the list and folder-tree layout toggles):
`rtl:-scale-x-100` on the icon; an `ActionMenu` entry's icon, a submenu's included, takes it through the entry's
`iconClassName`. A glyph standing for a physical thing — pause, a rotation, a camera — and a checkmark
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

An animation or transition that is not essential stops under `prefers-reduced-motion: reduce` (Tailwind's `motion-safe:`
/ `motion-reduce:` variants). Motion driven from script reads `prefersReducedMotion()` before it starts and subscribes
with `onReducedMotion()` (`packages/ui/src/reducedMotion.ts`) to cut a running effect short, leaving it at its end
state, when the preference turns on. Overlays that react-aria opens and closes (menus, popovers, tooltips, select and
combo box lists, dialogs, toasts) appear and go at once: one rule in `packages/ui/src/style.css` stops the enter and
exit animation of every element marked `data-entering` / `data-exiting`, which HeroUI leaves running for its
popover-level overlays, so a new overlay needs no guard of its own. The few stock transitions HeroUI leaves unguarded (a
select's chevron turning as its list opens, a combo box trigger's hover and press fade, and a checkbox's checkmark
drawing itself in) are guarded there too, and `packages/ui/src/style.test.ts` fails on any other an imported HeroUI
sheet brings.

No transition starts inside content folded with `hidden="until-found"` (a disclosure's closed panel): a transition
started there never reports its end, and react-aria holds every focus move that follows a press without a pointer
until no transition is running, so focus would stay on the page. One rule in `packages/ui/src/style.css` turns them off;
the content is not rendered while folded, so nothing visible changes. The rule cannot stop a transition still running
as the content folds, whose end Chrome does not report until the content is shown again, so a dialog that must take
focus at once places it itself rather than leaving it to react-aria's deferred focus (`ConfirmDialog`, and a caller of
`focusesItselfOnOpen`).

Below the `phone` breakpoint a pane slides in from the side it lives on as it fades — the sidebar's drawer, with its
rail, from the start edge, a floating right-pane card from the end edge — mirrored under a right-to-left language.
Under `prefers-reduced-motion: reduce` neither slides: the drawer's and the rail's transitions are off and the card's
own reduced-motion rule applies, so each simply appears and goes.

### Dynamic collections

When items are added, removed, reordered or folded within a visible collection, or an item in a row as wide as its
items (a tab strip) grows or shrinks on its own, surviving items that change position slide into place using the
shared list-motion implementation. This applies to navigation lists, settings records, tab strips, selected tags,
wrapping chips and virtualized file and change collections. Use stable item identities, not array indices or
translated labels. Horizontal and wrapping collections move on both axes, including under RTL.

Use a 300 ms ease with `cubic-bezier(0.2, 0, 0, 1)` for position changes. In nonvirtualized collections, new items
fade in from the left while removed items fade out to the right as inert, accessibility-hidden visual copies.
These effects move by 25% of each item's own width over 300 ms with `cubic-bezier(0.4, 0, 0.2, 1)`, starting alongside
the surviving items' movement. Removed items stop accepting input immediately; do not delay an action or keep a
removed control reachable to play an exit animation. Preserve keyboard focus on surviving controls and retain the
surface's focus handoff when the focused item is removed: a tab removed while it or its context menu holds focus
hands focus to the strip's shown tab, or into the panel when no tab is left, with the ring shown, and so does one
closed through a confirmation its context menu asked for, once the confirmation has gone.

Initial rendering, scrolling, viewport recycling and container resizing do not trigger item movement animations, and
neither does every item of a row changing width at once (the text zoomed). Virtualized collections keep virtualization
and animate only surviving rendered identities whose layout changes. In a tree, rows that appear or vanish because their
parent expands or folds move with the fold and play no entry or exit; only rows added to or removed from a parent that
stays expanded do. Pass the tree's parents and expanded rows to the list motion so it can tell the two apart. Static
choices, a replacement page or directory listing, and fixed result snapshots do not require collection motion.
Collections whose component already provides the same behavior, such as the toast stack, keep that behavior. With
reduced motion requested, update positions immediately, cancel any entry or movement already running and remove exit
copies.

Use `useListMotion` from `packages/ui/src/components/useListMotion.ts`, marking items with `data-list-motion`. Its
descendant mode supports semantic wrappers and `display: contents`; its virtualized mode follows rendered identities
without replacing the collection's layout transforms or focus management. Browser collections use `useBrowserListMotion`
to preserve their existing scroll and focus refs. Keep animation timing and preference handling in the shared
implementation rather than writing a separate transition at each caller. A content-width, start-aligned tab strip passes
`contentWidth`: its own width changes with its tabs, which would otherwise read as a resize and animate nothing, and
positions are measured from its reading-start edge. A caller that moves items with inline transforms during a drag calls
`releaseListMotion(item)` as it lets go, before clearing the transforms, so the dropped item slides on from where it is
drawn instead of moving twice. A divider that is hidden after the last item uses
`[&:not(:has(~[data-list-motion]))]:border-b-0`, not `last:`, because exit copies are appended after the last real item;
a row whose following siblings may carry no motion key (such as a shared row component used both with and without one)
uses `[&:not(:has(~:not([data-list-motion-exit])))]:border-b-0` instead, which skips only the exit copies. Where the
content below a collection would visibly jump, or be drawn over by departing copies, as items come and go, the
collection passes `fitHeight` to `useListMotion` (a virtualized or content-width collection cannot): its height then
follows the change with the position-change easing, clipped at its top and bottom edges, so the content below glides
with it. Such a collection is a positioned (`relative`) container, so that its clip reaches the absolutely placed exit
copies, and sizes by its border box (`box-sizing: border-box`, as Tailwind's base styles give). For a lazily mounted
choice popup, use `MotionListBox` inside the popup so the observer mounts with the list and captures its initial
positions before the first live choice update. Apply the same motion rule to choices that refresh while the popup
remains open; a static choice list simply has no movement to animate.
