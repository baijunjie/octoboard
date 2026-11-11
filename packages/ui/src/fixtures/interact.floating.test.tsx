// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { App } from "../App";
import { PanelHeader } from "../components/panel/PanelHeader";
import { mountForTooltip } from "../components/hint/tooltipTestSupport";
import { resetAppPreferences } from "../appTestSupport";
import { BROWSER_HOST } from "../test/hosts";
import { PlatformProvider } from "../platform/react";
import { ThemeProvider } from "../preferences/theme";
import { DaemonProvider, type Daemon } from "../store";
import { testDaemon } from "../test/testDaemon";
import { matchesWidthQuery } from "../test/viewport";
import type { ClientRequestBody, Project } from "../protocol";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";
import { createUi } from "./interact";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
vi.setConfig({ testTimeout: 30_000 });

vi.mock("../project-browser/LazyProjectBrowser", () => ({
  LazyProjectBrowser: ({ project }: { project: Project }) => <PanelHeader title={project.name} />,
}));
vi.mock("../report/ReportPanel", () => ({ ReportPanel: () => <PanelHeader title="Report" /> }));
vi.mock("../terminal/TerminalController", () => ({
  TerminalController: class {
    constructor() {
      return new Proxy(this, { get: (target, key) => (key === "then" ? undefined : key in target ? Reflect.get(target, key) : () => {}) });
    }
  },
}));

/** A window `width` px wide, as the media queries answer width queries. */
function stubWidth(width: number) {
  const previousWidth = Object.getOwnPropertyDescriptor(document.documentElement, "clientWidth");
  Object.defineProperty(document.documentElement, "clientWidth", { configurable: true, get: () => width });
  window.dispatchEvent(new Event("resize"));
  onTestFinished(() => {
    if (previousWidth) Object.defineProperty(document.documentElement, "clientWidth", previousWidth);
    else Reflect.deleteProperty(document.documentElement, "clientWidth");
  });
  vi.stubGlobal("matchMedia", (media: string) => ({
    media,
    get matches() {
      return media === "(hover: none)" ? false : matchesWidthQuery(media, width);
    },
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
  }));
}

/** The cards on screen: a closed card fades away out of reach (inert) before it leaves. */
const cards = () => Array.from(document.querySelectorAll("[data-panel-card]")).filter((card) => !card.closest("[inert]")).map((card) => card.id);

it("shows the group a toggle press floated in for as long as the pointer rests on its card, not hiding it when the pointer leaves the toggle", async () => {
  stubWidth(900);
  document.documentElement.style.setProperty("--rail-width", "48px");
  document.documentElement.style.setProperty("--peek-gap", "8px");
  document.documentElement.style.setProperty("--breakpoint-tablet", "768px");
  document.documentElement.style.setProperty("--breakpoint-phone", "390px");
  onTestFinished(() => {
    resetAppPreferences();
    vi.unstubAllGlobals();
    for (const property of ["--rail-width", "--peek-gap", "--breakpoint-tablet", "--breakpoint-phone"]) document.documentElement.style.removeProperty(property);
    try {
      localStorage.clear();
    } catch {
      // No storage: nothing was kept.
    }
  });
  const console_ = consoleOf("c-float", "Main");
  const project = projectOf("p-site", console_.id, "Website");
  const session = sessionOf("s-site", console_.id, project.id, "Fix header", "idle");
  const daemon = testDaemon({
    state: snapshotState({ consoles: [console_], projects: [project], sessions: [session] }),
    request: vi.fn(async (body: ClientRequestBody) => (body.type === "list_processes" ? { type: "processes_listed", id: null, trees: [] } : { type: "ack", id: null })) as Daemon["request"],
  });
  mountForTooltip(
    <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}>
      <ThemeProvider><DaemonProvider value={daemon}><App /></DaemonProvider></ThemeProvider>
    </PlatformProvider>,
  );
  const ui = createUi(document, "en");
  await act(async () => ui.press(ui.session("Fix header")));
  await act(async () => ui.showPanel("files"));
  expect(cards()).toEqual(["panel-files"]);
  // The group's hide delay after a pointer leaves is 200 ms; the pointer is on the card, so it does not run.
  await act(async () => new Promise((resolve) => setTimeout(resolve, 600)));
  expect(cards()).toEqual(["panel-files"]);
});
