# Issue reports

**Report issue**, offered on an alert about an unexpected failure (see "Recovery and unexpected errors" in
`docs/product/error-alerts.md`), opens a new Issue on the issue address configured as `issueUrl` in `config/app.json`,
prefilled with a title and a body that describe the failure. The prefill is passed as GitHub's `title` and `body` query
parameters added to that address; nothing else is attached, no screenshot and no file. The Issue is not created until
the user submits it on the Issue page, where the prefilled text can be edited first.

The same prefill is used wherever **Report issue** is offered: the error alert, and the account login failure dialog
when its failure is unexpected (see "When a login fails" in `docs/product/logging-accounts-in.md`).

## Which failures a report describes

A report describes the failures the alert is about: each failure it lists, or else its one failure. An alert that has
no error of its own but shows raw text under **Error details** — a UI that failed, a daemon that exited — is
described by that text as its failure. The login failure dialog's report describes the daemon's own failure, not the
sentence the dialog words for the user.

The failure an alert stands for as a whole is unexpected; each listed failure keeps its own classification, so a batch
of failures can mix unexpected and actionable items. Identical entries are listed once.

## The prefilled title

The title is `Unexpected error: <summary>`, the summary taken from the first unexpected failure:

- a failure the daemon reported with a code: the code itself, such as `git_failed`, except for the generic codes
  `internal_error` and `unexpected_error`, which use the first line of their raw text instead (or the code when that
  text is empty);
- any other error: the first line of its message, or its name when it has no message.

A summary from text is sanitized as described in "What is removed from a report" below and cut to 80 characters,
ending in an ellipsis when cut. With no summary the title is `Unexpected error` alone.

## The prefilled body

The body is Markdown in three sections:

- **What happened**: a hidden comment asking what the user was doing and what they expected instead; in the desktop
  application only, a second hidden comment saying that the application keeps its logs in `~/.octoboard/logs/`
  (`daemon.log`, `desktop.log`, see "Files Octoboard owns" in `docs/product/application-lifecycle.md`) and that a
  maintainer may ask about them if more detail is needed — the logs themselves are never attached; then a line saying
  the details below were filled in automatically with personal information removed, and asking the user to check them
  before submitting.
- **Environment**: one line each, where it is known — the application's name and, in the desktop application, the
  version running; the platform, `desktop app` or `browser`; the operating system's family and version as the user
  agent names them (such as `macOS 10.15.7`), nothing else of the device; and the UI's current language as its code
  (such as `zh-Hans`). With none known, the section says `Unknown`.
- **Error**: one entry per failure:
  - its kind: `unexpected`, or `actionable (<remedy>)` with the name of the remedy kind it was classified under;
  - for a failure the daemon reported with a code, the code and the names of its parameters (at most 20, sorted),
    never their values;
  - for an unexpected failure only, its raw text, sanitized, as a code block: for a failure the daemon coded, the raw
    text its alert shows under **Error details**, where it has one; for an error of the window's own, its stack led by
    its name and message; for an alert described by its raw text, that text.

  With no failure to describe, the section says no error details were captured.

A report never includes the alert's title, message or body as the UI words them, the session it concerns, or a
parameter's value.

## What is removed from a report

Every text from a failure — a raw text in the body and the text a title summary is taken from — is sanitized before
it is put in. The removal is best effort and errs toward removing too much:

- **Names of the user's records** the window knows: console and project names, session titles and terminal titles,
  shell terminal titles, branch names and account names become `<name>`. Project paths, project remote URLs and account
  configuration folders are replaced whole, a path as `<path>`, also in its `~` form. A name shorter than two
  characters is left alone.
- **The home directory** becomes `~`: the user's own, and any directory of the shape `/Users/<user>`,
  `/home/<user>` or `<drive>:\Users\<user>`.
- **Other absolute paths** become `<path>`, `file://` URLs included. A path with a space in it that no record name
  covers is replaced only up to its first space.
- **URLs** become `<url>`, as do `user@host:path` remote addresses, except the application's own (on `localhost`,
  a loopback address or the desktop application's own hosts, with no user name in them), which are kept without
  their query and fragment, with a home directory in their path collapsed to `~` and a stack frame's line and column
  kept.
- **Email addresses** become `<email>`; **IP addresses** other than loopback ones become `<ip>`; **UUIDs** become
  `<id>`.
- **Secrets** become `<redacted>`: bearer and basic credentials, API keys and access tokens of the known shapes, any
  run of 20 or more letters and digits mixing both, the value of a cookie, and the value assigned to any key whose name
  holds a word such as token, secret, password, API key, access key, private key, authorization, credential or session
  ID.

## Length limits

Each text is cut to 16,000 characters before it is sanitized, ending in a note that it was truncated. The whole
prefilled address is kept within 7,000 characters: where it would be longer, the raw texts in the body are cut to the
longest head that fits, each cut one ending in the same note. The title is never cut for length beyond its summary's
80 characters.

## Where the bare address is used

The prefill belongs to the **Report issue** action alone. The issue address is shown bare, with no title or body, in:

- the alert for an Issue page that could not be opened, which shows the address to open manually (see "Recovery and
  unexpected errors" in `docs/product/error-alerts.md`);
- the guidance that names the address where the desktop application cannot open links;
- the **Issue tracker** link in Settings' About section (see "About" in `docs/product/settings.md`).
