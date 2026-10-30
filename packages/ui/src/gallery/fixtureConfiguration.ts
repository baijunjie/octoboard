// What the fixture daemon answers exporting and importing configuration with. An export is built
// from the scenario's own state, as the daemon builds one from what it stores, so the document a
// scenario saves is the one its consoles and projects would export. An inspection and an import
// result are what the scenario says: they depend on the filesystem and on the clones the real
// daemon makes, which the gallery has none of, so a scenario that needs one gives it (and one that
// does not give it is refused rather than answered with a made-up one). What a result says of the
// request it answers is held to the request, so a scenario whose steps choose otherwise than its
// result assumes is refused rather than shown as if the choice had gone the way the result says.
import type {
  Account,
  AccountName,
  Agent,
  ConfigurationDocument,
  DocumentAccounts,
  DocumentConsole,
  DocumentProject,
  DocumentProjectAccount,
  DocumentSettings,
  Event,
  PerAgent,
  Project,
  ProjectAccount,
  RequestBody,
} from "../protocol";
import type { State } from "../store";
import { refuseUnfillable } from "./banner";
import type { FixtureError } from "./fixtures/projectFiles";
import type { ConfigurationFixture } from "./scenario";

/** The last segment of `path`, or an empty string where it has none (the filesystem root). */
const directoryName = (path: string): string => path.split("/").filter(Boolean).at(-1) ?? "";

/** Whether a remote can be cloned from on another machine: a URL of a network scheme, or the
 * `host:path` form, and not a local path or a `file://` URL. */
const isClonableRemote = (url: string): boolean =>
  /^(https?|ssh|git|git\+ssh|ssh\+git):\/\/\S/i.test(url) || /^[^\s/:]+:(?![:/]{1,2})\S/.test(url);

type ExportRequest = Extract<RequestBody, { type: "export_configuration" }>;

/** The regions an export holds, settled as the daemon settles them: `consoles` absent is `one` when a console is named
 * and `all` otherwise, the settings are in exactly when every console is unless said, and the accounts only when asked. */
function exportedRegions(request: ExportRequest): { consoles: NonNullable<ExportRequest["consoles"]>; settings: boolean; accounts: boolean } {
  const consoles = request.consoles ?? (request.console !== undefined ? "one" : "all");
  return { consoles, settings: request.include_settings ?? consoles === "all", accounts: request.include_accounts ?? false };
}

/** Why the daemon refuses an export request, before it reads anything: nothing selected, `one` with no console named, or a
 * console it does not have. `undefined` for one it answers. */
export function exportRefusal(state: State, request: ExportRequest): FixtureError | undefined {
  const { consoles, settings, accounts } = exportedRegions(request);
  if (consoles === "none" && !settings && !accounts) return { code: "export_empty", params: {}, message: "nothing is selected to export" };
  if (consoles !== "one") return undefined;
  if (request.console === undefined) return { code: "field_required", params: { field: "console" }, message: "console is required" };
  if (!state.consoles.has(request.console)) return { code: "unknown_console", params: { console: request.console }, message: `unknown console ${request.console}` };
  return undefined;
}

/** An account's directory as a document writes it: `~/<relative>` under the host's home directory (`~` for the directory
 * itself), so no user name leaks; elsewhere the full path. The export's paths choice does not touch it: an account cannot
 * exist without its directory. */
function documentDirectory(directory: string, home: string | null): string {
  if (home !== null && directory === home) return "~";
  if (home !== null && directory.startsWith(`${home}/`)) return `~/${directory.slice(home.length + 1)}`;
  return directory;
}

/** The accounts region: the added agents and, for each agent with an account of its own or a named default account, the
 * default account's name and place in the display order (the accounts before it) and the accounts the user owns in that
 * order (the saved order, then those added since). Never an id, a credential, the default account's directory or its
 * disabled flag. */
function exportedAccounts(state: State): DocumentAccounts {
  const { settings } = state;
  const agents: DocumentAccounts["agents"] = [];
  for (const agent of ["claude", "codex", "grok"] as const) {
    const owned = settings.accounts.filter((account) => account.agent === agent);
    const name = settings.default_account_names?.[agent];
    if (owned.length === 0 && !name) continue;
    const byId = new Map<string | null, Account | null>([[null, null], ...owned.map((account): [string, Account] => [account.id, account])]);
    const display: (Account | null)[] = [];
    for (const id of [...(settings.account_order?.[agent] ?? []), null, ...owned.map((account) => account.id)]) {
      if (!byId.has(id)) continue;
      display.push(byId.get(id)!);
      byId.delete(id);
    }
    agents.push({
      agent,
      default_account: { name: name ?? null, position: display.indexOf(null) },
      accounts: display.flatMap((account) =>
        account
          ? [{ name: account.name, directory: documentDirectory(account.config_dir, state.homeDir), share_default_config: account.share_default_config, disabled: account.disabled }]
          : [],
      ),
    });
  }
  return { ...(settings.added_agents !== null && { added_agents: settings.added_agents }), agents };
}

/** One per-agent group with `map` applied to each agent's value. */
function mappedAgents<T, U>(group: PerAgent<T>, map: (value: T, agent: Agent) => U): PerAgent<U> {
  return Object.fromEntries((["claude", "codex", "grok"] as const).map((agent) => [agent, map(group[agent], agent)])) as PerAgent<U>;
}

/** The document `export_configuration` answers: the regions the request chooses (see `exportedRegions`): the console named
 * or every console or none, the application settings, the accounts; consoles by creation time and a console's projects by
 * name, with no id and no console working directory in it, and an account carried by its name alone. Without
 * `include_paths` a project's path is only its last segment and a project's remote is kept only where it can be cloned
 * from; an account directory is written whatever it says. A document with the settings carries the user's explicit Claude
 * compatibility `choices` (not the values in effect, which `state.settings` holds), `{}` when the user chose for no agent. */
export function exportedDocument(
  state: State,
  request: Omit<ExportRequest, "type"> = {},
  choices: Partial<Record<Agent, boolean>> = {},
): ConfigurationDocument {
  const regions = exportedRegions({ type: "export_configuration", ...request });
  const includePaths = request.include_paths ?? true;
  const accountName = (agent: Agent, id: string | null | undefined): AccountName | null => {
    const account = state.settings.accounts.find((candidate) => candidate.id === id && candidate.agent === agent);
    return account ? { name: account.name } : null;
  };
  const projectAccount = (agent: Agent, reference: ProjectAccount): DocumentProjectAccount => {
    if (reference.kind !== "account") return reference;
    const account = accountName(agent, reference.id);
    return account ? { kind: "account", ...account } : { kind: "inherit" };
  };
  const documentProject = (project: Project): DocumentProject => ({
    name: project.name,
    path: includePaths ? project.path : directoryName(project.path),
    source: project.source,
    remote_url: includePaths || (project.remote_url && isClonableRemote(project.remote_url)) ? project.remote_url : null,
    default_agent: project.default_agent,
    pinned: project.pinned,
    tags: project.tags,
    description: project.description,
    accounts: mappedAgents(project.accounts, (account, agent) => projectAccount(agent, account)),
    launch: project.launch,
    git_workflow: project.git_workflow,
  });
  const consoles = Array.from(state.consoles.values())
    .filter((console) => regions.consoles === "all" || (regions.consoles === "one" && console.id === request.console))
    .sort((a, b) => a.created_at - b.created_at || (a.id < b.id ? -1 : 1))
    .map(
      (console): DocumentConsole => ({
        name: console.name,
        icon: console.icon,
        default_agent: console.default_agent,
        project_session_agent: console.project_session_agent ?? null,
        accounts: mappedAgents(console.accounts, (id, agent) => accountName(agent, id)),
        launch: console.launch,
        project_session_launch: console.project_session_launch,
        console_session_prompt: console.console_session_prompt,
        projects: Array.from(state.projects.values())
          .filter((project) => project.console_id === console.id)
          .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : a.id < b.id ? -1 : 1))
          .map(documentProject),
      }),
    );
  return {
    format: "octoboard-configuration",
    version: 1,
    exported_by: null,
    settings: regions.settings
      ? {
        auto_pull_repositories: state.settings.auto_pull_repositories,
        auto_delete_unresumable_archives: state.settings.auto_delete_unresumable_archives,
        auto_select_account: state.settings.auto_select_account,
        auto_switch_on_usage_limit: state.settings.auto_switch_on_usage_limit,
        sidebar_order_by_status: state.settings.sidebar_order_by_status,
        auto_start_quota_windows: state.settings.auto_start_quota_windows,
        auto_approve_session_requests: state.settings.auto_approve_session_requests,
        auto_approve_resume_requests: state.settings.auto_approve_resume_requests,
        auto_approve_account_switches: state.settings.auto_approve_account_switches,
        auto_approve_handoffs: state.settings.auto_approve_handoffs,
        ask_questions_as_choices: state.settings.ask_questions_as_choices,
        git_workflow: state.settings.git_workflow,
        claude_compatibility: { ...choices },
      }
      : null,
    ...(regions.accounts && { accounts: exportedAccounts(state) }),
    consoles,
  };
}

/** What an import's result says that its request did not ask for, or the other way about: a project
 * left out that was given a resolution, one placed or tried that was given none, consoles reported though none were to be created, a created one
 * placed another way than the one asked for, an account left out that was not given the resolution to leave it out (or
 * the other way about), accounts reported without being asked for or asked for and not reported, and application
 * settings reported without being asked for or asked for and not reported. A console that could not be created is not looked into, since
 * its projects are all not tried whatever was asked. `undefined` where the two agree. */
export function importDisagreement(
  request: Extract<RequestBody, { type: "import_configuration" }>,
  result: Omit<Extract<Event, { type: "configuration_imported" }>, "type">,
): string | undefined {
  // Without `apply_consoles` the daemon creates no console or project and reads no resolution.
  if (request.apply_consoles === false && result.consoles.length > 0) return "the result reports consoles, but the request asked for none to be created";
  const asked = new Map((request.apply_consoles === false ? [] : (request.resolutions ?? [])).map((r) => [`${r.console}:${r.project}`, r.choice.kind]));
  for (const [c, console] of result.consoles.entries()) {
    // A console that could not be created has all its projects not tried, whatever was asked of
    // them.
    if (console.status === "failed") continue;
    for (const [p, project] of console.projects.entries()) {
      const choice = asked.get(`${c}:${p}`);
      if (project.status === "left_out") {
        if (choice !== undefined) return `${project.name} is reported left out, but the request resolves it as ${choice}`;
      } else if (choice === undefined) {
        return `${project.name} is reported ${project.status}, but the request resolves nothing for it`;
      } else if (project.status === "created" && project.via !== choice) {
        return `${project.name} is reported placed as ${project.via}, but the request resolves it as ${choice}`;
      }
    }
  }
  const reportsAccounts = result.accounts != null;
  if (reportsAccounts !== (request.apply_accounts === true)) {
    return reportsAccounts
      ? "the result reports accounts, but the request did not ask for them"
      : "the request asked for the accounts, but the result reports none";
  }
  const leftOut = new Set((request.account_resolutions ?? []).filter((r) => r.leave_out === true).map((r) => `${r.agent}:${r.account}`));
  for (const agent of result.accounts?.agents ?? []) {
    for (const [a, account] of agent.accounts.entries()) {
      const left = leftOut.has(`${agent.agent}:${a}`);
      if (account.status === "left_out" && !left) return `${account.name} is reported left out, but the request does not leave it out`;
      if (account.status !== "left_out" && left) return `${account.name} is reported ${account.status}, but the request leaves it out`;
    }
  }
  const reportsSettings = result.settings_applied != null || result.settings_error != null;
  if (reportsSettings !== (request.apply_settings === true)) {
    return reportsSettings
      ? "the result reports application settings, but the request did not ask for them"
      : "the request asked for the application settings, but the result reports none";
  }
  return undefined;
}

/** The result with the document's `keyboard_shortcuts` echoed in `settings_applied`, as the daemon does when it applied
 * the settings: it does not apply them itself, so it hands them back as they came, for the client to apply. A document
 * without them, or a result that reports no settings applied (none asked for, or a `settings_error`), echoes nothing. */
export function withShortcutsEcho(
  request: Extract<RequestBody, { type: "import_configuration" }>,
  result: Omit<Extract<Event, { type: "configuration_imported" }>, "type">,
): Omit<Extract<Event, { type: "configuration_imported" }>, "type"> {
  const carried = (request.document as { settings?: { keyboard_shortcuts?: unknown } | null } | null)?.settings?.keyboard_shortcuts;
  if (!result.settings_applied || carried === undefined || carried === null) return result;
  return { ...result, settings_applied: { ...result.settings_applied, keyboard_shortcuts: carried as NonNullable<DocumentSettings["keyboard_shortcuts"]> } };
}

/** The reply to a configuration request, or `undefined` for a request that is not one. `fail`
 * throws a daemon error as the fixture daemon words it. */
export function createConfigurationReplies(
  fixture: ConfigurationFixture | undefined,
  getState: () => State,
  fail: (error: FixtureError) => never,
  claudeCompatibilityChoices: () => Partial<Record<Agent, boolean>>,
): (body: RequestBody) => Event | Promise<Event> | undefined {
  return (body) => {
    switch (body.type) {
      case "export_configuration": {
        const refusal = exportRefusal(getState(), body);
        if (refusal) return fail(refusal);
        return { type: "configuration_exported", id: null, document: exportedDocument(getState(), body, claudeCompatibilityChoices()) };
      }
      case "inspect_configuration": {
        const inspection = fixture?.inspection;
        if (inspection === undefined) return refuseUnfillable("the scenario gives no inspection to answer inspect_configuration with");
        if ("code" in inspection) return fail(inspection);
        return { type: "configuration_inspected", ...inspection };
      }
      case "import_configuration": {
        const imported = fixture?.imported;
        if (imported === undefined) return refuseUnfillable("the scenario gives no result to answer import_configuration with");
        if (imported === "pending") return new Promise<Event>(() => {});
        if ("code" in imported) return fail(imported);
        const disagreement = importDisagreement(body, imported);
        if (disagreement !== undefined) return refuseUnfillable(`the scenario's import result does not follow from its steps: ${disagreement}`);
        return { type: "configuration_imported", ...withShortcutsEcho(body, imported) };
      }
      default:
        return undefined;
    }
  };
}
