import type { Language } from "../i18n/languages";
import type { Scenario } from "../gallery/scenario";
import {
  consoleOf,
  gitStatusOf,
  pageOf,
  projectOf,
  sessionOf,
  snapshotState,
} from "../gallery/fixtures/builders";
import { previewRepositories } from "./repositories";
import { previewCopy as english } from "./locales/en";
import { previewCopy as chinese } from "./locales/zh-Hans";

export type PreviewScene = "overview" | "report" | "changes";
export const copyFor = (language: Language) =>
  language === "zh-Hans" ? chinese : english;

function report(copy: typeof english): string {
  // This is sample agent-authored report content, rendered by the application's real report panel.
  const rows = [
    [copy.projectAdmin, copy.adminResult],
    [copy.projectMerchant, copy.merchantResult],
    [copy.projectCustomer, copy.customerResult],
    [copy.projectServer, copy.serverResult],
    [copy.projectMobile, copy.mobileResult],
    [copy.projectTablet, copy.tabletResult],
    [copy.projectPos, copy.posResult],
    [copy.projectKitchen, copy.kitchenResult],
  ];
  return `<!doctype html><html><head><title>${copy.promotionReportTitle}</title><style>
    body{font:14px/1.6 system-ui,sans-serif;margin:0;padding:24px;color:#253044;background:#fff}
    small{color:#667085}h1{font-size:26px;line-height:1.2;margin:12px 0}h2{font-size:16px;margin:24px 0 8px}
    .status{display:inline-block;background:#e4f4ed;color:#136845;padding:3px 9px;border-radius:6px;font-size:12px}
    .stats{display:flex;gap:20px;border-block:1px solid #e7e9ed;padding:16px 0;margin:20px 0}
    .stats div{flex:1}.stats strong{display:block;font-size:26px;line-height:1.2}.stats span{font-size:11px;color:#667085}
    article{padding:12px 0;border-bottom:1px solid #e7e9ed}article strong{font-size:13px}article p{margin:3px 0;color:#536070}
    .decision{padding:14px;background:#fff6e5;border-radius:8px;color:#715014}
  </style></head><body data-website-preview-report><small>${copy.reportSubtitle}</small><h1>${copy.promotionReportTitle}</h1>
  <span class="status">${copy.ready}</span><p>${copy.promotionReportIntro}</p>
  <div class="stats"><div><strong>${rows.length}</strong><span>${copy.projects}</span></div><div><strong>48</strong><span>${copy.checks}</span></div><div><strong>1</strong><span>${copy.decision}</span></div></div>
  ${rows.map(([title, content]) => `<article><strong>${title}</strong><p>${content}</p></article>`).join("")}
  <h2>${copy.decision}</h2><p class="decision">${copy.stackingDecision}</p><h2>${copy.next}</h2><p>${copy.promotionNext}</p>
  </body></html>`;
}

export function createPreviewScenario(
  scene: PreviewScene,
  language: Language,
): { scenario: Scenario; output: (session: string) => string | undefined } {
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
    scene === "overview" ? "working" : "idle",
    { colour: "teal" },
  );
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
      scene === "overview" ? "working" : "idle",
      { agent: "claude", bound_to: coordinator.id },
    ),
    sessionOf(
      "preview-customer-session",
      workspace.id,
      projects[2].id,
      copy.customerTask,
      scene === "overview" ? "working" : "idle",
      { agent: "grok", bound_to: coordinator.id },
    ),
    sessionOf(
      "preview-server-session",
      workspace.id,
      projects[3].id,
      copy.serverTask,
      scene === "overview" ? "working" : "idle",
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
      scene === "overview" ? "working" : "idle",
      { agent: "claude", bound_to: coordinator.id },
    ),
    sessionOf(
      "preview-tablet-session",
      workspace.id,
      projects[5].id,
      copy.tabletTask,
      scene === "overview" ? "working" : "idle",
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
      scene === "overview" ? "working" : "idle",
      { agent: "claude", bound_to: coordinator.id },
    ),
  ];
  const scenario: Scenario = {
    id: `website-${scene}`,
    group: "Focus mode",
    category: "normal",
    title: copy.demo,
    state: snapshotState({
      consoles: [workspace],
      projects,
      sessions: [coordinator, ...projectSessions],
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
    }),
    preferences: {
      sidebarConsole: workspace.id,
      sidebarWidth: scene === "overview" ? 360 : 310,
      asideWidth: 430,
      // The overview shows the terminal alone: no panel of either owner is open.
      ...(scene === "overview"
        ? { panelStacks: { project: { open: [], shares: {} }, report: { open: [], shares: {} } } }
        : {}),
      changeLayout: "tree",
    },
    configuration: { unavailable: true },
    clipboard: "unavailable",
    steps: [
      async (ui) => {
        // Match the application's CSS breakpoint: the standalone demo also runs on phones, where
        // the sessions list is a drawer that has to be opened to select a session and closed to see
        // the report or browser, which dock beside the terminal at every width.
        const breakpoint = parseFloat(
          getComputedStyle(document.documentElement).getPropertyValue(
            "--breakpoint-docked",
          ),
        );
        const narrow = !window.matchMedia(`(min-width: ${breakpoint}px)`)
          .matches;
        if (narrow) await ui.press(ui.t("titleBar.sidebar.show"));
        await ui.press(
          ui.session(scene === "changes" ? copy.review : coordinator.title),
        );
        if (narrow) await ui.press(ui.t("titleBar.sidebar.hide"));
      },
    ],
  };
  if (scene === "changes")
    scenario.steps!.push(
      (ui) => ui.press(ui.t("git.view.changes")),
      (ui) => ui.press(ui.changeRow("meal-deal-orders.ts", "modified")),
      (ui) => ui.wait(600),
    );
  const sessionIds = new Set([coordinator.id, ...projectSessions.map((session) => session.id)]);
  const cyan = "\x1b[38;5;80m";
  const reset = "\x1b[0m";
  return {
    scenario,
    output: (session) => {
      // A shell terminal of the Terminal panel is not a session: it is left to the fake terminal's shell.
      if (!sessionIds.has(session)) return undefined;
      const lines =
        session === coordinator.id
          ? scene === "overview"
            ? copy.terminalOverview
            : copy.terminalReport
          : (copy.sessionUpdates[session] ?? copy.terminalChanges);
      return (
        [`${cyan}${copy.demo}${reset}`, "", ...lines].join("\r\n") + "\r\n"
      );
    },
  };
}
