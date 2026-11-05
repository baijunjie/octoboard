// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { App } from "./App";
import { mountForTooltip } from "./components/hint/tooltipTestSupport";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./fixtures/builders";
import { PlatformProvider } from "./platform/react";
import type { Account } from "./protocol";
import { DaemonProvider, type Daemon } from "./store";
import { testDaemon } from "./test/testDaemon";
import { ThemeProvider } from "./preferences/theme";
import { BROWSER_HOST, IS_MAC } from "./test/hosts";
import { matchesWidthQuery } from "./test/viewport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

vi.mock("./report/ReportPanel", () => ({ ReportPanel: () => null }));
vi.mock("./terminal/TerminalController", () => ({
  TerminalController: class {
    mount() {}
    dispose() {}
    fit() {}
    syncSize() {}
    setInputLabel() {}
    detach() {}
    focus() {}
  },
}));

const ASKING = '[role="alertdialog"]';
const account = (incomplete: boolean): Account => ({ id: "a-half", agent: "claude", name: "Half", config_dir: "/a/half", share_default_config: true, share_default_config_incomplete: incomplete, disabled: false });

function mountWith(incomplete: boolean) {
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: matchesWidthQuery(media, 1440), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  onTestFinished(() => {
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--rail-width");
    document.documentElement.style.removeProperty("--breakpoint-tablet");
    document.documentElement.style.removeProperty("--breakpoint-phone");
  });
  document.documentElement.style.setProperty("--rail-width", "48px");
  document.documentElement.style.setProperty("--breakpoint-tablet", "768px");
  document.documentElement.style.setProperty("--breakpoint-phone", "390px");
  const console_ = consoleOf("c-main", "Main", { accounts: { claude: "a-half" } });
  const state = snapshotState({
    consoles: [console_],
    projects: [projectOf("p-1", console_.id, "Search API")],
    sessions: [sessionOf("s-1", console_.id, undefined, "Half session", "interrupted", { account_id: "a-half" })],
  });
  const daemon = testDaemon({
    state,
    request: vi.fn(async () => ({ type: "ack", id: null })) as Daemon["request"],
    showNotice: vi.fn(),
  });
  daemon.store.setState({ settings: { ...daemon.store.getState().settings, accounts: [account(incomplete)] } });
  const container = mountForTooltip(
    <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}>
      <ThemeProvider>
        <DaemonProvider value={daemon}><App /></DaemonProvider>
      </ThemeProvider>
    </PlatformProvider>,
  );
  const resumes = () => vi.mocked(daemon.request).mock.calls.filter(([body]) => body.type === "resume_session");
  const opens = () => vi.mocked(daemon.request).mock.calls.filter(([body]) => body.type === "open_session");
  // The sidebar's New session button with the shortcut modifier held, which quick-creates.
  const quickCreate = () => act(async () => (container.querySelector('[data-region="sidebar"] button[aria-label="New session"]') as HTMLElement)
    .dispatchEvent(new MouseEvent("click", { bubbles: true, ...(IS_MAC ? { metaKey: true } : { ctrlKey: true }) })));
  const select = () => act(async () => (container.querySelector('[data-region="sidebar"] [role="button"][aria-label^="\u2068Half session"]') as HTMLElement).click());
  return { container, resumes, opens, quickCreate, select };
}

it("resuming an interrupted session on an incomplete account asks first; closing sends nothing and Start anyway resumes", async () => {
  const { resumes, select } = mountWith(true);
  await select();
  expect(document.body.querySelector(ASKING)).not.toBeNull();
  expect(resumes()).toHaveLength(0);
  await act(async () => (document.body.querySelector(`${ASKING} button[aria-label="Close"]`) as HTMLElement).click());
  expect(document.body.querySelector(ASKING)).toBeNull();
  expect(resumes()).toHaveLength(0);
  await select();
  await act(async () => Array.from(document.body.querySelectorAll<HTMLElement>(`${ASKING} button`)).find((button) => button.textContent === "Start anyway")!.click());
  expect(resumes()).toEqual([[{ type: "resume_session", session: "s-1", terminal_appearance: "light" }]]);
});

it("resumes an interrupted session on a complete account without asking", async () => {
  const { resumes, select } = mountWith(false);
  await select();
  expect(document.body.querySelector(ASKING)).toBeNull();
  expect(resumes()).toHaveLength(1);
});

it("quick-creating a session on an incomplete account asks first; closing sends nothing and Start anyway opens it", async () => {
  const { opens, quickCreate } = mountWith(true);
  await quickCreate();
  expect(document.body.querySelector(ASKING)).not.toBeNull();
  expect(opens()).toHaveLength(0);
  await act(async () => (document.body.querySelector(`${ASKING} button[aria-label="Close"]`) as HTMLElement).click());
  expect(opens()).toHaveLength(0);
  await quickCreate();
  await act(async () => Array.from(document.body.querySelectorAll<HTMLElement>(`${ASKING} button`)).find((button) => button.textContent === "Start anyway")!.click());
  expect(opens()).toEqual([[expect.objectContaining({ type: "open_session", account: "a-half" })]]);
});
