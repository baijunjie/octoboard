# Configuration export and import dialogs

The window's side of carrying a configuration between machines: where exporting and importing start, the export
dialog, and the import dialog up to its review and the start of the import. What the import dialog shows while the
import runs and once it has finished is in `docs/product/configuration-import-progress-and-result.md`. What a
configuration document holds, and what an import does on the machine running Octoboard, are in
`docs/product/configuration-export-and-import.md`; the terms used here — the document's regions, the machine, account
references and their fallbacks — are that doc's.

## Where they are

Exporting is the **Export** button of Settings' **Configuration** section, and nowhere else.
Importing is the **Import** button of that section, and the **Import configuration** button of the welcome screen,
which is what an installation with no console yet shows. The import dialog opened from Settings opens over Settings,
which stays open under it, and belongs to the window rather than to Settings, so nothing that closes Settings ends it;
when it closes, keyboard focus goes back to the section's **Import** button. Both go through the window's own file dialogs, so no place is
imposed on the file: the desktop app raises its save and open dialogs, and a browser tab downloads the file and opens a
file picker. Where the window can do neither, the welcome screen's button is not shown and the Configuration section
says that exporting and importing are not available (see "Configuration" in `docs/product/settings.md`).

## The file

Both dialogs offer JSON files (`.json`). The name offered for saving is `octoboard-<console name>.json` for one console
— any character a file name cannot hold (a control character, `/`, `\`, `:`, `*`, `?`, `"`, `<`, `>`, `|`) becomes a
space, and runs of spaces become one — and `octoboard-configuration.json` for any other export, or for a console whose
name leaves nothing. The user can change it in the save dialog; a browser may file the download under a name of its
own. The desktop app's open dialog reads the picked file whole and refuses, before anything is sent to the daemon, one
larger than 64 MiB, as exceeding the file-read limit, and one that is not UTF-8 text, as not a text file; either
is told in an error alert over the import dialog whose remedy is to choose a configuration file a compatible version
exported, and another file can be chosen.

## When the file fails

An error alert raised from either dialog — a file that could not be saved or read, or a request the daemon refuses —
stands over it, and the dialog stays open. It is titled "Could not export the configuration" from the export dialog and
"Could not import the configuration" from the import dialog (see "The error alert" in `docs/product/error-alerts.md`).
A file that could not be written says that the file could not be saved, with the remedy to choose a writable folder
with enough disk space and save again, unless a recognized operating-system cause the system's error names gives its
own, such as granting Octoboard access (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`). A
picked file that could not be read at all says that the file could not be read, with the remedy, whatever the system's
error names, to make sure the file still exists and that Octoboard may read it, granting access in the system's privacy
settings if needed, or to choose another file. What the system itself said is under **Error details**, never in the
sentence.

## The export dialog

**Exporting** opens a dialog that chooses each region of the document on its own (see "Scope" in
`docs/product/configuration-export-and-import.md`):

- **Consoles and projects**, a choice of three, each a label with nothing under it: **Console** *name* **and its
  projects** — the console the sidebar shows, offered only while there is one —, **All consoles and their projects**,
  and **Do not export consoles**. The first is chosen when it is offered, and All consoles otherwise.
- **Include full project paths**, a checkbox, unchecked by default (see "Project paths" in
  `docs/product/configuration-export-and-import.md`). It is about project paths alone — the accounts' directories are
  written whatever it says — so it is not shown while Do not export consoles is chosen, and its note follows its own
  state and nothing else: while unchecked, that checking it has the file record each project's full directory path, so
  an import can look for the project there first; while checked, that an import first looks for each project at its
  recorded full path. Only while it is checked, and so before anything is saved, a warning alert stands directly under
  it (see "Warnings about a choice being made" in `docs/product/error-alerts.md`), titled that the file contains
  project paths: they can reveal details such as user and folder names; the thing to do is to share the file only with
  people the user trusts.
- A group headed **Settings**, with a checkbox for each section of Settings whose settings a document can carry, named
  and ordered as Settings names them: **General**, **Appearance**, **Agent behavior**, **Git**, **Keyboard
  shortcuts** and **Cleanup** (see "Which settings travel" in `docs/product/settings-in-a-configuration.md`). Each has a
  one-sentence note under it saying what it includes: General the language, resuming sessions on start, preventing
  sleep, closing idle terminals and the sorting and, only where this client's platform has both, running in the
  background and the menu bar icon (named as the system names it); Appearance the theme, the Markdown style and the
  terminal font size; Agent behavior which requests are approved automatically and whether questions are asked as choices and whether sessions
  are kept to their own project; Git the default
  clone directory, pulling repositories automatically and the Git workflow; Keyboard shortcuts the shortcuts the
  user has changed; Cleanup deleting archived sessions whose conversation is gone. Trusted folders, Permissions and
  Notifications have no checkbox, since nothing of them travels.
- A group headed **Agent accounts**, with one checkbox, **Include agent accounts**, whose note says that it includes
  the account names, their configuration directories, their turned-off choices, their order, the added agents and the
  agent account settings, and that neither sharing the default configuration nor login credentials are included, so
  sharing is to be turned on and the accounts logged in again after an import. With it unchecked the file names no
  account at all: every console and project in it uses its agents' default accounts (see "How an account reference
  travels" in `docs/product/configuration-export-and-import.md`).

Every checkbox of the two groups starts unchecked, and no note changes with the other choices.

**Export** is unavailable while nothing is chosen — Do not export consoles with no checkbox checked — and nothing says
why, since the boxes that left it so are in view. It asks the daemon for the document with the sections checked, adds
the settings this client holds of those sections to it — for General its sorting and, in the desktop application,
keeping running in the background and showing the menu bar icon; for Appearance its theme, Markdown style and terminal
font size; for Keyboard shortcuts its changed shortcuts (see "The settings a client holds" in
`docs/product/settings-in-a-configuration.md`) — and opens the save dialog. In the desktop app a
toast names the file it was saved as. A browser files a download itself, under its own name and wherever its settings
say, and does not report one that was blocked or cancelled, so there the toast says only that the configuration was
downloaded and names no file. The save dialog is part of the export's request, so until it is answered the export
dialog is busy and cannot be left (see "A dialog cannot be left while its request runs" in `docs/product/dialogs.md`).
Dismissing the desktop app's save dialog saves nothing and leaves the export dialog open; a file that could not be
written is told as above.

## Choosing the file to import

**Importing** opens a dialog in three steps. The first step asks only for a configuration file exported earlier.
The user picks a file, and the window sends its document to the daemon to be inspected; a file that is not JSON,
or that the daemon refuses (not a configuration, a version this Octoboard does not read, unreadable), is told with its
reason, and another can be chosen. A browser too old to report a dismissed file picker never answers for one, so the
dialog is not busy while the picker is open: its **Choose file** button shows as pending, but the dialog can be closed,
or the picker opened again. Once the picked file's document is sent, the dialog cannot be left until the daemon has
answered (see "A dialog cannot be left while its request runs" in `docs/product/dialogs.md`). What the daemon answers is
then **reviewed before anything is created**.

## The import review

The review starts with a sentence naming the file. Under it, where the file carries more than one region, a group
headed **What to import** has a checkbox for each region the file carries, in this order: **Consoles and projects**,
checked; **Settings**, unchecked, since settings change how the machine behaves rather than add to what it holds;
**Agent accounts**, checked when the file brings something for an agent — an agent to add, an account, a default
account's name or turned-off choice that the import would apply, or provider settings or tags that an agent's card
would show (see "Agent accounts in the review" below) — and unchecked otherwise, as for a file whose accounts region
holds only the three switches of the agent account settings. A region left unchecked is not imported. A file that
carries one region alone has no such group, and importing it imports that region. A file whose list of consoles is
empty carries no Consoles and projects region, unless it carries nothing else at all; a file carries the Settings
region when it carries at least one section of them (see "Which settings travel" in
`docs/product/settings-in-a-configuration.md`).

Each region to be imported then has a **tab** of the same name, in the same order, holding its review. A region left
unchecked has no tab, and tabs come and go as the boxes are checked. The tab bar stands even for a single tab, that of
a file of one region included; with nothing checked there is no tab. A tab whose review needs attention — on Consoles
and projects, a project not found or an account reference that falls back to a default account; on Agent accounts, an
account of a checked agent that cannot be created as the file has it — has an amber warning glyph after its label, its
accessible name says it needs attention, and while the pointer rests on it or it has keyboard focus a tooltip beside it
says that this part has something that needs the user's attention. The Settings tab never has one. The first tab is
open when the review opens.

**Import** is unavailable while the review would import nothing: no Consoles and projects, no Agent accounts, and
either no Settings or Settings with every section unchecked. With Agent accounts checked it stays available even with
every agent unchecked: the three switches of the agent account settings the file carries are applied whichever agents
are checked.

Every card of the dialog, in the review, the progress and the result alike, is outlined with its rows on the dialog's
own background; its header row, and an agent card's Accounts band, stand on a fill of their own.

### Consoles and projects in the review

In this order:

- When the file has a console, a note that every console is created as a new one and the existing consoles stay as
  they are.
- Four buttons under the label **Set every project that offers it to**: **Exported path**, **Default clone
  directory**, **Clone from remote** and **Leave out**, each with the number of projects it reaches. Each sets that
  choice on every project that offers it — Leave out reaches every project — and a project it does not reach keeps its
  choice; a button that reaches none is unavailable.
- A card for each console: its name, how many projects it has (or that it has none), and a row for each of its
  projects. Where the console's own account references would fall back to a default account, an amber warning mark
  follows its name (see "Account fallbacks" below).
- While any project is set to be cloned, a note that cloning can take several minutes and that an import cannot be
  cancelled once it has started.

A **project's row** has the project's name and, under it, where the daemon found it: at its exported path, in the
default clone directory, or not found but able to be cloned from its remote. That line follows what was found, whatever
choice is then made. A project with nothing offered, and so no default (see "Where an imported project goes" in
`docs/product/configuration-export-and-import.md`), has no such line: an amber warning mark follows its name instead,
named after the project as not found, whose popover says that no matching project folder was found — or, for an
exported path written on another operating system (see "Where an imported project goes" in
`docs/product/configuration-export-and-import.md`), that the exported path was written on another operating system and
so cannot be used here — and that it is left out unless a directory is chosen for it. A project not found that also
has account fallbacks still has this one mark, named after the project as not found and with accounts not set up, its
popover adding the fallbacks' sentences (see "Account fallbacks" below) after its own. The choice of where the project goes sits at the end of the
name's line, and drops under it where the row is narrow; its accessible name is "Where to place" followed by the
project's name. A row offers only the choices its state allows, with the daemon's default preselected, and always
offers **Another directory** and **Leave out**. Labelled lines under them give the paths:

- **Exported path** — the path the project had on the exporting machine. There is none for a project the document
  records by its directory name alone, nor for a project placed at its exported path, whose **Placed in** line already
  says it.
- **Placed in** — the directory the choice would use, for the exported path and the default clone directory.
- **Clone from** and **Clone into** — where a clone would come from and go to, for a clone.

A project whose exported path was written on another operating system but that has a default — able to be cloned
from its remote — says so in a note mark after the value of its Exported path line, named after the project's exported
path as unable to be used, whose popover says only that the exported path was written on another operating system and
so cannot be used here, with no remedy. The Exported path line still shows the path as the file records it. Such a
project with nothing offered says it in its not-found mark instead (above).

Another directory has no Placed in line: it shows a field labelled **Directory** (its accessible name is "Directory
for" followed by the project's name), with a **Browse** button, where a path is typed or browsed to; a path that is
not absolute and does not start with `~/` is refused. A project left out has no path line but its exported path. Where
the project's own account references would fall back to a default account, an amber warning mark follows its name (see
"Account fallbacks" below).

What the rows find and offer under the default clone directory, and where a clone would go, follow the choice about the
file's default clone directory (see "Settings in the review" below): while Settings is imported with its Git section
checked and the file's directory is used, they are worked out under the file's directory, and otherwise under this
machine's. When that changes, a row the user has made no choice for — on the row itself or through the Set every
project buttons — moves to the new preselected choice, or to Leave out where there is none; a row the user chose for
keeps its choice, unless it is Default clone directory or Clone from remote and is no longer offered, when it moves to
the new preselected choice too.

### Agent accounts in the review

In this order:

- A card headed **Agent account settings**, when the file carries the agent account settings, with a line for each of
  their three switches — **Automatically select an account**, **Automatically switch accounts at the usage limit** and
  **Start usage windows automatically** — badged as a switch of Settings is (see "Settings in the review" below). They
  belong to no agent and are applied whenever Agent accounts is imported, whichever agents are checked; the card has
  no choice of its own.
- A card for each agent the file brings something for, headed by the agent's name: first the agents the file gives
  accounts or default-account changes, in the file's order, then the agents it adds to the agent list, then the other
  agents this build knows that its provider settings or tags name. An agent for which none of the parts of a card
  would show has no card (see "An agent's card" below).
- Where a checked agent has an account the import would create — one that does not exist yet and is not left out,
  whether in the file's directory or in another — a note that the accounts are created without being logged in, and
  that each is to be logged in after the import.

#### An agent's card

The card's title is a checkbox named by the agent, checked when the review opens. Unchecked, the card collapses to its
title row, with a **Left out** outcome at its end, and nothing of that agent is imported: it is not added to the agent
list, its default account is neither named nor turned off or on, none of its provider settings or tags is applied, and
none of its accounts is created (see "Importing accounts" in `docs/product/configuration-export-and-import.md`). Its
accounts then count for neither the tab's warning glyph nor the login note. Checked, the card holds, in this order,
each part only where the file has it:

- a line saying that the agent will be added to the agent list, where it is among the file's added agents and this
  machine has not added it;
- a line saying that the default account will be named with the name the file gives it, where the import would apply
  that name; a name that would not be applied is not mentioned;
- a line saying that the default account will be disabled, or enabled, where the import would turn it off or back on;
  one the file leaves as this machine has it is not mentioned;
- the agent's **provider settings** (see "Provider settings" in `docs/product/per-agent-settings.md`), a line for each
  setting — today **Permission approval**, **Claude compatibility** and **Codex compatibility** — for which the file carries a choice that this
  machine's agent declares as carried and that the setting takes, named as the agent's tab names it, or, where the
  window has no wording for it, by the setting's id in words (an id `sandbox_mode` as "Sandbox mode"). Badged **Set to
  the file's**, it gives the file's value with, in brackets, the value this machine has now: a switch **On** or
  **Off**, a choice by its name, or by its value in words where the window has no name for it, and a choice left unset
  here by its provider's inheritance label — **Custom (config.toml)** for Codex permission approval, otherwise
  **Follow account configuration**. It is badged **No change** when the value is the same, or when this machine does
  not report the setting;
- **Tags**, where the file carries tags for the agent: badged **Set to the file's**, with the file's tags shown as tags,
  or **No tags** for none, and in brackets the agent's tags now; or **No change** when they are the same;
- under a band headed **Accounts** — the band only where something stands above it — a row for each account the file
  has for the agent, in the file's order.

An **account's row** has the account's name, then a badge saying what the import would do with it: **New**, or
**Already exists, used as it is** for an account of that agent that already has the name (see "Importing accounts" in
`docs/product/configuration-export-and-import.md`); then a **Disabled** badge where the file has it turned off. Whether
an account shares the default configuration is not shown, since an import never turns that on. An account that cannot
be created as the file has it has no such badge: an amber warning mark follows its name instead (see "Failures,
warnings and notes marked at their point" in `docs/product/error-alerts.md`):

- for a directory in use, the mark is named after the account's directory being in use, and its popover says which
  account uses it — the default account by its name in the current language — and that the account is left out unless
  another directory is chosen for it;
- for an account the Add account form would refuse, the mark is named after the account as unable to be created, and
  its popover says why, with the form's remedy where it has one, then that the account is left out unless another
  directory is chosen for it — or, where its name is the problem, only that it is left out — and last any raw text.
  For an account whose directory in the file was written on another operating system (see "Importing accounts" in
  `docs/product/configuration-export-and-import.md`), the popover never says the directory is not absolute: it says
  that the directory in the file was written on another operating system and so cannot be used here, then that the
  account is left out unless another directory is chosen for it. An account whose name is the problem as well is
  described by its name's reason, as above.

The choice of what to do with the account sits at the end of the name's line, and drops under it where the row is
narrow; its accessible name is "What to do with" followed by the account's name. A new account offers **Create it**,
preselected, and **Leave out**. An existing account has no choice, since it is used as it is. A directory in use, or a
directory the form refuses, offers **Another directory** and **Leave out**, with Leave out preselected; a name the form
refuses offers Leave out alone.

A line under the name gives the directory as the file writes it, led by a folder icon whose tooltip names it
**Directory in the file**, on every row but an existing account's, and not while Another directory is chosen. Another
directory shows a field labelled **Directory** (its accessible name is "Directory for" followed by the account's name),
with a placeholder giving an example made from the agent's own default directory, such as `~/.claude-work` for Claude
Code, and a **Browse** button. A field left empty says to enter a directory or leave the account out, and a path that
is not absolute and does not start with `~/` is refused.

### Settings in the review

A card for each section of Settings the file carries, each headed by its section's name, in the order of Settings'
sections: **General**, **Appearance**, **Agent behavior**, **Git**, **Keyboard shortcuts** and **Cleanup** (see
`docs/product/settings.md`); a section the file does not carry has no card. The card's title is a checkbox named by the
section, and every section's checkbox is checked when the review opens. Only the sections checked are imported;
unchecked, a card collapses to its title row, with a **Left out** outcome at its end. Within a card the lines stand in
the order of that section's rows:

- **General**: Language, then the switch Resume sessions on start, then Keep running in the background and Show menu
  bar icon, then the switch Prevent sleep and When to prevent sleep, then the switch Close idle terminals when closing
  the panel, then **Sorting**.
- **Appearance**: Theme, Markdown style, then Terminal font size.
- **Agent behavior**: its six switches.
- **Git**: the default clone directory, Automatically pull repositories, then one line for the git workflow.
- **Keyboard shortcuts**: one line, **Changed shortcuts**.
- **Cleanup**: the switch Automatically delete archived sessions whose conversation is gone.

A setting only a client holds, Language and Sorting have their line only when the file carries a value this client
can use: a language this build offers; Keep running in the background and Show menu bar icon have none in a browser. A
switch, When to prevent sleep or the git workflow that the file does not carry within a section it does carry — Resume
sessions on start, in a file from a build without it — has its line all the same, badged **No change**, since applying
leaves it as it is; so has When to prevent sleep when the file names a choice this build does not know.

A **switch**'s line names the setting with a badge for what applying would change from what is set now: **Turns on**,
with what then follows in that setting's own words (see "The application settings an export carries" in
`docs/product/settings-in-a-configuration.md`) — nothing more for Prevent sleep; **Turns off**; or **No change**.

**When to prevent sleep**, which the daemon holds, is badged **Set to the file's**, with the file's choice and, in
brackets, the one set now, each by its name as Settings words it; or **No change** when they are the same.

A **setting only a client holds** — Theme, Markdown style, Terminal font size, Keep running in the background, Show
menu bar icon — is badged **Set to the file's**, with the file's value and, in brackets, the value this client has now,
worded as Settings words them (a Markdown style by its name, a switch as **On** or **Off**, a font size in pixels, as
"18 px (now 15 px)"); or **No change** when they are the same. The file's terminal font size is given as this client
would apply it: rounded, and held to the range of 10 to 24 pixels (see "The application settings an export carries"
in `docs/product/settings-in-a-configuration.md`).
**Language**, which the daemon holds rather than the client, reads the same way, comparing the file's language with
the language the daemon holds now — or with the language this window shows, when the daemon holds none or one this
build does not offer — each by its name in its own language (see "The language the daemon holds" in `docs/product/language.md`).

The **default clone directory** has its line when the file carries one that can be used here, or one written on another
operating system. A directory written on another operating system (see "The default clone directory on import" in
`docs/product/settings-in-a-configuration.md`) is not offered: its line is badged **No change** and says that the
file's default clone directory was written on another operating system, so it cannot be used here, and that the current
default clone directory is kept; it has no checkbox. A directory that can be used here is **No change** when it is
this machine's already. Otherwise the line is badged **Set to the file's**, with the file's directory and, in brackets,
this machine's, and holds a checkbox, **Use the file's default clone directory**, checked whenever a file is opened.
Unchecked, the badge is **No change**, and the line says that this machine's directory is kept instead of the file's;
the rows of Consoles and projects follow the choice (see "Consoles and projects in the review" above).

The settings made of several items have one line each. When applying would change such a setting, its line lists under
the badge each item that would change, one per row: the item, then the file's value, then in brackets the value it has
now. A line with no item to list is badged **No change**, so a changing badge never stands without a list:

- **Sorting**, present when the file carries a sort this client knows: badged **Set to the file's**, it lists each of
  its three parts — **Sessions**, **Active projects** and **Inactive projects** — whose choice differs from this
  client's, named and worded as the Sorting group words them (see "Choosing the sort" in
  `docs/product/order-of-projects-and-sessions.md`).
- **Git workflow**, always present in the Git card, covering all five application-wide git workflow settings: badged **Set to the
  file's**, it lists each of the five whose value differs from the current one, named and worded as the settings page
  names them (see "The five settings" in `docs/product/git-workflow-settings.md`), a switch as **On** or **Off**. Squash
  commits before delivery, Rebase before delivery and Delivery are listed when they differ even where the file's Where
  sessions develop is **On the current branch**, since their values are written all the same.
- **Changed shortcuts**, present when the document carries keyboard shortcuts: badged **Set to the file's**, with how
  many of the file's shortcuts differ from the defaults — counting only those that can be used here — and that changes
  apply only to the current desktop app or, when importing in a browser, the current browser; or **Back to defaults**,
  when none of them does, with the same scope note. Under that note it lists each command whose shortcut would change,
  the shortcuts shown as key caps. It is **No change** when the file's shortcuts are the ones the current client
  already has.

Whether any of them is applied is the **Settings** checkbox of What to import, and then which sections are is each
card's own checkbox; beyond those and the default clone directory's checkbox, the cards have no choice of their own.

### Account fallbacks

In the review and in the result alike, a console or project holding account references that match no account (see
"Accounts are rebound by name" in `docs/product/configuration-export-and-import.md`) has an amber warning triangle right
after its name, named that accounts are not set up for that console or project — in the review, for a project not
found, the not-found mark instead, which says both. Its popover has one sentence per such reference, naming the agent
and the account and saying that the account is not set up in Octoboard, so the default account is used. It opens as a
row's mark does (see "Failures, warnings and notes marked at their point" in `docs/product/error-alerts.md`). A console
or project with no such reference has no mark.

In the review the marks follow what the import would create. While Agent accounts is imported, a reference to an
account the import would create from the file — as the file has it or in another directory — is no fallback, and
setting that account to Leave out, or unchecking its agent, brings its mark back. While Agent accounts is unchecked, or
the file carries no accounts, every reference that matches no account on the machine has its mark, those to the
accounts the file would have created included.

### When settings change during the review

While the review is shown with a Settings section or Agent accounts checked, a change to any application setting — from
another window, a console session or a single action — has the file read again by itself, so that the review shows, and
the import acts on, the settings as they are now. The choices made in the review are kept where they still apply: a
project's choice stays unless the new reading no longer offers it, when it goes to the preselected one, and an account's
choice stays while its new status allows it, as do the agents and Settings sections checked. **Import** shows as pending
while the file is read again, and assistive technology is told that the file was read again because settings changed.

When reading it again fails, an Alert at the top of the review, titled "Could not read the file again", gives the reason
with **Try again**, or, for a failure no retry can change, says to choose the file again with a **Choose file** button
that does so. Meanwhile **Import** is unavailable: its tooltip and description are the Alert's title, and a press of it
moves keyboard focus to the Alert's button, its focus ring showing only when Import was pressed with a key. The Alert
goes once the file is read again, or once nothing checked writes settings any more.

## Applying the import

The review's **Import** button sends the document unchanged, in one request, with the regions checked, the sections of
Settings checked while Settings is, the choice of every project that is placed — a project left out is sent none —,
when the accounts are imported, the agents checked and the choice of every account of those agents given another
directory or left out, and, when the Git section is imported, whether the file's default clone directory is used. A directory the user typed, for a project or an account of a region
being imported, that is not usable stops the import before any request: the tab holding it comes forward, Consoles and
projects first, its row is marked and its field takes focus. Otherwise the **progress** replaces the review for as long
as the request runs, which with clones can be long. When the daemon answers, the **result** replaces it, and the dialog
is renamed to say that the import finished (see `docs/product/configuration-import-progress-and-result.md`). When the request fails instead — the daemon refuses it, or the connection
to it is lost — the dialog goes back to the review with every choice as it was, and the failure is told in an error
alert over it (see "When the file fails" above). The request carries the settings versions the review was read with,
and is refused before anything is created when a setting it would write was changed after that — a change that reached
the daemon before the review could be read again. The dialog then goes back to choosing the file, and the error alert
says that settings changed after the file was read and to choose the file again to read it with the current settings.

While the request runs the dialog cannot be left (see "A dialog cannot be left while its request runs" in
`docs/product/dialogs.md`): **Cancel** stays in the footer, disabled like every other control of the dialog, beside the
**Import** button that was pressed, now pending, since an import cannot be cancelled once it has started.
