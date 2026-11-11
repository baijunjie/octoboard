# Concurrent edits

The same setting can be changed from several places at once: two windows (or a window and a browser) on the same
daemon, a console session through the settings tools, a single action such as a rename or a pin, and a configuration
import. A change made through a form is based on what the form read, and the daemon refuses it when a field it changes
has been changed since, so that nobody overwrites a change they never saw. Two changes to different fields of the same
record both succeed.

## The records and fields that are checked

Four kinds of record are edited through a form, and each keeps a version for every field a form edits:

- **the application settings**: every setting Settings stores in the daemon — each switch, the interface language, when
  Prevent sleep holds, the default clone directory, each of the five git workflow settings, each agent's tags and each setting an agent's
  provider declares;
- **a console**: its name, both default agents, its avatar, its console session instructions, and for each agent its
  account and its console session and project session model, reasoning effort and each compatibility switch;
- **a project**: its name, default agent, pin, tags, description, its own git workflow settings item by item, and for
  each agent its account, model, reasoning effort and each compatibility switch;
- **an account**: its name, its directory, whether it is turned off and whether it shares the default configuration.

A field's version advances whenever the field's value changes, whoever changed it: a form, a single action, an agent's
settings tool or an import.

**Checked** — refused when a field they change has moved past the version they were based on:

- the console dialog, the project dialog when editing a project, and the account dialog when editing an account;
- the Settings switches whose value the daemon holds, the When to prevent sleep, Where sessions develop and Delivery
  choices, the default clone directory and each agent's tags;
- a configuration import that applies settings (see "Applying the import" in
  `docs/product/configuration-export-and-import-dialogs.md`).

**Not checked** — the change is made over whatever is there, and still advances the field's version, so a form based
on the old value is refused afterwards:

- single actions: renaming a project with Rename, pinning or unpinning it, naming a default account, turning an
  account off or on, changing an account's sharing, changing a provider setting (an agent's permission approval or
  Claude compatibility), and the interface language, which a Language choice in Settings takes at once and every window
  then follows;
- every change a console session makes through the settings tools (see `docs/product/agent-settings-tools.md`).

## The version check

Every record of those four kinds carries its fields' versions wherever the daemon sends the record — its state at
connection and every event that carries the record. A checked write names, for each field it changes, the version it
was based on. When any of them differs from the field's current version, **the whole write is refused** and nothing of
it is applied, with the code `write_conflict`, which names the fields whose versions differ and their current versions.
A client that shows the current values and lets the user change them again is then based on the current versions and is
not refused again. A write that names no versions is not checked. Naming a field the record does not have (an agent
that is not registered, say) is refused as an unreadable request.

Checked writes are applied one at a time, so two of them cannot both pass the check. A write that has to wait before it
is applied — for the agent's models to be read, or for a sharing account's directory to be let go — is checked after
the wait, just before it is applied; pointing a sharing account at another directory may therefore already have turned
its sharing off when the check then refuses it.

Versions hold only for one run of the daemon. After the daemon restarts, every version read before it is refused,
since every field's version has moved on, unless the system clock was set back between the two runs. A deleted record
keeps its versions, so a write based on it from before the deletion cannot recreate it unnoticed.

## A form while its record changes elsewhere

A form — the console, project and account dialogs, and a Settings field — sends only the fields the user changed, so
a change made elsewhere to a field the user left alone is neither overwritten nor a conflict.

While the form is open, a field that changes elsewhere to a value other than the one the form read **shows the current
value at once**, whether or not the user had edited it, and the form is based on the new version from then on. The
user's edits to the other fields stay. A field whose version moves while its value stays the same — as every field's
does after a daemon restart — keeps the user's edit; the form is only based on the new version.

When a field the user had edited is replaced, assistive technology is told, naming the fields by their labels in the
form: that they were changed elsewhere and that the current value, or values, are shown. Nothing marks it on the
screen. A field the user had not touched is replaced without a word, as is one replaced while the form's own save is
out, whose outcome the form reports itself.

Only a change that reaches the daemon before it reaches the window is refused at the save. The refusal opens the error
alert, under the title of the form's own failure, saying that a change was made elsewhere after the user last saw it, so
nothing was saved, and to check the current value and make the change again (see "The error alert" in
`docs/product/error-alerts.md`). The form stays open under it; once the window holds the change, each field the refusal
names whose value differs from what the form read shows the current value, and the user's other edits stay as they
were. That replacement is not announced, as the error alert speaks for it.

How each kind of Settings control saves under this rule is with the control: the switches, the choice of when to prevent
sleep and the git workflow choices in "How switches save" in `docs/product/settings.md`, the default clone directory in "Default clone directory" in `docs/product/git-settings.md`,
and the tags in "Agent tags" in `docs/product/per-agent-settings.md`. A window's own quick successive changes to one of
them never conflict with one another.
