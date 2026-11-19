# Release a provider

> Goal: turn one finished preview provider into a released agent: lift its gate and update, for that provider, every
> shared file its own milestone was kept away from.
> Completion criteria: a release build of the daemon serves the provider; the window, the product docs, the reference
> document's cross-agent sections, the gallery and the website each name and describe it; the full checks and the
> gallery sweep pass; and on a real account the provider's sessions pass the acceptance run below.

The overview is `README.md` in this directory. This milestone is carried out once for each of the five, after that
provider's own milestone is merged, and one provider at a time, since every item edits a file the other four also
need. The five to tick:

- [ ] Pi (`05-pi-provider.md`)
- [ ] Qwen Code (`06-qwen-code-provider.md`)
- [ ] GitHub Copilot CLI (`07-copilot-cli-provider.md`)
- [ ] Gemini CLI (`08-gemini-cli-provider.md`)
- [ ] OpenCode (`09-opencode-provider.md`)

The provider's mark and colour are not part of this milestone: the user confirmed them when the five were registered
(`04-register-preview-providers.md`, "Marks and colours").

## Technical design

- [ ] The provider's two registry lines and two module declarations lose the preview gate. Its place in the registry
  order does not change.
- [ ] The fixture daemon serves the provider, and the gallery gains the scenarios that show what is particular to it:
  its accounts section, its settings, its sign-in hint or login, a session of it in each status.
- [ ] The cross-agent sections and tables of `docs/agent-cli-reference.md` gain the provider, and the document's
  opening and its entry in `docs/README.md` stop counting three agents.
- [ ] The product docs state the provider wherever they state the agents one by one: the supported agents and their
  order, how each is given the injection and what each does differently at launch, what the statuses are derived
  from and the turns its signals do not report, the trust confirmations, the keys an interrupt sends, how each signs
  in, which accounts have a quota, the provider's settings, and how its CLI's install and update are told.
- [ ] `docs/conventions/provider-neutral-core.md` names the provider where it names the agents and their spellings to
  search for.
- [ ] The website, the documentation site and the repository's README list the provider among the supported agents,
  in every language they are written in. The website draws the provider's mark from the list of agent marks, which
  already holds it.

## Implementation plan

- [ ] Lift the gate and build a release daemon. Verified by its served providers and by the full checks.
- [ ] Add the provider to the fixture daemon and write its gallery scenarios; run the gallery's quick check and
  sweep.
- [ ] Update the reference document's shared sections, the documentation index and the convention doc.
- [ ] Update the product docs, through the product writer, from the provider's section of the reference document and
  its merged behaviour.
- [ ] Update the website, the documentation site and the README.
- [ ] Acceptance run on a real account, with the user: add the agent in Settings; open a console session and have it
  open a project session; watch the project session work, raise its hand at a permission prompt and at a question,
  finish and report; send it an instruction while it works; interrupt a turn from its parent; archive and resume it;
  switch the appearance and the language once; confirm the user's own configuration of the agent is as it was.

## Notes for the developer

**Reusable capabilities**

- The gallery, its fixture daemon and its sweep, for showing and checking a provider's states.
- The served-info fixtures from `01-provider-neutral-repairs.md`, which the fixture daemon's provider list is built
  from.
- The website's own list of the agents it shows, updated by hand; it draws each one's mark from the list of agent
  marks by id.

**Development notes**

- Product docs describe what a release does; they were deliberately not touched while the provider was a preview.
- If a mark or colour confirmed at registration reads badly beside the provider's real states in the gallery, show
  the user and change it only on their word.
- Changes to the documentation site follow its own conventions on quoted values and product names.
- The acceptance run uses the user's real account: nothing in it spends more than a few turns, and nothing changes
  the account.

**Reference docs**

- `docs/conventions/provider-neutral-core.md`, "UI" and "Checking a change against this rule"
- `docs/conventions/tests.md`, "The UI state gallery and its fixtures"
- `docs/conventions/ui-standard.md`
- `docs/conventions/website.md` and `docs/conventions/docs-site.md`
- `docs/product/supported-agents.md`
- `docs/product/per-agent-launch-specifics.md`
- `docs/product/session-statuses.md`, "What the statuses are derived from"
- `docs/product/folder-trust.md`, "The agents' trust confirmations"
- `docs/product/interrupting-turns.md`, "The keys each agent gets"
- `docs/product/logging-accounts-in.md`, "How each agent signs in"
- `docs/product/account-quotas.md`, "Which accounts have a quota"
- `docs/product/per-agent-settings.md`
- `docs/product/agent-cli-updates.md`
