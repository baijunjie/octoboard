# Configuration import progress and result

What the import dialog shows once its **Import** button is pressed: the progress while the import runs, and the result
once the daemon has answered (see "Applying the import" in `docs/product/configuration-export-and-import-dialogs.md`).
The terms used here are those of `docs/product/configuration-export-and-import.md`, and the cards are drawn as the
review's are (see "The import review" in `docs/product/configuration-export-and-import-dialogs.md`).

## The import progress

At its top, a progress bar named **Importing configuration**. Until the daemon's first report on the import it shows no
amount, and the line beside its name says that the import is starting; then it fills by the items that have settled —
done, failed or not attempted — out of every item the import handles, and the line gives the count ("3 of 7
processed"). The count is not announced at each step: assistive technology is told when the import starts and again
when every item has been processed.

Under it, one card per item group, in the review's order: a card for each console, with a row for each of its
projects the import places; a card headed **Default clone directory** when the import writes the file's; a card headed
**Settings**, standing for every section checked at once, when the rest of the settings are applied; a card headed
**Agent account settings** when the import applies them; and a card for each agent with accounts the import handles,
with a row for each of those accounts (see "Progress while an import runs" in
`docs/product/configuration-export-and-import.md`). A project or an account left out has no row, and an agent with no
account to handle — an agent left unchecked included — has no card; an account that already exists has its row,
whatever was chosen for it. There are no tabs. Each console, project, account and settings card shows its state beside its name, a
glyph followed by its word:

- **Waiting** — not started yet;
- **Creating…**, or **Cloning…** for a project being cloned, with a spinner in place of the glyph — being worked on;
- **Done** — finished. A console is done as soon as it is created, before its projects, and an account that already
  exists is done at once;
- **Failed** — with its reason written out under the name, as in the result (see "The import result" below), and
  **Details** where there is more, disabled, like everything else in the dialog, until the import has ended;
- **Not attempted** — a project whose console could not be created, with a line saying so.

The progress shows no account fallback marks or totals: those are the result's.

## The import result

The result has a tab for each region imported, in the review's order: **Consoles and projects** when the file had
consoles and they were imported, **Settings** when the sections checked were applied or could not be, or when the
file's default clone directory was written, and **Agent accounts** when the accounts were imported. A single one has
no tab bar, unlike the review's. A tab whose region holds a failure — on Consoles and projects, a console or project
that failed or was not attempted; on Settings, settings that could not be applied; on Agent accounts, an account that
failed or agent account settings that could not be applied — has the warning glyph, as in the review (see "The import review" in
`docs/product/configuration-export-and-import-dialogs.md`), and the first such tab is the one open; otherwise the
first tab is. Nothing stands above the tabs: each region's totals are its
own, and whether nothing failed is judged by that region's own failures alone.

The result is the only place the import's failures are shown: no error alert opens for them. A failure is written out
where it happened, under the name of the row it concerns or, for a console, in its card's header under its name: the
reason in Octoboard's own words, under it the one thing to do about it, and, where there is more, a **Details** button that opens the further
explanation and the raw text (see "Names and diagnostic details" in `docs/product/error-alerts.md`). An outcome is a
glyph followed by its word, each outcome with a glyph of its own, and is not a control.

### Consoles and projects in the result

A line of totals, each item a full phrase, the items set apart by dots: the consoles created, the projects created, the
projects left out, how many failed — consoles and projects together — and the projects not attempted, each only when
there were any, and **Nothing failed** when nothing failed and nothing went unattempted.

Then a card for each console, its header holding the console's name and always its outcome, **Created** or
**Failed**, with the reason in the header under the name when it failed; then a row for each of its projects, or a line saying
that it has none. A project's row has the project's name and its outcome — **Created**, **Left out**, **Failed** or
**Not attempted** — on one line, the outcome going under the name where the row is narrow. Under them, for a project
created, a muted line saying how it was placed (at its exported path, in the default clone directory, cloned from its
remote, in the directory chosen); for a project not attempted, that its console could not be created; for a project
that failed, its reason. A console or project whose account references fell back to a default account has the warning
mark right after its name, on the card's header or the project's row (see "Account fallbacks" in
`docs/product/configuration-export-and-import-dialogs.md`); the result has no list of them elsewhere.

### Agent accounts in the result

A line of totals in the same form: the accounts created, those that already existed, those left out and how many
failed, each only when there were any, and **Nothing failed** when no account failed and the agent account settings did
not fail either. Only the agents that were checked count: nothing of an agent left unchecked is in the result.

Then, when the agent account settings could not be applied, a red compact alert titled that they could not be
imported, saying so with the reason, its remedy and **Details**; otherwise, when they were applied, the **Agent account
settings** card, its three switches badged **On** or **Off**.

Then a card for each agent the import did something for, headed by the agent's name, in the review's order and with no
checkbox, holding in this order, each part only where it happened: a line saying that the agent was added to the agent
list; a line saying that the default account is now named as in the file; a line saying that the default account is
now disabled, or enabled; a line for each of the agent's provider settings applied, named as in the review (see "An
agent's card" in `docs/product/configuration-export-and-import-dialogs.md`), badged **Matches the file** with the value
it was set to; a **Tags** line badged **Matches the file**, its tags shown as tags, or **No tags** for none; and, under
an **Accounts** band where something stands above it, a row for each of its accounts in the file's order with its
outcome: **Created**, **Already existed**, **Left out**, or **Failed** with its reason under it. The result does not
show the accounts' directories.

Last, when any account was created, a note that the new accounts are not logged in yet and that each is to be logged in
before a session uses it. The result has no **Log in** button: the accounts are logged in as any other (see
`docs/product/logging-accounts-in.md`).

### Settings in the result

When the settings could not be applied, a red compact alert titled that the settings could not be imported, saying so
with the reason, its remedy and **Details** (see "Failures shown on the screen" in `docs/product/error-alerts.md`). Beside it, when the file's default clone directory had been written before the rest
failed, a **Git** card with its one line, **Default clone directory**, badged **Matches the file**, saying that it was
set before the other settings failed to apply and giving the directory; and nothing else.

When they were applied, a card for each section applied, grouped and headed as in the review (see "Settings in the
review" in `docs/product/configuration-export-and-import-dialogs.md`), with a line for each setting applied:

- each switch badged **On** or **Off**;
- When to prevent sleep, when the daemon applied the file's choice, badged with that choice, worded as in the review;
- each setting only a client holds that this client applied, badged with the value applied, worded as in the review;
- Language, when the daemon applied the file's language, badged with the language applied, worded as in the review;
- Sorting, when this client applied any of its parts, badged **Matches the file** and listing each part applied with
  its choice;
- the default clone directory, when the file's was written, badged **Matches the file**, with the directory under it;
- one line for the git workflow settings, badged **Matches the file**, listing all five, one per row, the item and the
  value applied;
- and, when this device applied the file's keyboard shortcuts, the **Changed shortcuts** line, badged with how many
  changes from the defaults its shortcuts now have, or **Defaults**, with how many of the file's shortcuts could not be
  used here and were left out said under it, when any were, and then each shortcut applied, as key caps — none for
  Defaults.

This tab has no totals.
