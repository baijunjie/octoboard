# Terminology: console sessions and project sessions

> Goal: one vocabulary for the two kinds of session, named on a single axis, with no trace of the singleton
> phrasing the old name carried. No behaviour changes.
> Completion criteria: the project's static checks and its whole test suite pass, and a search of the repository's
> prose, identifiers and user-facing strings finds "hub" only where it is the default title given to a new console
> session. File names are excluded; see the note below. The application behaves exactly as before.

A rename ahead of everything else, so that the milestones that follow are written and read in the settled
vocabulary. It is deliberately behaviour-free and mergeable on its own.

## Technical design

- [ ] The session role's two values are named for what the session belongs to: a **console session** and a
  **project session**. The role value that today names the orchestrating session, and the one that names a working
  session, are both renamed.
- [ ] `origin`'s value for "started by the orchestrating session" is renamed to match.
- [ ] Prose terms settled and used consistently from here on:
  - **console session** — a session that orchestrates. It belongs to a console, runs in the console's working
    directory, and carries no project.
  - **project session** — a session that works in a project.
  - **bound** / **unbound** — whether a project session reports to a console session. Replaces "include in hub" and
    "inside / outside the orchestration".
  - A bound session's **owner** is the console session it is bound to.
- [ ] "Hub" is retained in exactly one place: the default title given to a newly created console session. It is a
  title, not a type name, and the user may rename it.
- [ ] The product doc whose file name contains "hub" keeps that name. A file name is neither prose nor an
  identifier, and renaming it would churn the documentation index and every cross-reference for no reader's benefit.
  Whether it is renamed later is left open in the overview.
- [ ] No noun phrase treats a console as having one console session. Anything of the form "the console's
  orchestrating session" is rewritten to name a particular console session.

## Implementation plan

- [ ] Rename the role and origin values, and every identifier derived from them, across the daemon, the protocol and
  the application.
- [ ] Rewrite the user-facing strings in every locale the project ships, following the project's copy conventions.
- [ ] Rewrite the product documentation that describes the two kinds of session, their listing, their reporting and
  their archive, into the new vocabulary — still describing today's single-console-session behaviour, which later
  milestones change.
- [ ] Leave the membership boolean's name alone. It is deleted outright in the next milestone, so renaming it here
  would be thrown away.

## Notes for the developer

**Reusable capabilities**

- The project already has a localization resource set per shipped language and a documented copy convention; the
  strings go through those rather than being written inline.

**Development notes**

- All prose committed to the repository is in English, localization resources excepted.
- The protocol between the daemon and the application is contract-checked; a renamed enum value is a protocol change
  and both sides plus the contract must move together.

**Reference docs**

- `docs/product/sessions.md`, `docs/product/hub-orchestration.md`, `docs/product/sidebar.md`
- `docs/architecture.md`, `apps/daemon/PROTOCOL.md`
- The project's UI copy conventions skill, `knowledge-i18n-copy`
