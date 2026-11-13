# Keyboard shortcuts

Octoboard's own commands have keyboard shortcuts the user can change, and Settings' **Keyboard shortcuts** section lists
them together with the keys that belong to the window, the system, the file viewer or Settings, which cannot be
changed. A changed shortcut belongs to the client it was changed in — the desktop app, or one browser — and nowhere else
(see "Where the shortcuts are kept" below).

What each command does, and when it does nothing, is in the doc of the command itself; this doc covers the keys: which
there are, how they are changed and how a key press is matched against them.

## Menu keyboard shortcuts

Session, project and console action menus show fixed single-letter shortcuts on their top-level actions, apart from
the selected session's Focus mode item described below. While a menu is open, a letter takes that action immediately,
on the session, project or console the menu belongs to, regardless of the selected session. The mappings are in "Session menu keyboard shortcuts" in
`docs/product/sidebar-session-rows.md`, "Project menu keyboard shortcuts" in `docs/product/sidebar-project-rows.md`
and "The console header" in `docs/product/sidebar.md`.

Only items that perform an action have these keys. Entries that open another menu or a custom panel, such as Colour,
Switch account, Open in and View archive, have no letter or key cap; they keep their usual pointer and keyboard
navigation. Choices inside those submenus have no temporary letter shortcuts either. Keys pressed in a submenu or its
panel never activate a parent item.

An action shows its letter at its trailing edge, except when a disabled item's reason takes that space. The letters
are fixed across languages, cannot be changed and are not listed in Settings. Only actions that are present can be
taken; a disabled action does nothing from its letter, as from a press. Each action keeps its normal confirmation and
protection rules.

The letter is matched against the character typed, with no modifier held. Holding a letter does not repeat its
action, and input-method composition does not activate an item. Once the menu closes its letters have no effect.
Arrow keys, Enter, Space and Escape keep the menu's usual navigation and activation behavior.

The selected session's Focus mode item shows and takes the current configurable Toggle focus mode combination,
including its modifiers, instead of a fixed letter. It is offered only outside focus mode on a selected session that
is not archived and has an available context. Its availability and target follow "The focus mode shortcut" in
`docs/product/focus-mode.md`. That combination works in the menu's main list; another menu, a submenu or a dialog
suppresses it. It does not repeat while held. The project's Focus mode item keeps its fixed `F`.

## How a combination is written and matched

**A command's shortcut is one key with a set of modifiers, and it is matched exactly**: the key, and those modifiers
with none more and none fewer. The key is the physical key, by its position on the keyboard, so a keyboard layout does
not move a shortcut and a held Shift does not change which key it is. A key cap is labelled with the key's name on a US
layout: `F` for the F key, `,` for the comma key.

The modifiers are written the same on every platform:

| Modifier | On an Apple keyboard | Elsewhere |
|---|---|---|
| The primary modifier | `Cmd` (⌘) | `Ctrl` |
| Shift | `Shift` (⇧) | `Shift` |
| Alt | `Option` (⌥) | `Alt` |
| The other modifier | `Ctrl` (⌃) | the Windows key |
| Control | `Ctrl` (⌃) | `Ctrl` |

So a default means the same thing on every platform, and a shortcut carried from one machine to another in a
configuration file (see "The shortcuts as data" below) is the same combination there. **Control** is the one exception:
it is the Control key on every keyboard, so on an Apple keyboard it is the other modifier and elsewhere the primary
one, and a combination written with it is compared, matched and shown as that modifier. The console-session keys'
defaults are written with it (see "What is listed" below); a recorded shortcut never is. Wherever a shortcut is shown
its key caps read in the order the platform writes a combination in, then the key, and keep that order under a
right-to-left language (see `docs/product/right-to-left-layout.md`):

- **On an Apple keyboard**, the other modifier, Alt, Shift, the primary modifier, as macOS's symbols: ⌃ ⌥ ⇧ ⌘.
- **Elsewhere**, Windows' order — the Windows key, Ctrl, Alt, Shift — as the keys' names: `Win`, `Ctrl`, `Alt`,
  `Shift`. Home, End, Page Up and Page Down read `Home`, `End`, `PgUp` and `PgDn` there, rather than macOS's symbols
  for them; the other keys' caps, such as Enter's and the arrows', are the same on every platform.

Where the Keyboard shortcuts section tells assistive technology a combination, it says it in words in the same order,
the keys named in the UI's language: "Shift+Command+F" on an Apple keyboard, "Ctrl+Shift+F" elsewhere, where the
Windows key is said as "Windows".

## What is listed

The section lists the window, system and file-viewer shortcuts in five groups, in this order. Within a group the
commands that can be changed come first. The defaults are written for an Apple keyboard; elsewhere each modifier is the
one the table above maps it to, except the console-session keys', which are Control with Tab on every keyboard: `⌃Tab`
and `⌃⇧Tab` on an Apple keyboard, `Ctrl+Tab` and `Ctrl+Shift+Tab` elsewhere.

| Group | Command | Default | Can be changed | Listed in |
|---|---|---|---|---|
| Navigation | **Back** | `Cmd+[` | yes | the desktop app on macOS |
| Navigation | **Forward** | `Cmd+]` | yes | the desktop app on macOS |
| Navigation | **Next console session** | `Ctrl+Tab` | yes | the desktop app on any system |
| Navigation | **Previous console session** | `Ctrl+Shift+Tab` | yes | the desktop app on any system |
| Navigation | **Next region of the window** | `F6` | no | every client |
| Navigation | **Previous region of the window** | `Shift+F6` | no | every client |
| Focus mode | **Toggle focus mode** | `Shift+Cmd+F` | yes | every client |
| Focus mode | **New session** | `Cmd+N` | yes | every client |
| Focus mode | **New session with defaults** | `Shift+Cmd+N` | yes | every client |
| Application | **Open settings** | `Cmd+,` | yes | every client (in the macOS desktop app its native menu takes the key; wherever there is no native menu — the Windows desktop app and a browser — the page does) |
| Editing | **Undo**, **Redo**, **Cut**, **Copy**, **Paste**, **Select all** | `Cmd+Z`, `Shift+Cmd+Z`, `Cmd+X`, `Cmd+C`, `Cmd+V`, `Cmd+A` | no | the desktop app (its native menu) |
| Editing | **Find** | `Cmd+F` | no | every client |
| Window | **Hide Octoboard**, **Hide others**, **Toggle full screen**, **Minimize**, **Close window**, **Quit Octoboard** | `Cmd+H`, `Option+Cmd+H`, `Ctrl+Cmd+F`, `Cmd+M`, `Cmd+W`, `Cmd+Q` | no | the desktop app (its native menu) |

**A command the client does not have is not listed**: in a browser, Back and Forward, the console-session keys and the
native menu's keys are left out, and so is the Window group, which holds nothing else; the Application group holds only
Open settings and the Editing group only Find there. The Windows desktop app, which has no native menu, leaves
out the same commands but the console-session keys, which it lists. Undo, Redo, Cut, Copy, Paste and Select all, and
the Window group's keys, are the macOS menu's: elsewhere the engine or the system keeps them (and Ctrl+C interrupts the terminal's program), so the
page does not take them. Such a command keeps its key all the same, and that key cannot
be given to another command there either (see "Which combinations are refused" below).

**The released Windows app has none of the web engine's own browser keys.** Its window is drawn by WebView2, whose
keys for reloading the page (`F5`, `Ctrl+R`), printing it (`Ctrl+P`), finding in the page (`Ctrl+F`, `F3`) and zooming
it from the keyboard are turned off there, so none of them reloads, prints, opens the engine's find bar or zooms; a key
that is one of Octoboard's own, such as Find's `Ctrl+F`, reaches the page as usual. Editing keys in a text
field — select all, copy, paste and the like — are not browser keys and keep working.

What each command does: Back and Forward in "Keyboard shortcuts" in `docs/product/navigation-history.md`; the
console-session keys in "The switch strip" in `docs/product/focus-mode.md`; the region keys in
`docs/product/moving-focus-between-regions.md`; Toggle focus mode in "The focus mode shortcut" and the two New session
commands in "Creating sessions from the keyboard", both in `docs/product/focus-mode.md`; Open settings in "Opening
and closing Settings" in `docs/product/settings.md`; Find, which acts only in the file viewer, where it finds in the
file (see `docs/product/finding-in-the-file-viewer.md`), and in Settings, where it focuses the search field (see "The
search field" in `docs/product/searching-settings.md`); and the native menu's keys in `docs/product/application-lifecycle.md`.

## The Keyboard shortcuts section

The section sits after Notifications and before Configuration in Settings (see `docs/product/settings.md`). A paragraph
under its heading says that the app's own commands can be changed, that the keys of the system, the window and the file
viewer are listed too and cannot be, and that the changes apply only to the current desktop app or, when viewed in a
browser, the current browser. Beside it, only while at least one shortcut differs from its default, is a **Restore
defaults** button.

Each group has a heading, and each command a row: its name, then its current key caps, and then a slot of fixed width
at the row's end, so that the key caps of every row line up. That slot holds:

- a **Cancel** icon button (an X icon) while the row is recording (see "Changing a shortcut" below);
- a **Reset** icon button (a rotate-arrow icon) while the command's shortcut differs from its default and the row is not
  recording;
- a **lock** icon for a key that cannot be changed, whose tooltip and name say "Fixed" — the word itself is not shown;
- nothing otherwise.

A command that can be changed has its key caps as a button: the caps draw no button fill of their own, and darken
under the pointer and while pressed. Its name says it changes the shortcut for that command, its tooltip says the same,
and its description is the current combination. A key that cannot be changed shows its key caps only, as text.

For the tooltips of Cancel, of Reset and of the lock see "Tooltips on icon-only controls" and "Tooltips on indicators" in
`docs/product/labels-and-tooltips.md`.

A group's heading stays pinned at the top of the section's area while the group's rows scroll beneath it, as in
"Group headings" in `docs/product/settings.md`.

### Changing a shortcut

Pressing a row's key caps — with the pointer, or with Enter or Space while they have keyboard focus — starts recording
on that row: the key caps button gives way, in its place, to a prompt asking for the new keys, drawn as a single key cap
in the accent colour (the accent's soft fill). The prompt is not a control: pressing it does nothing, and it does not
change under the pointer. The row's Reset goes away until the recording ends, and a **Cancel** icon button takes its
place in the row's trailing slot; keyboard focus moves to Cancel as the recording starts, whether it was started with
the pointer or with the keyboard. Cancel's tooltip is a short "Cancel", its name says that it cancels changing the
shortcut for the command, and its description is the prompt. Only Cancel — pressed with the pointer, or with Enter or
Space while it has keyboard focus — and Escape (see below) cancel the recording. When a recording ends with keyboard
focus on Cancel, or on nothing, focus moves to the row's key caps button; when Tab or another control has taken focus
meanwhile, it stays there. One row records at a time; starting another row's recording ends the first.

While a row records, every key press goes to the recording and to nothing else: no window shortcut runs, Escape does not
close Settings and the terminal receives nothing. A modifier pressed on its own is the start of a combination and is
waited past; a key held down counts once; a key pressed during an input method's composition is left alone. The first
other combination pressed is then taken:

- **One that can be used is saved at once**: the recording ends and the row shows the new keys. Choosing a command's
  default is the same as resetting it.
- **One that cannot be used is not saved**: the row says why under the command's name (see "Which combinations are
  refused" below) and goes on recording, so another combination can be tried straight away.

A few keys are not recorded:

- **Escape**, with no modifier, cancels the recording, as pressing the recording row's Cancel does.
- **Tab** held with neither `Ctrl` nor `Cmd` (the Windows key elsewhere) — so `Shift+Tab` and `Option+Tab` too — ends
  the recording and moves keyboard focus as Tab always does. Tab with either of them is recorded, as the
  console-session keys use `Ctrl+Tab`.
- **Enter** and **Space**, with no modifier, are left to the focused button, so on the recording row's Cancel they
  cancel.

Ending a recording in any of these ways changes nothing and clears the row's reason.

### Resetting and restoring the defaults

**Reset** gives the row's command its default back, and keyboard focus moves to that row's key caps button. When another
command has taken the default meanwhile, the reset is refused: the row says which command holds it, the changed shortcut
stays, and focus stays on Reset.

**Restore defaults** gives every command its default back at once, ends any recording, and moves keyboard focus to the
list's first key caps button as the Restore defaults button itself goes away — the first one a search in Settings
leaves shown, or the section's tab while the search shows none (see `docs/product/searching-settings.md`).

### What assistive technology is told

The section has one status region of its own, which says that a recording has started and that Escape cancels it, the
new keys once one is saved, that a change was cancelled, the keys a reset or Restore defaults put back, and every
reason a combination or a reset was refused — again when the same refusal repeats. A reason shown on a row is also part
of the description of that row's key caps button, after the current combination, of its Cancel, after the prompt, and
of its Reset.

## Which combinations are refused

The same rules decide what a recording accepts, what a reset may put back and what an imported shortcut may be (see
"The shortcuts as data" below). In the order they are checked:

- **A key that cannot be in a shortcut**: anything but a letter, a digit, `F1` to `F24`, the punctuation keys
  `` - = [ ] ; ' ` \ , . / ``, Tab, Enter, Space, Backspace, Delete, Insert, Home, End, Page Up, Page Down and the arrow
  keys. The numeric keypad's keys, the keys of non-US layouts that a US keyboard does not have, and the media keys are
  all refused for this.
- **Escape, F6 and the context-menu key**, with any modifiers, which keep their own meaning.
- **No primary modifier.** A shortcut needs ⌘ (Ctrl elsewhere), so that it cannot clash with typing. On an Apple
  keyboard a combination of ⌃ without ⌘ is refused as one of the terminal's control keys; elsewhere a combination of the
  Windows key without Ctrl is refused as the system's. The one exception is Tab with ⌃ on an Apple keyboard, with any
  mix of ⌥ and ⇧, which the console-session keys use.
- **An editing key with the primary modifier**: the arrow keys, Home, End, Page Up, Page Down, Backspace, Delete and
  Enter, which with ⌘ (Ctrl) move the caret or edit text in every text field and in the terminal.
- **A combination the system keeps for itself**: on an Apple keyboard `Cmd+Space` and ``Cmd+` ``, with any modifiers
  but ⌃, and ⌘ with Tab, with any other modifiers; elsewhere Tab with Ctrl and Alt or the Windows key, such as
  `Ctrl+Alt+Tab`. **Tab is accepted only with Control**: on an Apple keyboard ⌃Tab with any mix of ⌥ and ⇧, elsewhere
  only `Ctrl+Tab` and `Ctrl+Shift+Tab` (`Win+Tab`, with no Ctrl, is refused by the rule above).
- **`Ctrl+C` on its own, off an Apple keyboard**, which interrupts the program in the terminal. (On an Apple keyboard
  `Cmd+C` is Copy, refused as a conflict below.)
- **Shift with a digit or a symbol key, for Open settings.** The native menu matches its keys by the character a key
  types with Shift applied, so such a shortcut would never fire. It is refused on every client, since a shortcut can be
  carried to the desktop app in a configuration file.
- **A combination another command has**: one of the fixed keys, or the current shortcut of another command that can be
  changed — the same keys on this platform, however each is written. The reason names the command; when that command
  is not listed on this client — the desktop app's own, in a browser — it adds that the command is the desktop app's.

## Where the shortcuts show

Where a configurable shortcut is shown it is the command's current one, and it changes as soon as the shortcut does:

- the selected session menu's Focus mode item and the Leave focus mode button's tooltip;
- the tooltips that name a shortcut: the **+** that the New session commands act on, which shows the shortcut of
  whichever of the two it would take (see "Creating sessions from the keyboard" in `docs/product/focus-mode.md`);
- the macOS desktop app's native menu, whose **Settings…** item carries Open settings' shortcut.

## Where the shortcuts are kept

**Changed shortcuts are kept per client**, in that client's own browser storage, as the appearance choice is (see
"Where the choice is stored" in `docs/product/appearance.md`). Only the shortcuts that differ from their defaults are
kept; a client with none changed, or whose storage has been cleared, has every default. The daemon does not store them,
so a desktop window and a browser tab, or two browsers reaching the same daemon, each have their own, and no agent can
read or change them (see "Who reads and changes what" in `docs/product/agent-settings-tools.md`). A kept shortcut that
cannot stand when it is read — one these rules refuse, or one for a command that no longer exists — is dropped, and
its command has its default.

A configuration export that includes the Keyboard shortcuts section carries the exporting client's changed shortcuts,
and an import can apply a file's to the importing client (see "The application settings an export carries" and "The
keyboard shortcuts on import" in `docs/product/settings-in-a-configuration.md`).

## The shortcuts as data

The changed shortcuts, as kept and as carried in a configuration document, are one JSON object: a key for each changed
command, by its id, whose value is the combination — the key's code (the `KeyboardEvent.code` of the physical key) and
each modifier that is held, as `true`, among `primary`, `shift`, `alt`, `other` and `ctrl` (Control; see "How a
combination is written and matched" above). A modifier that is absent or `false` is not held. `ctrl` is never written
together with `primary` or `other`: a value holding both is not a combination in this form. `{}` means every shortcut at
its default.

```json
{ "focusMode": { "code": "KeyK", "primary": true, "shift": true }, "back": { "code": "BracketLeft", "primary": true, "alt": true } }
```

The command ids are `back`, `forward`, `nextConsoleSession`, `previousConsoleSession`, `focusMode`, `newSession`,
`quickCreate` (New session with defaults) and `openSettings`. The fixed window keys and menu letters have
none.

**Such an object is judged as a whole set**, against the bindings the whole set would leave, so that two commands
swapping their shortcuts, or a chain of moves, stands whatever order the entries are written in. An entry is dropped
when its id names no command, its value is not a combination in this form, or the combination is refused (see "Which
combinations are refused" above). Each entry is judged as the keys it makes **on the platform reading it**, so an entry
valid on one machine can be dropped on another: `{ "code": "Tab", "other": true }`, ⌃Tab on a Mac, is `Win+Tab` on
Windows and is dropped there, and `{ "code": "Tab", "primary": true }`, `Ctrl+Tab` on Windows, is ⌘Tab on a Mac and is
dropped there. When two entries make the same keys the earlier one stays. Dropping an entry gives its command that
platform's default back, which can clash with another entry, so the set is judged again until nothing more is dropped.
An entry that makes its command's default keys is simply not kept. An entry that is kept is kept as it is written, never
rewritten into another form.
