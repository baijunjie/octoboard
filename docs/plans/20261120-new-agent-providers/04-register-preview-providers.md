# Register the five as preview providers

> Goal: register Gemini CLI, GitHub Copilot CLI, OpenCode, Qwen Code and Pi as bare providers behind the preview gate,
> each with its brand mark and colour, editing every shared file once, so that each provider's own milestone
> afterwards touches only that provider's files.
> Completion criteria: `pnpm test:rust`, `pnpm clippy:rust`, `pnpm check:windows`, the UI tests and
> `node scripts/verify/provider-neutral.mjs` pass with the five registered and no generic file changed beyond the
> registry lines, the list of agent marks and the check's vocabulary; a development build lists the five among the
> agents that can be added, each under its own mark, and starts and resumes a plain session of each one that is
> installed; the user has seen the five marks and colours in both appearances and confirmed each; a release build of
> the daemon compiles without warnings and serves only Claude Code, Codex and Grok Build.

The overview is `README.md` in this directory; the facts are in `cli-fact-sheets.md`. This milestone follows
`03-accounts-without-plan-quota-or-browser-sign-in.md`. The gate, the marks arriving here, and the ids and their order
are the user's decisions (`README.md`, "Key design decisions").

## Technical design

### What "bare" means

- [ ] A bare provider has a descriptor and an `AgentProvider` that answers only its descriptor and its launch plan.
  Everything else is the generic default: no hooks, no status integration, no records, no screen reader, no accounts
  operations, no model list, no settings, no codes.
- [ ] Its launch plan starts and resumes the CLI with the flags in the table below, the model and effort where the
  CLI has a flag, and its account directory exported through the layout's conversion. It injects nothing.
- [ ] A session of a bare provider is a terminal running the agent. It is what a development build offers until the
  provider's own milestone lands, and never what a release offers.

### The preview gate

- [ ] A preview provider's two registry lines and two module declarations carry `#[cfg(any(test, debug_assertions))]`,
  in the descriptor registry and in the behaviour registry alike.
- [ ] The provider-neutral check accepts a registration line with that attribute before it (provided by
  `01-provider-neutral-repairs.md`).
- [ ] Nothing in the UI is gated: the window lists what the daemon serves, and an unserved provider's presentation
  module, its mark included, is simply unused. The list of agent marks only answers the website by id, and the website
  names the agents it shows itself, so it shows none of the five until `10-release-a-provider.md`.

### Initial descriptors

The values are the fact sheets', unmeasured unless the sheet says otherwise. Each provider's own milestone confirms or
corrects them; they are written here so that the five files exist and compile.

| Fact | `gemini` | `copilot` | `opencode` | `qwen` | `pi` |
|---|---|---|---|---|---|
| Label, short name | Gemini CLI, Gemini | GitHub Copilot CLI, Copilot | OpenCode, OpenCode | Qwen Code, Qwen | Pi, Pi |
| Binary | `gemini` | `copilot` | `opencode` | `qwen` | `pi` |
| Directory variable, and what it names | `GEMINI_CLI_HOME`, the parent | `COPILOT_HOME`, the directory | `XDG_DATA_HOME`, the parent | `QWEN_HOME`, the directory | `PI_CODING_AGENT_DIR`, the directory |
| Default directory | `.gemini` | `.copilot` | `.local/share/opencode` | `.qwen` | `.pi/agent` |
| Records directory | `tmp` | `session-state` | none | `projects` | `sessions` |
| Markers of its own | `.gemini`, `GEMINI.md` | none | `.opencode`, `opencode.json`, `opencode.jsonc` | `.qwen`, `QWEN.md` | `.pi` |
| Generic markers it reads | none | `AGENTS.md` | `AGENTS.md` | `AGENTS.md`, `.agents` | `AGENTS.md`, `.agents` |
| Instruction file | `GEMINI.md` | `AGENTS.md` | `AGENTS.md` | `QWEN.md` | `AGENTS.md` |
| Tool name prefix and separator | `mcp_`, `_` | to be measured | none, `_` | `mcp__`, `__` | `mcp__`, `__` |
| Ask-the-user tool | `ask_user` | `ask_user` | none yet | `ask_user_question` | `ask_question` |
| Pre-allocates the session id | yes | yes | no | yes | yes |
| Accounts beyond the default | yes | yes | no | yes | yes |
| New session, first task | `--session-id`, positional | `--session-id=`, `-i` | none, `--prompt` | `--session-id`, `-i` | `--session-id`, positional |
| Resume | `--resume` | `--resume=` | `--session` | `--resume` | `--session` |
| Model, effort | `--model`, none | `--model`, `--reasoning-effort` | `--model`, none | `--model`, none | `--model`, `--thinking` |

- [ ] Every capability not in the table is declared off, the quota windows are none, model ids are shown as they
  are, and there are no settings, codes or inherited session variables yet.
- [ ] They are registered after Grok Build in the order `gemini`, `copilot`, `opencode`, `qwen`, `pi`.

### Marks and colours

- [ ] Each of the five presentation modules carries the provider's own mark: the vendor's published mark, redrawn as
  one path on the 24-unit square with one fill, as the three existing marks are. A mark the brand draws in a single
  dark or light tone takes the text colour (`currentColor`), which also makes it a template image in the menu bar; a
  mark with a colour of its own keeps that colour as its fill.
- [ ] Each module carries a colour, a light and a dark value, where the brand has one that can stand for the agent on
  a usage bar; a provider without one draws its bars in the text colour, as Grok Build does. A colour is held to the
  UI standard's rule for a usage bar's fill against every track, in both appearances, and can be told apart from
  Claude Code's orange, Codex's blue and the other new colours.
- [ ] `packages/ui/src/providers/agentMarks.ts` gains the five marks, by id.
- [ ] A test beside each module pins its mark's fill and its colour, as the registry test pins the first three's.
- [ ] A gallery sheet draws every built-in provider's mark at the sizes the window draws marks, and its colour on a
  compact usage bar and on a meter, in both appearances, the three existing providers beside the five for
  comparison. It is what the user is shown; whether it stays in the gallery afterwards is the user's answer.
- [ ] The user confirms each mark and each colour, or sends it back, before this milestone merges. A mark that cannot
  be drawn as one path with one fill (a gradient, several colours) is shown in its single-colour form and the
  difference is said; drawing more than that is a change to what a mark is, to be reported, not made here.

### The other shared edits

- [ ] UI: a presentation module per provider with its names, its mark and colour as above, its served-info fixture,
  and the five lines in the built-in list.
- [ ] The provider-neutral check: a vocabulary entry per provider (product name, dot-directory, instruction file,
  environment variable prefix). For `pi`, only spellings that cannot collide with ordinary code: the quoted id, the
  id constant, its dot-directory and its variable prefix.
- [ ] `docs/agent-cli-reference.md`: five section headings, "Gemini CLI", "GitHub Copilot CLI", "OpenCode", "Qwen
  Code" and "Pi", after Grok Build's section, each holding one line saying it is a preview provider not yet measured.
  The document's cross-agent sections and its opening are left for `10-release-a-provider.md`.

## Implementation plan

- [ ] Write the five descriptors and their bare behaviours, with a descriptor test each that pins the layout names and
  the launch arguments of a new session, a resume, a model and a task. Verified by those tests and by the registry
  tests that already walk every provider.
- [ ] Add the registry lines and module declarations with the gate. Verified by the registry-agreement test, and by a
  release build of the daemon whose served providers are the three.
- [ ] Add the five presentation modules, their fixtures and the built-in lines. Verified by the registry tests moved
  onto the built-in list in `01-provider-neutral-repairs.md`.
- [ ] Draw the five marks and choose the colours, add the marks to the list of agent marks, and pin each in its
  module's own test. Verified by those tests and by measuring each colour against the tracks the UI standard names.
- [ ] Add the gallery sheet of marks and colours and show it to the user through the gallery preview, in the light
  and the dark appearance; in the desktop application, show the five in the menu bar icon's menu as well. Wait for the
  user's answer on every mark and colour, redraw what is sent back, and show it again until each is confirmed.
- [ ] Fill the check's vocabulary and add a case to its test that an id of each new provider in generic code is
  reported.
- [ ] Add the five headings to the reference document.
- [ ] In a development build, with each CLI installed: add the agent, open a session, see the CLI's own first screen
  in the terminal, archive it and resume it. A CLI that stops at its sign-in screen is the expected result; nothing is
  signed in for this milestone.

## Notes for the developer

**Reusable capabilities**

- The test-only provider's descriptor and behaviour as the template of a bare provider: it declares nothing beyond
  what every provider must.
- The layout's conversions from `02-descriptor-facts-for-new-agent-shapes.md` for exporting an account's directory;
  `pinned_config_dir` for refusing a pinned directory that has gone before a resume.
- The served-info fixtures from `01-provider-neutral-repairs.md`.
- The agent icon, which draws any provider's mark from the registry, and the usage bar and meter, which take a
  provider's colour from it; the gallery's existing sheets (semantic colours, buttons) as the pattern for a sheet
  that shows one kind of thing in every variant.

**Development notes**

- A brand name is the same in every language; the three existing modules show how names are shared across catalogs.
- A provider's colour is written down once, in its presentation module; the native colours of the menu bar icon's
  quota bars are generated from it, never written a second time.
- A mark is a vendor's trademark used to name its product: redraw its shape faithfully, and do not invent a mark for
  a provider whose vendor publishes none; say so and show the alternatives to the user instead.
- The gallery preview and the wait for the user's confirmation follow the `agent-gallery` skill; the confirmation is
  required here, not left to its judgement.
- A gated module must not leave unused code behind in a release build: gate the module declaration, not only the
  registry line.
- A database a development build wrote names agents a release build lacks. That case is already defined
  (`docs/product/supported-agents.md`, "Data of an agent this build does not support"); do not add handling for it.
- Do not run any of the five against the user's own configuration while trying a session: use a scratch home, and for
  Gemini CLI note that even `--help` writes into its directory.

**Reference docs**

- `docs/conventions/provider-neutral-core.md`, "General principles" and "Checking a change against this rule"
- `docs/structure/daemon.md`, "Where new code goes"
- `docs/structure/ui.md`, "Where new code goes"
- `docs/conventions/ui-standard.md`, "Colour", and `docs/conventions/ui-accessibility.md`, "Not by colour alone"
- `docs/conventions/tests.md`, "The UI state gallery and its fixtures"
- `docs/product/supported-agents.md`, "How an agent is named and marked"
- `docs/product/account-quotas.md`, "How an account's quota is shown"
- `docs/product/agent-accounts.md`, "Added agents" and "Adding an agent"
- `cli-fact-sheets.md` in this directory
