import type { Language } from "../i18n/languages";
import type { Agent, AgentAvailability } from "../protocol";
import type { Scenario } from "../gallery/scenario";
import { quotaOf, quotaWindow } from "../gallery/fixtures/quotas";
import {
  ALL_AGENTS,
  consoleOf,
  gitStatusOf,
  pageOf,
  projectOf,
  sessionOf,
  shellTerminalOf,
  snapshotState,
} from "../gallery/fixtures/builders";
import { previewRepositories } from "./repositories";
import { previewCopy as english } from "./locales/en";
import { previewCopy as chinese } from "./locales/zh-Hans";
import { createTerminalScript } from "./terminals";
import { terminalCopy } from "./terminalScripts";
import type { TerminalScript } from "../gallery/fakeTerminal";
import { DEFAULT_GIT_WORKFLOW } from "../gitWorkflow";

export type PreviewScene = "overview" | "focus" | "changes";

/** Every agent installed, so each has a quota tab and the selected session its rail indicator. */
const agentAvailability = new Map<Agent, AgentAvailability>(
  ALL_AGENTS.map((agent) => [
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

function report(copy: typeof english): string {
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
  return `<!doctype html><html><head><title>${copy.promotionReportTitle}</title><style>
    body{font:14px/1.6 system-ui,sans-serif;margin:0;padding:24px;color:#253044;background:#fff}
    small{color:#667085}h1{font-size:26px;line-height:1.2;margin:12px 0}h2{font-size:16px;margin:24px 0 8px}
    .status{display:inline-block;background:#e4f4ed;color:#136845;padding:3px 9px;border-radius:6px;font-size:12px}
    .stats{display:flex;gap:20px;border-block:1px solid #e7e9ed;padding:16px 0;margin:20px 0}
    .stats div{flex:1}.stats strong{display:block;font-size:26px;line-height:1.2}.stats span{font-size:11px;color:#667085}
    article{padding:12px 0;border-bottom:1px solid #e7e9ed}article strong{font-size:13px}article p{margin:3px 0;color:#536070}
    .decision{padding:14px;background:#fff6e5;border-radius:8px;color:#715014}
  </style></head><body data-website-preview-report><small>${copy.progressReportSubtitle}</small><h1>${copy.promotionReportTitle}</h1>
  <span class="status">${copy.inProgress}</span><p>${copy.promotionProgressIntro}</p>
  <div class="stats"><div><strong>${rows.length}</strong><span>${copy.projects}</span></div><div><strong>48</strong><span>${copy.checksPassedSoFar}</span></div><div><strong>1</strong><span>${copy.decision}</span></div></div>
  ${rows.map(([title, result, progress]) => `<article><strong>${title}</strong><p>${result}</p><p>${progress}</p></article>`).join("")}
  <h2>${copy.decision}</h2><p class="decision">${copy.stackingDecision}</p><h2>${copy.next}</h2><p>${copy.promotionContinuation}</p>
  </body></html>`;
}

/** Scenes select a view of one ongoing workspace, so its sessions and progress stay shared. */
export function createPreviewScenario(
  scene: PreviewScene,
  language: Language,
  /** Whether a working session's terminal moves; false for a visitor who asks for reduced motion. */
  animate = true,
): { scenario: Scenario; output: (session: string) => string | TerminalScript | undefined } {
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
  const scenario: Scenario = {
    id: `website-${scene}`,
    group: "Focus mode",
    category: "normal",
    title: copy.demo,
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
        settings: {
          auto_pull_repositories: false,
          auto_delete_unresumable_archives: true,
          added_agents: ALL_AGENTS,
          default_clone_dir: "/demo", git_workflow: DEFAULT_GIT_WORKFLOW,
          accounts: [],
        },
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
      changeLayout: "tree",
    },
    configuration: { unavailable: true },
    clipboard: "unavailable",
    steps: [
      async (ui) => {
        // Match the application's CSS breakpoint: the standalone demo also runs on phones, where
        // the sessions list is a drawer that has to be opened to select a session and closed to see
        // the scene's panel, which docks beside the agent terminal, or floats over it on a phone.
        const breakpoint = parseFloat(
          getComputedStyle(document.documentElement).getPropertyValue(
            "--breakpoint-docked",
          ),
        );
        const narrow = !window.matchMedia(`(min-width: ${breakpoint}px)`)
          .matches;
        if (narrow) await ui.press(ui.t("titleBar.sidebar.show"));
        await ui.press(
          ui.session(
            scene === "focus" ? copy.serverTask : scene === "changes" ? copy.review : coordinator.title,
          ),
        );
        // On a phone picking the session closes the drawer already; between the two widths it stays.
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
    scenario.steps!.push(
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
    output: (session) => {
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
      // Additional shells opened by the visitor use the fake terminal's default output.
      const found = sessions.find((candidate) => candidate.id === session);
      if (!found) return undefined;
      return createTerminalScript(
        {
          id: found.id,
          agent: found.agent,
          status: found.status === "working" || found.status === "waiting_user" ? found.status : "idle",
          cwd: projects.find((project) => project.id === found.project_id)?.path ?? workspace.workdir,
          background: found.background_tasks?.[0]?.description,
          copy: terminalCopy(found.id, copy.terminals[found.id]),
        },
        { startedAt, animate },
      );
    },
  };
}
