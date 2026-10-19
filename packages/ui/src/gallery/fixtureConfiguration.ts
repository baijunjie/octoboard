// What the fixture daemon answers exporting and importing configuration with. An export is built
// from the scenario's own state, as the daemon builds one from what it stores, so the document a
// scenario saves is the one its consoles and projects would export. An inspection and an import
// result are what the scenario says: they depend on the filesystem and on the clones the real
// daemon makes, which the gallery has none of, so a scenario that needs one gives it (and one that
// does not give it is refused rather than answered with a made-up one). What a result says of the
// request it answers is held to the request, so a scenario whose steps choose otherwise than its
// result assumes is refused rather than shown as if the choice had gone the way the result says.
import type {
  AccountName,
  Agent,
  ConfigurationDocument,
  DocumentConsole,
  DocumentProject,
  DocumentProjectAccount,
  Event,
  Project,
  ProjectAccount,
  RequestBody,
} from "../protocol";
import type { State } from "../store";
import { refuseUnfillable } from "./banner";
import type { FixtureError } from "./fixtures/projectFiles";
import type { ConfigurationFixture } from "./scenario";

/** The document `export_configuration` answers: the console named, or every console and the
 * application settings; consoles by creation time and a console's projects by name, with no id and
 * no console working directory in it, and an account carried by its name alone. */
export function exportedDocument(state: State, consoleId?: string): ConfigurationDocument {
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
    path: project.path,
    source: project.source,
    remote_url: project.remote_url,
    default_agent: project.default_agent,
    pinned: project.pinned,
    tags: project.tags,
    description: project.description,
    claude_account: projectAccount("claude", project.claude_account_id),
    codex_account: projectAccount("codex", project.codex_account_id),
    grok_account: projectAccount("grok", project.grok_account_id),
    claude_session_model: project.claude_session_model,
    claude_session_effort: project.claude_session_effort,
    codex_session_model: project.codex_session_model,
    codex_session_effort: project.codex_session_effort,
    grok_session_model: project.grok_session_model,
    grok_session_effort: project.grok_session_effort,
  });
  const consoles = Array.from(state.consoles.values())
    .filter((console) => consoleId === undefined || console.id === consoleId)
    .sort((a, b) => a.created_at - b.created_at || (a.id < b.id ? -1 : 1))
    .map(
      (console): DocumentConsole => ({
        name: console.name,
        icon: console.icon,
        default_agent: console.default_agent,
        project_session_agent: console.project_session_agent ?? null,
        claude_account: accountName("claude", console.claude_account_id),
        codex_account: accountName("codex", console.codex_account_id),
        grok_account: accountName("grok", console.grok_account_id),
        claude_console_session_model: console.claude_console_session_model,
        claude_console_session_effort: console.claude_console_session_effort,
        codex_console_session_model: console.codex_console_session_model,
        codex_console_session_effort: console.codex_console_session_effort,
        grok_console_session_model: console.grok_console_session_model,
        grok_console_session_effort: console.grok_console_session_effort,
        claude_project_session_model: console.claude_project_session_model,
        claude_project_session_effort: console.claude_project_session_effort,
        codex_project_session_model: console.codex_project_session_model,
        codex_project_session_effort: console.codex_project_session_effort,
        grok_project_session_model: console.grok_project_session_model,
        grok_project_session_effort: console.grok_project_session_effort,
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
    ...(consoleId === undefined && {
      settings: {
        auto_pull_repositories: state.settings.auto_pull_repositories,
        auto_delete_unresumable_archives: state.settings.auto_delete_unresumable_archives,
      },
    }),
    consoles,
  };
}

/** What an import's result says that its request did not ask for, or the other way about: a project
 * left out that was given a resolution, one placed or tried that was given none, a created one
 * placed another way than the one asked for, and application settings reported without being asked
 * for or asked for and not reported. A console that could not be created is not looked into, since
 * its projects are all not tried whatever was asked. `undefined` where the two agree. */
export function importDisagreement(
  request: Extract<RequestBody, { type: "import_configuration" }>,
  result: Omit<Extract<Event, { type: "configuration_imported" }>, "type">,
): string | undefined {
  const asked = new Map((request.resolutions ?? []).map((r) => [`${r.console}:${r.project}`, r.choice.kind]));
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
  const reportsSettings = result.settings_applied != null || result.settings_error != null;
  if (reportsSettings !== (request.apply_settings === true)) {
    return reportsSettings
      ? "the result reports application settings, but the request did not ask for them"
      : "the request asked for the application settings, but the result reports none";
  }
  return undefined;
}

/** The reply to a configuration request, or `undefined` for a request that is not one. `fail`
 * throws a daemon error as the fixture daemon words it. */
export function createConfigurationReplies(
  fixture: ConfigurationFixture | undefined,
  getState: () => State,
  fail: (error: FixtureError) => never,
): (body: RequestBody) => Event | Promise<Event> | undefined {
  return (body) => {
    switch (body.type) {
      case "export_configuration":
        return { type: "configuration_exported", document: exportedDocument(getState(), body.console) };
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
        return { type: "configuration_imported", ...imported };
      }
      default:
        return undefined;
    }
  };
}
