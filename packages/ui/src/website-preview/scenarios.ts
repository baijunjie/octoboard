import { maxWidthQuery } from "../components/hooks/breakpoint";
import type { Language } from "../i18n/languages";
import { type Agent, type AgentAvailability, type PageContent, type Session } from "../protocol";
import type { State } from "../store";
import type { Fixture } from "../fixtures/fixture";
import type { Step } from "../fixtures/interact";
import type { Preferences } from "../fixtures/preferences";
import { quotaOf, quotaWindow } from "../fixtures/quotas";
import {
  consoleOf,
  gitStatusOf,
  pageOf,
  projectOf,
  sessionOf,
  shellTerminalOf,
  snapshotState,
} from "../fixtures/builders";
import { previewRepositories } from "./repositories";
import { previewCopy as english } from "./locales/en";
import { previewCopy as chinese } from "./locales/zh-Hans";
import { createIdleScript, createTerminalScript } from "./terminals";
import { terminalCopy } from "./terminalScripts";
import type { TerminalScript } from "../fixtures/fakeTerminal";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";

export type PreviewScene = "overview" | "focus" | "changes";

/** The agents the preview shows: its own list, apart from the providers the app serves. */
const PREVIEW_AGENTS = ["claude", "codex", "grok"] as const;

/** Every agent installed, so each has a quota tab and the selected session its rail indicator. */
const agentAvailability = new Map<Agent, AgentAvailability>(
  PREVIEW_AGENTS.map((agent) => [
    agent,
    { agent, availability: "available", default_account_dir: `/demo/.${agent}` },
  ]),
);

/** Sample readings of each agent's default account. Their reset times count from when the page
 * loaded, never from a fixed date, so no visit finds them already past. */
function previewQuotas() {
  return [
    quotaOf("claude", null, "max", [
      quotaWindow("session", 38, 2 * 60 + 17),
      quotaWindow("weekly", 46, 3 * 24 * 60 + 5 * 60),
    ]),
    quotaOf("codex", null, "plus", [
      quotaWindow("session", 21, 3 * 60 + 40),
      quotaWindow("weekly", 29, 4 * 24 * 60 + 2 * 60),
    ]),
    quotaOf("grok", null, "SuperGrok", [
      quotaWindow("credit_period", 31, 5 * 24 * 60 + 9 * 60),
    ]),
  ];
}

export const copyFor = (language: Language) =>
  language === "zh-Hans" ? chinese : english;

function report(copy: typeof english): PageContent {
  // This is sample agent-authored report content, rendered by the application's real report panel.
  const rows = [
    [copy.projectAdmin, copy.adminResult, copy.adminProgress],
    [copy.projectMerchant, copy.merchantResult, copy.merchantProgress],
    [copy.projectCustomer, copy.customerResult, copy.customerProgress],
    [copy.projectServer, copy.serverResult, copy.serverProgress],
    [copy.projectMobile, copy.mobileResult, copy.mobileProgress],
    [copy.projectTablet, copy.tabletResult, copy.tabletProgress],
    [copy.projectPos, copy.posResult, copy.posProgress],
    [copy.projectKitchen, copy.kitchenResult, copy.kitchenProgress],
  ];
  return {
    title: copy.promotionReportTitle,
    body: [
      {
        type: "markdown",
        text: `*${copy.progressReportSubtitle}*\n\n**${copy.inProgress}**\n\n${copy.promotionProgressIntro}`,
      },
      {
        type: "markdown",
        text: `**${rows.length}** ${copy.projects} · **48** ${copy.checksPassedSoFar} · **1** ${copy.decision}`,
      },
      {
        type: "section",
        heading: copy.projects,
        children: [{
          type: "markdown",
          text: rows.map(([title, result, progress]) => `**${title}**  \n${result}  \n${progress}`).join("\n\n"),
        }],
      },
      { type: "section", heading: copy.decision, children: [{ type: "markdown", text: copy.stackingDecision }] },
      { type: "section", heading: copy.next, children: [{ type: "markdown", text: copy.promotionContinuation }] },
    ],
  };
}

/** What the preview mounts: the daemon's fixture, the preferences the app starts from and the steps that
 * walk it into its scene. */
export type PreviewScenario = Fixture & { preferences: Preferences; steps: Step[] };

/** Scenes select a view of one ongoing workspace, so its sessions and progress stay shared. */
export function createPreviewScenario(
  scene: PreviewScene,
  language: Language,
  /** Whether a working session's terminal moves; false for a visitor who asks for reduced motion. */
  animate = true,
): {
  scenario: PreviewScenario;
  /** A terminal's output, given what the preview's daemon holds now: the visitor can open sessions,
   * and add projects, that the scenario does not know. */
  output: (session: string, held?: Pick<State, "sessions" | "projects" | "consoles">) => string | TerminalScript | undefined;
} {
  const copy = copyFor(language);
  const workspace = consoleOf("preview-console", copy.workspace, {
    workdir: "/demo/bistro",
  });
  const projects = [
    ["admin", copy.projectAdmin],
    ["merchant", copy.projectMerchant],
    ["customer", copy.projectCustomer],
    ["server", copy.projectServer],
    ["mobile", copy.projectMobile],
    ["tablet", copy.projectTablet],
    ["pos", copy.projectPos],
    ["kitchen", copy.projectKitchen],
  ].map(([id, name]) =>
    projectOf(`preview-${id}`, workspace.id, name, {
      path: `/demo/bistro/${id}`,
      tags: ["meal-deal-promotion"],
    }),
  );
  const coordinator = sessionOf(
    "preview-coordinator",
    workspace.id,
    undefined,
    copy.coordinator,
    "working",
    { colour: "teal" },
  );
  const shell = shellTerminalOf("preview-server-shell", projects[3], "zsh", 0);
  const projectSessions = [
    sessionOf(
      "preview-admin-session",
      workspace.id,
      projects[0].id,
      copy.adminTask,
      "idle",
      { agent: "codex", bound_to: coordinator.id },
    ),
    sessionOf(
      "preview-merchant-session",
      workspace.id,
      projects[1].id,
      copy.merchantTask,
      "working",
      { agent: "claude", bound_to: coordinator.id },
    ),
    sessionOf(
      "preview-customer-session",
      workspace.id,
      projects[2].id,
      copy.customerTask,
      "working",
      { agent: "grok", bound_to: coordinator.id },
    ),
    sessionOf(
      "preview-server-session",
      workspace.id,
      projects[3].id,
      copy.serverTask,
      "working",
      { agent: "codex", bound_to: coordinator.id },
    ),
    sessionOf(
      "preview-review",
      workspace.id,
      projects[3].id,
      copy.review,
      "idle",
      {
        agent: "claude",
        bound_to: coordinator.id,
        background_tasks: [
          { id: "preview-order-watch", type: "shell", description: "pnpm test --watch" },
        ],
      },
    ),
    sessionOf(
      "preview-order-checks",
      workspace.id,
      projects[3].id,
      copy.orderChecks,
      "waiting_user",
      { agent: "claude", bound_to: coordinator.id },
    ),
    sessionOf(
      "preview-mobile-session",
      workspace.id,
      projects[4].id,
      copy.mobileTask,
      "working",
      { agent: "claude", bound_to: coordinator.id },
    ),
    sessionOf(
      "preview-tablet-session",
      workspace.id,
      projects[5].id,
      copy.tabletTask,
      "working",
      { agent: "grok", bound_to: coordinator.id },
    ),
    sessionOf(
      "preview-pos-session",
      workspace.id,
      projects[6].id,
      copy.posTask,
      "idle",
      { agent: "codex", bound_to: coordinator.id },
    ),
    sessionOf(
      "preview-kitchen-session",
      workspace.id,
      projects[7].id,
      copy.kitchenTask,
      "working",
      { agent: "claude", bound_to: coordinator.id },
    ),
  ];
  const scenario: PreviewScenario = {
    state: {
      ...snapshotState({
        consoles: [workspace],
        projects,
        sessions: [coordinator, ...projectSessions],
        shellTerminals: [shell],
        pages: {
          [coordinator.id]: [
            pageOf("preview-report", coordinator.id, report(copy), 2),
          ],
        },
        gitStatuses: projects.map((project) =>
          gitStatusOf(project.id, {
            branch: previewRepositories.get(project.id)!.branch,
            ahead: 0,
          }),
        ),
        settings: { ...DEFAULT_SETTINGS, added_agents: [...PREVIEW_AGENTS], default_clone_dir: "/demo" },
        accountQuotas: previewQuotas(),
      }),
      agentAvailability,
    },
    preferences: {
      sidebarConsole: workspace.id,
      sidebarWidth: scene === "changes" ? 310 : 360,
      asideWidth: 430,
      panelStacks: {
        project: {
          open: scene === "focus" ? ["files", "terminal"] : scene === "changes" ? ["files"] : [],
          shares: {},
        },
        report: { open: [], shares: {} },
      },
      ...(scene === "focus"
        ? {
          sidebarFocus: `project:${shell.project_id}` as const,
          projectFocusIncludeConsoleSessions: true,
        }
        : {}),
      projectBrowsers: Object.fromEntries(projects.map((project) => [project.id, { mode: "files", layouts: { changes: "tree" } } as const])),
    },
    configuration: { unavailable: true },
    steps: [
      async (ui) => {
        // Match the application's CSS breakpoint: the standalone demo also runs on phones, where
        // the sessions list is a drawer that has to be opened to select a session and closed to see
        // the scene's panel, which docks beside the agent terminal, or floats over it on a phone.
        const narrow = window.matchMedia(maxWidthQuery("tablet")).matches;
        if (narrow) await ui.press(ui.t("titleBar.sidebar.show"));
        await ui.press(
          ui.session(
            scene === "focus" ? copy.serverTask : scene === "changes" ? copy.review : coordinator.title,
          ),
        );
        // Selecting a session normally closes the tablet drawer; close it here if it remains open.
        const hide = ui.t("titleBar.sidebar.hide");
        if (narrow && document.querySelector(`[aria-label="${hide}"]`))
          await ui.press(hide);
        // On a phone the drawer takes a floating panel away with it, so the scene's panel is
        // opened again once the drawer has closed.
        const show =
          scene === "focus"
            ? ui.t("titleBar.terminal.show")
            : scene === "changes"
              ? ui.t("titleBar.files.show")
              : undefined;
        if (show && document.querySelector(`[aria-label="${show}"]`))
          await ui.press(show);
      },
    ],
  };
  if (scene === "changes")
    scenario.steps.push(
      (ui) => ui.press(ui.t("git.view.changes")),
      (ui) => ui.press(ui.changeRow("meal-deal-orders.ts", "modified")),
      (ui) => ui.wait(600),
    );
  const sessions = [coordinator, ...projectSessions];
  const startedAt = Date.now();
  const cyan = "\x1b[38;5;80m";
  const reset = "\x1b[0m";
  return {
    scenario,
    output: (session, held) => {
      if (session === shell.id) {
        const prompt = `${cyan}server${reset} % `;
        return [
          `${prompt}pwd`,
          shell.cwd,
          "",
          `${prompt}git branch --show-current`,
          previewRepositories.get(shell.project_id)!.branch,
          "",
          prompt,
        ].join("\r\n");
      }
      const cwdOf = (target: Session) =>
        (target.project_id ? held?.projects.get(target.project_id)?.path : undefined)
        ?? projects.find((project) => project.id === target.project_id)?.path
        ?? held?.consoles.get(target.console_id)?.workdir
        ?? workspace.workdir;
      const opened = held?.sessions.get(session);
      const found = sessions.find((candidate) => candidate.id === session);
      // A session the visitor opened starts at rest, never on the fake terminal's own sample, which is
      // the gallery's and names its paths and addresses. A shell the visitor opens is no session the
      // daemon holds, so it gets no script and the fake terminal draws it a fresh prompt.
      if (!found) return opened && createIdleScript(opened.agent, cwdOf(opened));
      return createTerminalScript(
        {
          id: found.id,
          agent: found.agent,
          status: found.status === "working" || found.status === "waiting_user" ? found.status : "idle",
          cwd: cwdOf(found),
          background: found.background_tasks?.[0]?.description ?? undefined,
          copy: terminalCopy(found.id, copy.terminals[found.id]),
        },
        { startedAt, animate },
      );
    },
  };
}
