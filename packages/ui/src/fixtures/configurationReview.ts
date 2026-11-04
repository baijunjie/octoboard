// The document an import reviews and the result of importing it, shared by the gallery's
// "Configuration" scenarios and the tests of the import dialog and its plan: one document of a laptop's
// layout set up on a machine where it sits elsewhere, and what importing it with the defaults came to.
// What the daemon would work out from the filesystem is written out here by hand.
import type { Imported } from "../configuration/ImportResult";
import type { Inspection } from "../configuration/importPlan";
import { currentExportedProviderSettings, effectiveProviderSettings } from "./builders";
import { DEFAULT_GIT_WORKFLOW } from "../git/gitWorkflow";
import { type AccountFallback, type Agent, type GitWorkflowSettings, type InspectedProject } from "../protocol";
import { FIXTURE_AGENTS } from "./fixtureProviders";

/** A reference to an account this machine has none by that name of. The inspection lists it whether or not the document's
 * own accounts would create one of that name: the review decides from the accounts and the user's choices whether it
 * resolves. */
export const fallback = (agent: Agent, name: string): AccountFallback => ({ agent, name });

export const inspectedProject = (extra: Partial<InspectedProject> & Pick<InspectedProject, "name" | "path">): InspectedProject => ({
  remote_url: null,
  offers: { exported_path: null, default_clone_dir: null, clone: null },
  default: null,
  account_fallbacks: [],
  ...extra,
});

/** The git workflow settings the full export carries: not the ones this machine starts with. */
const FILE_GIT_WORKFLOW: GitWorkflowSettings = { development: "branch", squash_before_delivery: false, rebase_before_delivery: false, delivery: "merge", revert_gate: true };

/** The provider settings the full export carries: only what the user chose on the exporting machine, of the settings that
 * are exported, here Codex's Claude compatibility off, where this machine has every agent's on. */
const FILE_PROVIDER_SETTINGS = { codex: { claude_compatibility: false } };

/** The agent tags the full export carries: Codex with several. */
const FILE_AGENT_TAGS = { codex: ["review", "backend", "nightly"] };

/** This machine's exported provider settings as an import's review reports them: the values in effect of both agents'
 * Claude compatibility. The machines the document is set up on have every agent added and no explicit choice, so both
 * are on. */
export const THIS_PROVIDER_SETTINGS = currentExportedProviderSettings(effectiveProviderSettings({}, FIXTURE_AGENTS));

/** An inspection of a document that carries no settings and no accounts unless `parts` gives them: the regions a document
 * does not carry are `null`, as the daemon sends them. */
export function inspected(parts: Pick<Inspection, "consoles"> & Partial<Omit<Inspection, "type" | "consoles">>): Omit<Inspection, "type"> {
  return { id: null, settings: null, accounts: null, ...parts };
}

/** An import's result that applied no settings and no accounts unless `parts` says it did, likewise. */
export function imported(parts: Pick<Imported, "consoles"> & Partial<Omit<Imported, "type" | "consoles">>): Omit<Imported, "type"> {
  return { id: null, settings_applied: null, settings_error: null, accounts: null, ...parts };
}

/** A document of a laptop's layout set up on a machine where it sits elsewhere: one project at its
 * recorded path, one found under the default clone directory, one to be cloned and one that nothing
 * places (whose recorded path is long enough to break over lines, and whose account falls back too),
 * a second console whose only project can be cloned, and one with no project. */
export const REVIEW = inspected({
  settings: {
    auto_close_idle_shell_terminals: true,
    current_auto_close_idle_shell_terminals: false,
    auto_pull_repositories: true,
    current_auto_pull_repositories: false,
    auto_delete_unresumable_archives: false,
    current_auto_delete_unresumable_archives: true,
    auto_select_account: true,
    current_auto_select_account: false,
    auto_switch_on_usage_limit: true,
    current_auto_switch_on_usage_limit: false,
    sidebar_order_by_status: false,
    current_sidebar_order_by_status: true,
    auto_start_quota_windows: true,
    current_auto_start_quota_windows: false,
    auto_approve_session_requests: true,
    current_auto_approve_session_requests: false,
    auto_approve_resume_requests: true,
    current_auto_approve_resume_requests: false,
    auto_approve_account_switches: true,
    current_auto_approve_account_switches: false,
    auto_approve_handoffs: true,
    current_auto_approve_handoffs: false,
    ask_questions_as_choices: false,
    current_ask_questions_as_choices: true,
    git_workflow: FILE_GIT_WORKFLOW,
    current_git_workflow: DEFAULT_GIT_WORKFLOW,
    provider_settings: FILE_PROVIDER_SETTINGS,
    current_provider_settings: THIS_PROVIDER_SETTINGS,
    agent_tags: FILE_AGENT_TAGS,
    current_agent_tags: {},
  },
  consoles: [
    {
      name: "Work laptop",
      account_fallbacks: [fallback("claude", "Work")],
      projects: [
        inspectedProject({
          name: "Website",
          path: "/Users/ana/code/website",
          remote_url: "git@github.com:acme/website.git",
          offers: { exported_path: "/Users/ana/code/website", default_clone_dir: "/Users/dev/Projects/website", clone: null },
          default: "exported_path",
        }),
        inspectedProject({
          name: "Search API",
          path: "/Users/ana/work/search-api",
          offers: { exported_path: null, default_clone_dir: "/Users/dev/Projects/search-api", clone: null },
          default: "default_clone_dir",
          account_fallbacks: [fallback("codex", "Personal")],
        }),
        inspectedProject({
          name: "Design System",
          path: "/Users/ana/work/design-system",
          remote_url: "https://github.com/acme/design-system.git",
          offers: { exported_path: null, default_clone_dir: null, clone: "/Users/dev/Projects/design-system" },
          default: "clone",
        }),
        inspectedProject({
          name: "Notes",
          path: "/Users/ana/Documents/Work/Clients/Northwind/Archive/2019/Quarterly-Reports/final-notes",
          account_fallbacks: [fallback("codex", "Personal")],
        }),
      ],
    },
    {
      name: "Side projects",
      account_fallbacks: [],
      projects: [
        inspectedProject({
          name: "Dotfiles",
          path: "/Users/ana/dotfiles",
          remote_url: "https://github.com/ana/dotfiles.git",
          offers: { exported_path: null, default_clone_dir: null, clone: "/Users/dev/Projects/dotfiles" },
          default: "clone",
        }),
      ],
    },
    { name: "Scratch", account_fallbacks: [], projects: [] },
  ],
});

export const failure = (code: string, params: Record<string, string>, message: string) => ({ code, params, message });

/** What git says of a clone that failed: long, and with an address in it that has no place to break. */
const LONG_CLONE_DETAIL = [
  "Cloning into '/Users/dev/Projects/مرحبا/design-system'...",
  "fatal: unable to access 'https://github.com/acme/design-system.git/': Could not resolve host: github.com",
  "    retry failed after reading the proxy configuration",
].join("\n");

/** What importing `REVIEW` with its defaults came to: a project whose directory was gone by the time
 * it was imported, a clone that failed with a long reason, a project nothing placed, a console that
 * could not be created with its project not tried, and the account references that fell back. */
export const MIXED_RESULT = imported({
  settings_applied: { auto_close_idle_shell_terminals: true, auto_pull_repositories: true, auto_delete_unresumable_archives: false, auto_select_account: true, auto_switch_on_usage_limit: true, sidebar_order_by_status: false, auto_start_quota_windows: true, auto_approve_session_requests: true, auto_approve_resume_requests: true, auto_approve_account_switches: true, auto_approve_handoffs: true, ask_questions_as_choices: false, git_workflow: FILE_GIT_WORKFLOW, provider_settings: FILE_PROVIDER_SETTINGS, agent_tags: FILE_AGENT_TAGS },
  consoles: [
    {
      name: "Work laptop",
      status: "created",
      console: "c-new-1",
      error: null,
      account_fallbacks: [fallback("claude", "Work")],
      projects: [
        {
          name: "Website",
          status: "failed",
          via: null,
          project: null,
          error: failure(
            "path_not_found",
            { path: "/Users/ana/code/website" },
            "/Users/ana/code/website does not exist",
          ),
          account_fallbacks: [],
        },
        {
          name: "Search API",
          status: "created",
          via: "default_clone_dir",
          project: "p-new-2",
          error: null,
          account_fallbacks: [fallback("codex", "Personal")],
        },
        {
          name: "Design System",
          status: "failed",
          via: null,
          project: null,
          error: failure(
            "git_clone_failed",
            { detail: LONG_CLONE_DETAIL },
            `git clone failed: ${LONG_CLONE_DETAIL}`,
          ),
          account_fallbacks: [],
        },
        { name: "Notes", status: "left_out", via: null, project: null, error: null, account_fallbacks: [] },
      ],
    },
    {
      name: "Side projects",
      status: "failed",
      console: null,
      error: failure("internal_error", { detail: "the data directory cannot be written" }, "the data directory cannot be written"),
      account_fallbacks: [],
      projects: [{ name: "Dotfiles", status: "not_attempted", via: null, project: null, error: null, account_fallbacks: [] }],
    },
    { name: "Scratch", status: "created", console: "c-new-3", error: null, account_fallbacks: [], projects: [] },
  ],
});
