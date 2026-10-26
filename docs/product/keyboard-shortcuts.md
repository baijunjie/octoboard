# Keyboard shortcuts

Octoboard's own commands have keyboard shortcuts the user can change, and Settings' **Keyboard shortcuts** section lists
them together with the keys that belong to the window or the system, which cannot be changed. A changed shortcut belongs
to the client it was changed in — the desktop app, or one browser — and nowhere else (see "Where the shortcuts are
kept" below).

What each command does, and when it does nothing, is in the doc of the command itself; this doc covers the keys: which
there are, how they are changed and how a key press is matched against them.

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

So a default means the same thing on every platform, and a shortcut carried from one machine to another in a
configuration file (see "The shortcuts as data" below) is the same combination there. Wherever a shortcut is shown its
key caps read in the order the other modifier, Alt, Shift, the primary modifier — ⌃ ⌥ ⇧ ⌘ on an Apple keyboard — and
then the key, and keep that order under a right-to-left language (see `docs/product/right-to-left-layout.md`). Where the
Keyboard shortcuts section tells assistive technology a combination, it says it in words, as "Shift+Command+F".

## What is listed

The section lists every shortcut in six groups, in this order. Within a group the commands that can be changed come
first. The defaults are written for an Apple keyboard; elsewhere each modifier is the one the table above maps it to.

| Group | Command | Default | Can be changed | Listed in |
|---|---|---|---|---|
| Navigation | **Back** | `Cmd+[` | yes | the desktop app on macOS |
| Navigation | **Forward** | `Cmd+]` | yes | the desktop app on macOS |
| Navigation | **Next console session** | `Ctrl+Tab` | yes | the desktop app |
| Navigation | **Previous console session** | `Ctrl+Shift+Tab` | yes | the desktop app |
| Navigation | **Next region of the window** | `F6` | no | every client |
| Navigation | **Previous region of the window** | `Shift+F6` | no | every client |
| Focus mode | **Toggle focus mode** | `Shift+Cmd+F` | yes | every client |
| Focus mode | **New session** | `Cmd+N` | yes | every client |
| Focus mode | **New session with defaults** | `Shift+Cmd+N` | yes | every client |
| Sessions | **Lock or unlock the session** | `Shift+Cmd+L` | yes | every client |
| Sessions | **Pin or unpin the session** | `Shift+Cmd+P` | yes | every client |
| Sessions | **Archive the session** | `Shift+Cmd+A` | yes | every client |
| Sessions | **Copy session ID** | `Shift+Cmd+C` | yes | every client |
| Application | **Open settings** | `Cmd+,` | yes | the desktop app (its native menu) |
| Editing | **Undo**, **Redo**, **Cut**, **Copy**, **Paste**, **Select all** | `Cmd+Z`, `Shift+Cmd+Z`, `Cmd+X`, `Cmd+C`, `Cmd+V`, `Cmd+A` | no | the desktop app (its native menu) |
| Window | **Hide Octoboard**, **Hide others**, **Toggle full screen**, **Minimize**, **Close window**, **Quit Octoboard** | `Cmd+H`, `Option+Cmd+H`, `Ctrl+Cmd+F`, `Cmd+M`, `Cmd+W`, `Cmd+Q` | no | the desktop app (its native menu) |

**A command the client does not have is not listed**: in a browser, Back and Forward, the console-session keys, Open
settings and the native menu's keys are left out, and so are the Application, Editing and Window groups, which hold
nothing else. Such a command keeps its key all the same, and that key cannot be given to another command there either
(see "Which combinations are refused" below).

What each command does: Back and Forward in "Keyboard shortcuts" in `docs/product/navigation-history.md`; the
console-session keys in "The switch strip" in `docs/product/focus-mode.md`; the region keys in
`docs/product/moving-focus-between-regions.md`; Toggle focus mode in "The focus mode shortcut" and the two New session
commands in "Creating sessions from the keyboard", both in `docs/product/focus-mode.md`; the four session commands in
"Session keyboard shortcuts" in `docs/product/sidebar-session-rows.md`; Open settings in "Opening and closing
Settings" in `docs/product/settings.md`; and the native menu's keys in `docs/product/application-lifecycle.md`.

## The Keyboard shortcuts section

The section sits after Notifications and before Configuration in Settings (see `docs/product/settings.md`). A paragraph
under its heading says that the app's own commands can be changed, that the keys of the system and the window are listed
too and cannot be, and that the changes apply to this device or browser only. Beside it, only while at least one
shortcut differs from its default, is a **Restore defaults** button.

Each group has a heading, and each command a row: its name, then its current key caps, and then either a **Change**
button — with a **Reset** button after it while the command's shortcut differs from its default — or, for a key that
cannot be changed, the word **Fixed**.

A group's heading stays pinned at the top of the section's area while the group's rows scroll beneath it, as in
"Group headings" in `docs/product/settings.md`.

### Changing a shortcut

**Change** starts recording on that row: the key caps give way to a line asking for the new keys, and Change becomes
**Cancel**. One row records at a time; starting another row's recording ends the first.

While a row records, every key press goes to the recording and to nothing else: no window shortcut runs, Escape does not
close Settings and the terminal receives nothing. A modifier pressed on its own is the start of a combination and is
waited past; a key held down counts once; a key pressed during an input method's composition is left alone. The first
other combination pressed is then taken:

- **One that can be used is saved at once**: the recording ends and the row shows the new keys. Choosing a command's
  default is the same as resetting it.
- **One that cannot be used is not saved**: the row says why under the command's name (see "Which combinations are
  refused" below) and goes on recording, so another combination can be tried straight away.

A few keys are not recorded:

- **Escape**, with no modifier, cancels the recording, as **Cancel** does.
- **Tab** held with neither `Ctrl` nor `Cmd` (the Windows key elsewhere) — so `Shift+Tab` and `Option+Tab` too — ends
  the recording and moves keyboard focus as Tab always does. Tab with either of them is recorded, as the
  console-session keys use `Ctrl+Tab`.
- **Enter** and **Space**, with no modifier, are left to the focused button, so on Cancel they cancel.

Ending a recording in any of these ways changes nothing and clears the row's reason.

### Resetting and restoring the defaults

**Reset** gives the row's command its default back, and keyboard focus moves to that row's Change button. When another
command has taken the default meanwhile, the reset is refused: the row says which command holds it, the changed shortcut
stays, and focus stays on Reset.

**Restore defaults** gives every command its default back at once, ends any recording, and moves keyboard focus to the
list's first Change button as the button itself goes away.

### What assistive technology is told

The section has one status region of its own, which says that a recording has started and that Escape cancels it, the
new keys once one is saved, that a change was cancelled, the keys a reset or Restore defaults put back, and every
reason a combination or a reset was refused — again when the same refusal repeats. A reason shown on a row is also the
description of that row's Change and Reset buttons.

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
  Windows key without Ctrl is refused as the system's. The one exception is Tab with ⌃, with or without other modifiers,
  which the console-session keys use.
- **An editing key with the primary modifier**: the arrow keys, Home, End, Page Up, Page Down, Backspace, Delete and
  Enter, which with ⌘ (Ctrl) move the caret or edit text in every text field and in the terminal.
- **A combination the system keeps for itself**: the primary modifier with Tab, with any other modifiers; and on an
  Apple keyboard `Cmd+Space` and ``Cmd+` ``, with any modifiers but ⌃.
- **`Ctrl+C` on its own, off an Apple keyboard**, which interrupts the program in the terminal. (On an Apple keyboard
  `Cmd+C` is Copy, refused as a conflict below.)
- **Shift with a digit or a symbol key, for Open settings.** The native menu matches its keys by the character a key
  types with Shift applied, so such a shortcut would never fire. It is refused on every client, since a shortcut can be
  carried to the desktop app in a configuration file.
- **A combination another command has**: one of the fixed keys, or the current shortcut of another command that can be
  changed. The reason names the command; when that command is not listed on this client — the desktop app's own, in a
  browser — it adds that the command is the desktop app's.

## Where the shortcuts show

Everywhere a shortcut is shown it is the command's current one, and it changes as soon as the shortcut does:

- the key caps at the end of the session menu's Lock / Unlock, Pin / Unpin, Archive and Copy session ID items, and of
  the Focus mode items of the project and console session menus;
- the tooltips that name a shortcut: the **+** that the New session commands act on, which shows the shortcut of
  whichever of the two it would take (see "Creating sessions from the keyboard" in `docs/product/focus-mode.md`), and
  the session buttons listed in "Session keyboard shortcuts" in `docs/product/sidebar-session-rows.md`;
- the desktop app's native menu, whose **Settings…** item carries Open settings' shortcut;
- a report page, which passes the window's Back, Forward and console-session keys on as they are bound (see "Window
  shortcuts inside a page" in `docs/product/report-panel.md`).

## Where the shortcuts are kept

**Changed shortcuts are kept per client**, in that client's own browser storage, as the appearance choice is (see
"Where the choice is stored" in `docs/product/appearance.md`). Only the shortcuts that differ from their defaults are
kept; a client with none changed, or whose storage has been cleared, has every default. The daemon does not store them,
so a desktop window and a browser tab, or two browsers reaching the same daemon, each have their own, and no agent can
read or change them (see "Who reads and changes what" in `docs/product/agent-settings-tools.md`). A kept shortcut that
cannot stand when it is read — one these rules refuse, or one for a command that no longer exists — is dropped, and
its command has its default.

A full configuration export carries the exporting client's changed shortcuts, and an import can apply a file's to the
importing client (see "The application settings a full export carries" and "Application settings are left alone unless
they are asked for" in `docs/product/configuration-export-and-import.md`).

## The shortcuts as data

The changed shortcuts, as kept and as carried in a configuration document, are one JSON object: a key for each changed
command, by its id, whose value is the combination — the key's code (the `KeyboardEvent.code` of the physical key) and
each modifier that is held, as `true`, among `primary`, `shift`, `alt` and `other` (see "How a combination is written
and matched" above). A modifier that is absent or `false` is not held. `{}` means every shortcut at its default.

```json
{ "lock": { "code": "KeyK", "primary": true, "shift": true }, "back": { "code": "BracketLeft", "primary": true, "alt": true } }
```

The command ids are `back`, `forward`, `nextConsoleSession`, `previousConsoleSession`, `focusMode`, `newSession`,
`quickCreate` (New session with defaults), `lock`, `pin`, `archive`, `copyId` and `openSettings`. The fixed keys have
none.

**Such an object is judged as a whole set**, against the bindings the whole set would leave, so that two commands
swapping their shortcuts, or a chain of moves, stands whatever order the entries are written in. An entry is dropped
when its id names no command, its value is not a combination in this form, or the combination is refused (see "Which
combinations are refused" above) — on the platform reading it, so an entry valid on one machine can be dropped on
another. When two entries claim the same combination the earlier one stays. Dropping an entry gives its command its
default back, which can clash with another entry, so the set is judged again until nothing more is dropped. An entry
equal to its command's default is simply not kept.
