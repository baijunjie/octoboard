# Fixtures runtime

The stand-in for a daemon and a machine, shared by the UI state gallery (`packages/ui/src/gallery/`), the website
preview (`packages/ui/src/website-preview/`) and the application tests. It runs the real `App` with no daemon behind it.
It is excluded from the production type check and from release verification, and imports nothing from
`packages/ui/src/gallery/` (enforced by `eslint.config.mjs`).

## How it works

- `fixtureDaemon.ts` implements the store's `Daemon` interface: it answers each request from a `Fixture` (`fixture.ts`)
  and broadcasts the events the real daemon would, including the intermediate states of operations (a session switching
  account, a model change in flight). It answers a request only if some scenario sends it. It holds no replies of its
  own: `createFixtureContext` (`fixtureContext.ts`) builds what the domain modules share (the scenario, the store, `fail`,
  the refusals more than one needs), and each module answers the requests of one domain, as a `Handlers` table that
  `mergeHandlers` joins (a request answered by two modules throws; a `Handler` is typed to reply with the event its
  request is answered with, as `REPLY_TYPES` of `daemon-client.ts` names it, so a wrong reply fails the type check, and
  `neverReplied` is the reply of a request a scenario holds pending): `fixtureSessions.ts` (sessions),
  `fixtureConsoles.ts`, `fixtureAccounts.ts` (accounts, and the refusals a session gets on one),
  `fixtureAgentClis.ts` (the agents' CLI versions: the up-to-date statuses a window starts with, and an update), `fixtureSettings.ts`,
  `fixtureProjects.ts` (projects and their shell terminals), `fixtureFiles.ts` (a project's files, folder listings, path
  links) and `fixtureProcesses.ts` (the process ledger). A request no module answers is acknowledged. A new reply goes
  into the module of its domain, not into `fixtureDaemon.ts`. `BROWSE_REQUESTS` is the set of requests that read a
  project's folder (`GIT_REQUESTS` of `fixtureGit.ts` and `FILE_REQUESTS` of `fixtureFiles.ts`, each typed against the
  replies it has), which the website preview routes to the repository a project belongs to; a new browse request is
  routed correctly once it is in one of those two lists.
- `changeFixtures.ts` is the vocabulary of repository fixtures: one factory per kind of change, whose parameters carry
  exactly what the real reply for that kind needs, so an unanswerable change cannot be written. Where a reply cannot be
  filled faithfully the fixture daemon refuses it with `FixtureUnfillableError` (`banner.ts`'s `refuseUnfillable`)
  rather than sending the field unset. `fullContextPatch` builds the whole-file patch of a display fixture.
  `fixtureGit.ts` answers Git mode from them; `sampleGit.ts`, `sampleFiles.ts` and `viewerImages.ts` are the sample
  repository and files; `invariants.ts` holds rules over a whole repository that no single change shows
  (`objectIdCheck`), checked by a test.
- `fakeTerminal.ts` answers terminal sockets in the page (output, mouse, full screen, cursor beside reversed text, shell,
  exited, refused). The cursor sample uses ANSI reverse video and positions the real terminal cursor on the next
  character; it does not create a terminal selection or emulate copying.
  `builders.ts` builds protocol records and `snapshotState` builds a store state. `interact.ts` is the step driver
  (`ui.press`, `ui.t`, `ui.chord`, ...) that names controls as a user does, in the language the window is in.
  `fixtureProviders.ts` holds the providers the daemon serves in its snapshot (`FIXTURE_PROVIDERS`, with the real daemon's
  values), which `snapshotState` and the fixture daemon's store start with, and `servedLabel` for the name the daemon writes
  into its own text; UI text names a provider through `providerLabel` instead. `fixtureProviders.test.ts` holds that copy to the
  daemon's descriptors (`apps/daemon/src/providers/`), so a descriptor that changes there fails it until the copy follows.
  `UNPRESENTED_PROVIDER` is a provider with no presentation module, which a scenario serves beside the registered ones.
- `fixtureConfiguration.ts` answers the export, inspection and import of configuration. An import's progress is the scenario's
  `ConfigurationFixture.progress`: the items that have left `waiting`, as the last report of each says. The fixture daemon
  announces every item the request makes it handle `waiting` first, then sends those reports, to the listener of the
  request's `import_id` (`Daemon.onImportProgress`) and before the reply, which `imported: "pending"` holds back to keep the
  import part-way. It refuses a list the daemon's order of work (the default clone directory, the accounts and then the
  Agent accounts settings, each console with its projects, then the application settings, one at a time) cannot have
  produced (`importProgressDisagreement`): a console is `done` as soon as it is created,
  its projects following for themselves (or `failed`, with each project it announced `not_attempted`), and an account that
  exists already is announced whatever was chosen for it (`handledImportItems`).
- `pageFrame.ts` and `preferences.ts` are the page set-up the gallery and the website preview do before the app loads;
  they import nothing from the app that reads a stored preference at load.
- Sample data shared by scenarios and tests: `archiveSessions.ts`, `configurationReview.ts`, `restoreSample.ts`,
  `quotas.ts`, `agentModels.ts`, and `accountTestSupport.ts` (the two accounts the dialog tests share).

## Conventions for teaching the fixture daemon a reply

The fixture stands on the daemon's side of the protocol, so a reply it sends must be one the real daemon could send.

- Write a fixture daemon handler only for a request some scenario sends, and delete one left unused: no scenario renders
  its reply and no guard test reads it, so it is an unfaithful reply nothing can catch.
- When a reply gains a field the UI draws from, teach the fixture daemon the condition the real daemon fills the field
  under, and give every scenario's fixture what that condition needs, scenarios that answer with an error included (an
  unfilled field there is a state the real daemon never sends). Take the condition from the daemon's own code and tests
  (`apps/daemon/src/`), never from a list of cases in a doc or a plan, which can lag the rule; and never import a UI
  helper that happens to compute the same thing: repeat the rule inside this directory (as `lineCount` in
  `fixtureGit.ts` repeats the daemon's count), since UI code is free to change for the UI's reasons.
- Hold the condition in a signature before holding it at run time. A field the reply gains goes first into the
  parameters of the `changeFixtures.ts` factories whose kind of change the real daemon fills it for, because an unfilled
  field is a legal value of the reply and only a parameter that has to be passed stops an unanswerable fixture being
  written. Leave to the run-time refusal only a condition that depends on the data rather than on the kind of change.
  Extend `refuseUnfillable` (`banner.ts`) to each new field of this kind, and have the guard test over the fixture
  daemon's replies read every scenario's reply, those carrying no such field included, so a refusal fails the test run
  and not only the scenario someone opens. That test is `gallery/fixtures/projectGit.test.ts`, "answer every read a
  scenario's repository offers without a refusal": it takes the repositories from `SCENARIOS` (the sample one every
  scenario without a `git` is answered from included), so a new fixture is read without being named; it lets a daemon
  error through only where the fixture stages that very error for the change, and checks that each read reached its
  target.
- `FixtureUnfillableError` is deliberately not a `CodedError`: a scenario's daemon error is a legitimate UI
  state to show, so a gap in the gallery's own data must not be dressed as one that a test, or someone looking at the
  page, would let pass.
- A fixture field says what a reply costs (`expand.delay`, `expand.error`), never whether it is given. Whether the
  daemon answers at all follows from the change's own data (its group, its sides, whether making its patch already
  failed), and the fixture daemon reads it from there; gating on `expand`, which only some scenarios carry, would refuse
  a deletion, a binary change or a change past the patch budget with an `invalid_change` the real daemon never sends
  there. Likewise a factory parameter carries the content a reply needs, not the right to ask for it.
- Where the real daemon counts a change side's `lines`, the fixture carries the text to count it from
  (`FixtureChange.whole`), or the request rejects with `FixtureUnfillableError`. Change fixtures are therefore written
  through the factories (`textChange`, `deletion`, `untracked`, ...), never as literals. A rule over a whole repository
  that no single change shows is added to `invariants.ts`.

## External interfaces

- `createFixtureDaemon` (`fixtureDaemon.ts`) and the `Fixture` type (`fixture.ts`).
- `snapshotState`, `consoleWith`, `projectWith` and the other builders (`builders.ts`); a `Map` of records is given its
  type so a removed protocol field cannot survive in a fixture.
- `installFakeTerminal` (`fakeTerminal.ts`).
- `createUi`, `Ui` and `Step` (`interact.ts`) — the step driver.
- The factories of `changeFixtures.ts` (`textChange`, `deletion`, `untracked`, ...) — the only way a change fixture is
  written.
- `applyDocumentTheme` and `afterFrames` (`pageFrame.ts`); `writeStartingPreferences` (`preferences.ts`).
