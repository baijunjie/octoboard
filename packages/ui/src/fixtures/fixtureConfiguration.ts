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
  ProjectAccountRef,
  DocumentSettings,
  Event,
  ImportItem,
  PerAgent,
  Project,
  ProjectAccount,
  ClientRequestBody,
  Settings,
} from "../protocol";
import type { ImportProgressEvent, State } from "../store";
import type { Inspection } from "../configuration/importPlan";
import { refuseUnfillable } from "./banner";
import type { FixtureError } from "./sampleFiles";
import type { ConfigurationFixture, ImportProgressFixture } from "./fixture";
import { exportedProviderSettings } from "./builders";

/** The last segment of `path`, or an empty string where it has none (the filesystem root). */
const directoryName = (path: string): string => path.split("/").filter(Boolean).at(-1) ?? "";

/** Whether a remote can be cloned from on another machine: a URL of a network scheme, or the
 * `host:path` form, and not a local path or a `file://` URL. */
const isClonableRemote = (url: string): boolean =>
  /^(https?|ssh|git|git\+ssh|ssh\+git):\/\/\S/i.test(url) || /^[^\s/:]+:(?![:/]{1,2})\S/.test(url);

type ExportRequest = Extract<ClientRequestBody, { type: "export_configuration" }>;

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

/** The accounts region: the added agents and, for each added agent and each other agent with an account of its own, a
 * named default account or a turned-off one, the default account's name, disabled flag and place in the display order
 * (the accounts before it) and the accounts the user owns in that order (the saved order, then those added since); then
 * the settings of the Agent accounts section with the user's explicit provider setting `choices` of those that are
 * exported. Never an id, a credential, the default account's directory or whether an account shares the default
 * configuration. */
function exportedAccounts(state: State, choices: Settings["provider_settings"]): DocumentAccounts {
  const { settings } = state;
  const agents: DocumentAccounts["agents"] = [];
  for (const agent of ["claude", "codex", "grok"] as const) {
    const owned = settings.accounts.filter((account) => account.agent === agent);
    const name = settings.default_account_names?.[agent];
    const defaultDisabled = settings.disabled_default_accounts.includes(agent);
    // An added agent is listed with nothing else to carry too, so an import can turn on a default account this machine
    // disabled; a list not yet decided (`null`) counts every agent as added.
    const added = settings.added_agents === null || settings.added_agents.includes(agent);
    if (!added && owned.length === 0 && !name && !defaultDisabled) continue;
    const byId = new Map<string | null, Account | null>([[null, null], ...owned.map((account): [string, Account] => [account.id, account])]);
    const display: (Account | null)[] = [];
    for (const id of [...(settings.account_order?.[agent] ?? []), null, ...owned.map((account) => account.id)]) {
      if (!byId.has(id)) continue;
      display.push(byId.get(id)!);
      byId.delete(id);
    }
    agents.push({
      agent,
      default_account: { name: name ?? null, position: display.indexOf(null), disabled: defaultDisabled },
      accounts: display.flatMap((account) =>
        account
          ? [{ name: account.name, directory: documentDirectory(account.config_dir, state.homeDir), disabled: account.disabled }]
          : [],
      ),
    });
  }
  return {
    ...(settings.added_agents !== null && { added_agents: settings.added_agents }),
    agents,
    agent_settings: {
      auto_select_account: settings.auto_select_account,
      auto_switch_on_usage_limit: settings.auto_switch_on_usage_limit,
      auto_start_quota_windows: settings.auto_start_quota_windows,
      agent_tags: { ...settings.agent_tags },
      provider_settings: exportedProviderSettings(choices, state.providers),
    },
  };
}

/** One per-agent group with `map` applied to each agent's value. */
function mappedAgents<T, U>(group: PerAgent<T>, map: (value: T, agent: Agent) => U): PerAgent<U> {
  return Object.fromEntries((["claude", "codex", "grok"] as const).map((agent) => [agent, map(group[agent], agent)])) as PerAgent<U>;
}

/** The document `export_configuration` answers: the regions the request chooses (see `exportedRegions`): the console
 * named or every console or none, the application settings, the accounts; consoles by creation time and a console's
 * projects by name, with no id and no console working directory in it, and an account carried by its name alone.
 * Without `include_paths` a project's path is only its last segment and a project's remote is kept only where it can
 * be cloned from; an account directory is written whatever it says. Without the accounts region no account is named:
 * every console's reference is `null` and a project's reference to an account is `inherit`. A document with the
 * settings carries the user's explicit provider setting `choices` of the settings that are exported (not the values
 * in effect, which `state.settings` holds), `{}` when the user chose for none. A document with the settings carries
 * the default clone directory, written like an account directory, and the language the daemon holds when a window
 * has reported one. */
export function exportedDocument(
  state: State,
  request: Omit<ExportRequest, "type"> = {},
  choices: Settings["provider_settings"] = {},
): ConfigurationDocument {
  const regions = exportedRegions({ type: "export_configuration", ...request });
  const includePaths = request.include_paths ?? true;
  // Without the accounts region the document names no account: a console's reference is the default account and a
  // project's `account` reference is `inherit` (its `default` is kept, since it names none).
  const accountName = (agent: Agent, id: string | null | undefined): AccountName | null => {
    if (!regions.accounts) return null;
    const account = state.settings.accounts.find((candidate) => candidate.id === id && candidate.agent === agent);
    return account ? { name: account.name } : null;
  };
  const projectAccount = (agent: Agent, reference: ProjectAccount): ProjectAccountRef => {
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
        auto_close_idle_shell_terminals: state.settings.auto_close_idle_shell_terminals,
        auto_pull_repositories: state.settings.auto_pull_repositories,
        auto_delete_unresumable_archives: state.settings.auto_delete_unresumable_archives,
        sidebar_order_by_status: state.settings.sidebar_order_by_status,
        auto_approve_session_requests: state.settings.auto_approve_session_requests,
        auto_approve_resume_requests: state.settings.auto_approve_resume_requests,
        auto_approve_account_switches: state.settings.auto_approve_account_switches,
        auto_approve_handoffs: state.settings.auto_approve_handoffs,
        ask_questions_as_choices: state.settings.ask_questions_as_choices,
        git_workflow: state.settings.git_workflow,
        default_clone_dir: documentDirectory(state.settings.default_clone_dir, state.homeDir),
        ...(state.settings.interface_language !== null && { interface_language: state.settings.interface_language }),
      }
      : null,
    ...(regions.accounts && { accounts: exportedAccounts(state, choices) }),
    consoles,
  };
}

/** What an import's result says that its request did not ask for, or the other way about: a project left out that was
 * given a resolution, one placed or tried that was given none, consoles reported though none were to be created, a
 * created one placed another way than the one asked for, an account left out that was not given the resolution to
 * leave it out (or the other way about), accounts reported without being asked for or asked for and not reported,
 * application settings reported without being asked for or asked for and not reported, and a default clone directory
 * reported written without the settings or against the request keeping this machine's own. A console that could not
 * be created is not looked into, since its projects are all not tried whatever was asked. `undefined` where the two
 * agree. */
export function importDisagreement(
  request: Extract<ClientRequestBody, { type: "import_configuration" }>,
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
  // The directory is written with the settings, and only where the request does not keep this machine's own.
  const keepsOwnDir = request.apply_settings !== true || request.apply_clone_dir === false;
  if (keepsOwnDir && result.clone_dir_applied != null) {
    return "the result reports the default clone directory written, but the request did not ask for it to be applied";
  }
  if (request.apply_clone_dir === false && result.settings_applied?.default_clone_dir != null) {
    return "the result reports the default clone directory among the settings applied, but the request keeps this machine's own";
  }
  const appliedDir = result.settings_applied?.default_clone_dir;
  if (appliedDir != null && appliedDir !== result.clone_dir_applied) {
    return "the result reports the default clone directory among the settings applied, but not as the one written";
  }
  return undefined;
}

/** The items an `import_configuration` request makes the daemon handle, in the order it announces and handles them:
 * the document's default clone directory (when the settings are asked for, the document has a usable one and the
 * request does not keep this machine's own), the accounts (when asked for) other than those left out, an account
 * that exists already included whatever was chosen for it, since it is used as it is and the choice is never read,
 * the Agent accounts settings (when the accounts are asked for and the document carries them), then each console
 * with the projects that have a resolution (unless the request asks for no consoles), then the application settings
 * (when asked for). */
export function handledImportItems(request: Extract<ClientRequestBody, { type: "import_configuration" }>, inspection: Omit<Inspection, "type">): ImportItem[] {
  const items: ImportItem[] = [];
  if (request.apply_settings === true && request.apply_clone_dir !== false && inspection.settings?.default_clone_dir) items.push({ kind: "clone_dir" });
  if (request.apply_accounts === true) {
    const leftOut = new Set((request.account_resolutions ?? []).filter((r) => r.leave_out === true).map((r) => `${r.agent}:${r.account}`));
    for (const { agent, accounts } of inspection.accounts?.agents ?? []) {
      accounts.forEach((entry, account) => {
        if (entry.status === "existing" || !leftOut.has(`${agent}:${account}`)) items.push({ kind: "account", agent, account });
      });
    }
    if (inspection.accounts?.agent_settings) items.push({ kind: "agent_settings" });
  }
  if (request.apply_consoles !== false) {
    const resolved = new Set((request.resolutions ?? []).map((r) => `${r.console}:${r.project}`));
    inspection.consoles.forEach((console, c) => {
      items.push({ kind: "console", console: c });
      console.projects.forEach((_, p) => {
        if (resolved.has(`${c}:${p}`)) items.push({ kind: "project", console: c, project: p });
      });
    });
  }
  if (request.apply_settings === true) items.push({ kind: "settings" });
  return items;
}

const sameItem = (a: ImportItem, b: ImportItem): boolean => JSON.stringify(Object.entries(a).sort()) === JSON.stringify(Object.entries(b).sort());
const describeItem = (item: ImportItem): string => Object.values(item).join(" ");

/** What a progress list says that the daemon's work cannot have produced, or `undefined` where it can. The daemon handles
 * the items in the order `handled` has them, one at a time, so once an item still waits every later one waits too; a
 * console is `done` as soon as it is created, its projects following for themselves, or `failed` with every project it
 * announced `not_attempted`, so a project that has left `waiting` (and is not `not_attempted`) has a `done` console;
 * only a project that is cloned is `cloning`; and an item that is not handled has no report. */
export function importProgressDisagreement(handled: ImportItem[], progress: ImportProgressFixture[]): string | undefined {
  const reports = new Map<number, ImportProgressFixture>();
  for (const report of progress) {
    const at = handled.findIndex((item) => sameItem(item, report.item));
    if (at < 0) return `${describeItem(report.item)} is reported, but the import does not handle it`;
    if (reports.has(at)) return `${describeItem(report.item)} is reported twice`;
    reports.set(at, report);
  }
  const stateOf = (at: number) => reports.get(at)?.state ?? "waiting";
  let waiting = false;
  for (const [at, item] of handled.entries()) {
    const state = stateOf(at);
    if (waiting && state !== "waiting") return `${describeItem(item)} is ${state} after an item that still waits`;
    if (state === "waiting") waiting = true;
    if ((state === "cloning" || state === "not_attempted") && item.kind !== "project") return `${describeItem(item)} cannot be ${state}`;
    if (item.kind !== "project") continue;
    const consoleAt = handled.findIndex((other) => other.kind === "console" && other.console === item.console);
    const consoleState = stateOf(consoleAt);
    if ((state === "not_attempted") !== (consoleState === "failed") && state !== "waiting") return `${describeItem(item)} is ${state} but its console is ${consoleState}`;
    if (consoleState === "failed" && state === "waiting") return `${describeItem(item)} still waits though its console failed`;
    if (state !== "waiting" && state !== "not_attempted" && consoleState !== "done") return `${describeItem(item)} is ${state} but its console is ${consoleState}`;
  }
  return undefined;
}

/** The settings only a client holds, which the daemon does not apply but hands back in what it applied. */
const PASSED_THROUGH = ["keyboard_shortcuts", "appearance", "markdown_style", "run_in_background"] as const;

/** The result with the document's client-held settings (see `PASSED_THROUGH`) echoed in `settings_applied`, as the daemon
 * does when it applied the settings: it does not apply them itself, so it hands them back as they came, for the client to
 * apply. A document without them, or a result that reports no settings applied (none asked for, or a `settings_error`),
 * echoes nothing. */
export function withClientValuesEcho(
  request: Extract<ClientRequestBody, { type: "import_configuration" }>,
  result: Omit<Extract<Event, { type: "configuration_imported" }>, "type">,
): Omit<Extract<Event, { type: "configuration_imported" }>, "type"> {
  const carried = (request.document as { settings?: Record<string, unknown> | null } | null)?.settings;
  if (!result.settings_applied || !carried) return result;
  const echoed = PASSED_THROUGH.flatMap((field) => (carried[field] === undefined || carried[field] === null ? [] : [[field, carried[field]] as const]));
  if (echoed.length === 0) return result;
  return { ...result, settings_applied: { ...result.settings_applied, ...(Object.fromEntries(echoed) as Pick<DocumentSettings, (typeof PASSED_THROUGH)[number]>) } };
}

/** The reply to a configuration request, or `undefined` for a request that is not one. `fail`
 * throws a daemon error as the fixture daemon words it. */
export function createConfigurationReplies(
  fixture: ConfigurationFixture | undefined,
  getState: () => State,
  fail: (error: FixtureError) => never,
  providerSettingChoices: () => Settings["provider_settings"],
  reportProgress: (event: ImportProgressEvent) => void,
): (body: ClientRequestBody) => Event | Promise<Event> | undefined {
  return (body) => {
    switch (body.type) {
      case "export_configuration": {
        const refusal = exportRefusal(getState(), body);
        if (refusal) return fail(refusal);
        return { type: "configuration_exported", id: null, document: exportedDocument(getState(), body, providerSettingChoices()) };
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
        if (fixture?.progress) {
          const inspection = fixture.inspection;
          if (inspection === undefined || "code" in inspection) return refuseUnfillable("the scenario gives progress for an import but no inspection to say what it handles");
          if (!body.import_id) return refuseUnfillable("the scenario gives progress for an import, but the request carries no import_id to name it by");
          const handled = handledImportItems(body, inspection);
          const disagreement = importProgressDisagreement(handled, fixture.progress);
          if (disagreement !== undefined) return refuseUnfillable(`the scenario's import progress does not follow from its steps: ${disagreement}`);
          // Every item is announced waiting first; then each reports where it stands, in the order they are handled.
          for (const item of handled) reportProgress({ type: "configuration_import_progress", import_id: body.import_id, item, state: "waiting" });
          for (const item of handled) {
            const report = fixture.progress.find((candidate) => sameItem(candidate.item, item));
            if (report) reportProgress({ type: "configuration_import_progress", import_id: body.import_id, ...report });
          }
        }
        if (imported === "pending") return new Promise<Event>(() => {});
        if ("code" in imported) return fail(imported);
        const disagreement = importDisagreement(body, imported);
        if (disagreement !== undefined) return refuseUnfillable(`the scenario's import result does not follow from its steps: ${disagreement}`);
        return { type: "configuration_imported", ...withClientValuesEcho(body, imported) };
      }
      default:
        return undefined;
    }
  };
}
