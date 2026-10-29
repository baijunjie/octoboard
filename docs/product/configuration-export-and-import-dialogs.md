# Configuration export and import dialogs

The window's side of carrying a configuration between machines: where exporting and importing start, the export
dialog, and the import dialog with its review and its result. What a configuration document holds, and what an import
does on the machine running Octoboard, are in `docs/product/configuration-export-and-import.md`; the terms used here —
the document's regions, the machine, account references and their fallbacks — are that doc's.

## Where they are

Exporting is the **Export…** button of Settings' **Configuration** section, and nowhere else.
Importing is the **Import…** button of that section, and the **Import configuration…** button of the welcome screen,
which is what an installation with no console yet shows. Both go through the window's own file dialogs, so no place is
imposed on the file: the desktop app raises its save and open dialogs, and a browser tab downloads the file and opens a
file picker. Where the window can do neither, the welcome screen's button is not shown and the Configuration section
says that exporting and importing are not available (see "Configuration" in `docs/product/settings.md`).

## The file

Both dialogs offer JSON files (`.json`). The name offered for saving is `octoboard-<console name>.json` for one console
— any character a file name cannot hold (a control character, `/`, `\`, `:`, `*`, `?`, `"`, `<`, `>`, `|`) becomes a
space, and runs of spaces become one — and `octoboard-configuration.json` for any other export, or for a console whose
name leaves nothing. The user can change it in the save dialog; a browser may file the download under a name of its
own. The desktop app's open dialog reads the picked file whole and refuses, before anything is sent to the daemon, one
larger than 64 MiB, as too large to be a configuration file, and one that is not UTF-8 text, as not a text file; either
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
  project paths: these are paths on the machine running Octoboard, which may not be the device the user is on, and they
  can reveal details such as the user name; the thing to do is to share the file only with people the user trusts.
- A group headed **Global settings**, with two checkboxes. **Application settings**, checked by default whichever
  consoles are chosen, notes that it includes the settings that apply to every console, this device's keyboard
  shortcuts among them. **Accounts**, unchecked by default, notes that it includes the account names, their
  configuration directories, their sharing and turned-off choices, their order and the added agents, and that no login
  credentials are included, so the accounts are to be logged in again after an import. Neither note changes with the
  other choices.

**Export** is unavailable while nothing is chosen — Do not export consoles with both global settings unchecked — and
nothing says why, since the boxes that left it so are in view. It asks the daemon for the document, adds this client's
keyboard shortcuts to it when it carries the application settings, and opens the save dialog. In the desktop app a
toast names the file it was saved as. A browser files a download itself, under its own name and wherever its settings
say, and does not report one that was blocked or cancelled, so there the toast says only that the configuration was
downloaded and names no file. Dismissing the desktop app's save dialog saves nothing and leaves the export dialog open;
a file that could not be written is told as above.

## Choosing the file to import

**Importing** opens a dialog in three steps. The user picks a file — the step says beforehand that the file is one
exported earlier, that its contents can be reviewed and where each project goes on the machine running Octoboard chosen
before anything is created, and that the file may also carry accounts, which are created without being logged in, and
the application settings — and the window sends its document to the daemon to be inspected; a file that is not JSON,
or that the daemon refuses (not a configuration, a version this Octoboard does not read, unreadable), is told with its
reason, and another can be chosen. A browser too old to report a dismissed file picker never answers for one, so the
dialog is not held while the picker is open: it can be closed, or the picker opened again. What the daemon answers is
then **reviewed before anything is created**.

## The import review

The review starts with a sentence naming the file. Under it, where the file carries more than one region, a group
headed **What to import** has a checkbox for each region the file carries, in this order: **Consoles and projects**,
checked; **Accounts**, checked; **Application settings**, unchecked, since they change how the machine behaves rather
than add to what it holds. A region left unchecked is not imported. A file that carries one region alone has no such
group, and importing it imports that region, the application settings included. A file whose list of consoles is empty
carries no Consoles and projects region, unless it carries nothing else at all.

Each region to be imported then has a **tab** of the same name, in the same order, holding its review. A region left
unchecked has no tab, and tabs come and go as the boxes are checked. A single tab has no tab bar, its review standing
on its own; with nothing checked there is no tab, and **Import** is unavailable. A tab whose review needs attention —
on Consoles and projects, a project not found or an account reference that falls back to a default account; on
Accounts, an account that cannot be created as the file has it — has an amber warning glyph after its label, its
accessible name says it needs attention, and while the pointer rests on it or it has keyboard focus a tooltip beside it
says that this part has something that needs the user's attention. The Application settings tab never has one. The
first tab is open when the review opens.

Every card of the dialog, in the review and in the result alike, stands on a fill of its own, and its header row on a
darker one.

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
named after the project as not found, whose popover says that it was not found on the machine running Octoboard and
that it is left out unless a directory is chosen for it. A project not found that also has account fallbacks still has
this one mark, named after the project as not found and with accounts not set up, its popover adding the fallbacks'
sentences (see "Account fallbacks" below) after its own. The choice of where the project goes sits at the end of the
name's line, and drops under it where the row is narrow; its accessible name is "Where to place" followed by the
project's name. A row offers only the choices its state allows, with the daemon's default preselected, and always
offers **Another directory** and **Leave out**. Labelled lines under them give the paths:

- **Exported path** — the path the project had on the exporting machine. There is none for a project the document
  records by its directory name alone, nor for a project placed at its exported path, whose **Placed in** line already
  says it.
- **Placed in** — the directory the choice would use, for the exported path and the default clone directory.
- **Clone from** and **Clone into** — where a clone would come from and go to, for a clone.

Another directory has no Placed in line: it shows a field labelled **Directory** (its accessible name is "Directory
for" followed by the project's name), with a **Browse** button, where a path is typed or browsed to; a path that is
not absolute and does not start with `~/` is refused. A project left out has no path line but its exported path. Where
the project's own account references would fall back to a default account, an amber warning mark follows its name (see
"Account fallbacks" below).

### Accounts in the review

In this order:

- Where the file's added agents include agents this machine has not added, a line saying that those agents will be
  added to the user's agents.
- For each agent whose default account the import would name, a line saying that its default account will be named
  with the name the file gives it. A name that would not be applied is not mentioned.
- A card for each agent of the file, headed by the agent's name, with a row for each of its accounts in the file's
  order — or, for an agent the file only names a default account for, a line saying that the file has no accounts of
  its own for that agent.
- A note that the accounts are created without being logged in, and that each is to be logged in after the import.

An **account's row** has the account's name, then a **Shares default configuration** badge where the file has it share
the default configuration and a **Disabled** badge where the file has it turned off. Under the name a line says what
the import would do with it: **New**, or **Already exists, used as it is** for an account of that agent that already
has the name (see "Importing accounts" in `docs/product/configuration-export-and-import.md`). An account that cannot be
created as the file has it has no such line: an amber warning mark follows its name instead (see "Failures, warnings
and notes marked at their point" in `docs/product/error-alerts.md`):

- for a directory in use, the mark is named after the account's directory being in use, and its popover says which
  account uses it — the default account by its name in the current language — and that the account is left out unless
  another directory is chosen for it;
- for an account the Add account form would refuse, the mark is named after the account as unable to be created, and
  its popover says why, with the form's remedy where it has one, then that the account is left out unless another
  directory is chosen for it — or, where its name is the problem, only that it is left out — and last any raw text.

The choice of what to do with the account sits at the end of the name's line, and drops under it where the row is
narrow; its accessible name is "What to do with" followed by the account's name. A new account offers **Create it**,
preselected, and **Leave out**. An existing account has no choice, since it is used as it is. A directory in use, or a
directory the form refuses, offers **Another directory** and **Leave out**, with Leave out preselected; a name the form
refuses offers Leave out alone.

A line labelled **Directory in the file** gives the directory as the file writes it, on every row but an existing
account's, and not while Another directory is chosen. Another directory shows a field labelled **Directory** (its
accessible name is "Directory for" followed by the account's name), with a placeholder giving an example made from the
agent's own default directory, such as `~/.claude-work` for Claude Code, and a **Browse** button. A field left empty
says to enter a directory or leave the account out, and a path that is not absolute and does not start with `~/` is
refused.

### Application settings in the review

One card, with a line for each setting the document carries — one an older document leaves out has no line — naming
the setting with a badge for what applying would change from what is set now: **Turns on**, with what then follows in
that setting's own words (see "The application settings an export carries" in
`docs/product/configuration-export-and-import.md`); **Turns off**; or **No change**. A document that carries the
application-wide git workflow settings adds one line for all four, named **Git workflow**, badged **Set to the
file's** when they differ from the current ones and **No change** when they do not. A document that carries a Claude
compatibility choice for an agent that has the setting adds one line for them all, named **Claude compatibility**,
badged **Set to the file's** when applying would change the value of an agent this machine reports and **No change**
when it would not; a document with no such choice has no line. A document that carries keyboard shortcuts adds a line
for them, badged **Set to the file's**, with how many of the file's shortcuts differ from the defaults — counting only
those that can be used here — and that they apply to this device or browser only; **Back to defaults**, with that it
applies to this device or browser only; or **No change**, when the file's are the ones this device already has.

Whether the settings are applied is the **Application settings** checkbox of What to import; the card has no choice of
its own.

### Account fallbacks

In the review and in the result alike, a console or project holding account references that match no account (see
"Accounts are rebound by name" in `docs/product/configuration-export-and-import.md`) has an amber warning triangle right
after its name, named that accounts are not set up for that console or project — in the review, for a project not
found, the not-found mark instead, which says both. Its popover has one sentence per such reference, naming the agent
and the account and saying that the account is not set up in Octoboard, so the default account is used. It opens as a
row's mark does (see "Failures, warnings and notes marked at their point" in `docs/product/error-alerts.md`). A console
or project with no such reference has no mark.

In the review the marks follow what the import would create. While Accounts is imported, a reference to an account the
import would create from the file — as the file has it or in another directory — is no fallback, and setting that
account to Leave out brings its mark back. While Accounts is unchecked, or the file carries
no accounts, every reference that matches no account on the machine has its mark, those to the accounts the file would
have created included.

## Applying the import

The review's **Import** button sends the document unchanged, in one request, with the regions checked, the choice of
every project that is placed — a project left out is sent none — and, when the accounts are imported, the choice of
every account given another directory or left out. A directory the user typed, for a project or an account of a region
being imported, that is not usable stops the import before any request: the tab holding it comes forward, Consoles and
projects first, its row is marked and its field takes focus. The dialog cannot be dismissed while the request runs,
which with clones can be long. The **result** then replaces the review, and the dialog is renamed to say that the
import finished.

## The import result

The result has a tab for each region imported, in the review's order: **Consoles and projects** when the file had
consoles and they were imported, **Accounts** when the accounts were imported, and **Application settings** when they
were applied or could not be. A single one has no tab bar. A tab whose region holds a failure — on Consoles and
projects, a console or project that failed or was not attempted; on Accounts, an account that failed or a sharing
warning; on Application settings, settings that could not be applied — has the warning glyph, as in the review, and the
first such tab is the one open; otherwise the first tab is. Nothing stands above the tabs: each region's totals are its
own, and whether nothing failed is judged by that region's own failures alone.

The result is the only place the import's failures are shown: no error alert opens for them. A failure is written out
where it happened, under the row it concerns or, for a console, under its card's header: the reason in Octoboard's own
words, under it the one thing to do about it, and, where there is more, a **Details** button that opens the further
explanation and the raw text (see "Names and diagnostic details" in `docs/product/error-alerts.md`). An outcome is a
glyph followed by its word, each outcome with a glyph of its own, and is not a control.

### Consoles and projects in the result

A line of totals, each item a full phrase, the items set apart by dots: the consoles created, the projects created, the
projects left out, how many failed — consoles and projects together — and the projects not attempted, each only when
there were any, and **Nothing failed** when nothing failed and nothing went unattempted.

Then a card for each console, its header holding the console's name and always its outcome, **Created** or
**Failed**, with the reason under the header when it failed; then a row for each of its projects, or a line saying
that it has none. A project's row has the project's name and its outcome — **Created**, **Left out**, **Failed** or
**Not attempted** — on one line, the outcome going under the name where the row is narrow. Under them, for a project
created, a muted line saying how it was placed (at its exported path, in the default clone directory, cloned from its
remote, in the directory chosen); for a project not attempted, that its console could not be created; for a project
that failed, its reason. A console or project whose account references fell back to a default account has the warning
mark right after its name, on the card's header or the project's row (see "Account fallbacks" above); the result has no
list of them elsewhere.

### Accounts in the result

A line of totals in the same form: the accounts created, those that already existed, those left out and how many
failed, each only when there were any, and **Nothing failed** when none failed.

Then a card for each agent the file had accounts for, headed by the agent's name, with a row for each account in the
file's order and its outcome: **Created**, **Already existed**, **Left out**, or **Failed** with its reason under it. A
created account whose sharing of the default configuration could not be turned on has, under its row, an amber warning
glyph and a sentence saying that the account was created but sharing could not be turned on, followed by that failure's
remedy and Details. The result does not show the accounts' directories.

Under the cards, a line names the agents added to the user's agents, when any were; a line for each agent whose
default account is now named as in the file; and, when any account was created, a note that the new accounts are not
logged in yet and that each is to be logged in before a session uses it. The result has no **Log in** button: the
accounts are logged in as any other (see `docs/product/logging-accounts-in.md`).

### Application settings in the result

When the application settings could not be applied, a red compact alert titled that the application settings could not
be imported, saying so with the reason, its remedy and **Details** (see "Failures shown on the screen" in
`docs/product/error-alerts.md`), and nothing else. When they were applied, a card headed **Application settings
applied**, with a line for each setting applied, badged **On** or **Off**; one line for the git workflow settings, when
the document carried them, badged **Matches the file**; one line for Claude compatibility, when the document carried a
choice for an agent that has the setting, badged **Matches the file**; and, when this device applied the file's keyboard
shortcuts, a line for them badged with how many changes from the defaults its shortcuts now have, or **Defaults**, with
how many of the file's shortcuts could not be used here and were left out said under it, when any were. This tab has no
totals.
