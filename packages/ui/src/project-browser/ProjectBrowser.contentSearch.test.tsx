// @vitest-environment jsdom
import { act, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { useFindInFilesShortcut } from "../app/useFindInFilesShortcut";
import { PlatformProvider } from "../platform/react";
import type { ClientRequestBody, Event, Project } from "../protocol";
import { defaultBinding } from "../shortcuts/commands";
import { ThemeProvider } from "../preferences/theme";
import { DaemonProvider } from "../store";
import { fakeTimers } from "../test/clock";
import { BROWSER_HOST, IS_MAC } from "../test/hosts";
import { type } from "../test/dom";
import type { ViewedFile } from "../viewer/viewedFile";
import { contentMatch, contentSearchDaemon, modalPage } from "./files/contentSearch/contentSearchTestSupport";
import { SEARCH_DEBOUNCE_MS } from "./files/useFileSearch";
import { useProjectBrowserState } from "./browserState";
import { ProjectBrowser } from "./ProjectBrowser";

// These checks are about opening Find in files and a result; the viewer's rendering has its own suite.
vi.mock("../viewer/FileViewer", () => ({ FileViewer: () => null }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const root = (project: string): Event => ({
  type: "project_dir",
  id: null,
  project,
  worktree: null,
  path: "",
  root_id: "r",
  entries: [{ name: "src", kind: "directory", size: null, version: null, target: null }],
  complete: true,
});

function Shortcut() {
  useFindInFilesShortcut();
  return null;
}

async function mount({ mode = "files", active = true }: { mode?: "files" | "git"; active?: boolean } = {}) {
  modalPage();
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  onTestFinished(() => { vi.unstubAllGlobals(); });
  // Everything but the listing of the root and the content search waits for good.
  const fake = contentSearchDaemon({ request: (body: ClientRequestBody) => (body.type === "list_project_dir" && body.path === "" ? Promise.resolve(root(body.project)) : new Promise<Event>(() => {})) });
  const onViewFile = vi.fn<(file: ViewedFile, returnFocus?: () => boolean) => void>();
  const container = document.body.appendChild(document.createElement("div"));
  const view = createRoot(container);
  function Browser() {
    const browser = useProjectBrowserState("p");
    useEffect(() => browser.setMode(mode), []);
    return <ProjectBrowser project={{ id: "p", name: "demo" } as Project} active={active} selectedSessionId={undefined} onViewFile={onViewFile} />;
  }
  await act(async () =>
    view.render(
      <ThemeProvider><DaemonProvider value={fake.daemon}>
        <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}>
          <Shortcut />
          <section data-region="panel-files">
            <Browser />
          </section>
        </PlatformProvider>
      </DaemonProvider></ThemeProvider>,
    ),
  );
  onTestFinished(() => {
    act(() => view.unmount());
    container.remove();
  });
  return { fake, onViewFile };
}

const dialog = () => document.querySelector<HTMLElement>("[role=dialog]");
const query = () => document.querySelector<HTMLInputElement>('input[aria-label="Search file contents"]')!;
const findButton = () => document.querySelector<HTMLButtonElement>('button[aria-label="Find in files"]');

it("opens Find in files on the whole project from the button beside the filter", async () => {
  fakeTimers();
  const { fake } = await mount();
  expect(findButton()).not.toBeNull();
  // The button follows the filter's toggle in the header.
  const filter = document.querySelector<HTMLButtonElement>('button[aria-label="Filter by file name"]')!;
  expect(filter.compareDocumentPosition(findButton()!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  await act(async () => findButton()!.click());
  expect(dialog()?.textContent).toContain("Find in files");
  expect(document.querySelector("[data-content-search-scope]")?.textContent).toBe("In demo");
  type(query(), "foo");
  await act(async () => { vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS); });
  expect(fake.latest()).not.toHaveProperty("directory");
});

it("opens it on the whole project from the window's shortcut", async () => {
  fakeTimers();
  await mount();
  const binding = defaultBinding("findInFiles");
  expect(binding).toEqual({ code: "KeyF", primary: true, shift: true });
  await act(async () => {
    window.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyF", key: "F", shiftKey: true, metaKey: IS_MAC, ctrlKey: !IS_MAC, bubbles: true, cancelable: true }));
  });
  expect(dialog()?.textContent).toContain("Find in files");
  expect(document.querySelector("[data-content-search-scope]")?.textContent).toBe("In demo");
});

it("opens it limited to a folder from the folder's actions, showing the folder", async () => {
  fakeTimers();
  const { fake } = await mount();
  const row = [...document.querySelectorAll<HTMLElement>("[role=row]")].find((element) => element.textContent?.includes("src"))!;
  expect(row).toBeDefined();
  await act(async () => row.querySelector<HTMLButtonElement>("button[aria-haspopup]")!.click());
  const item = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.textContent === "Find in folder");
  expect(item).toBeDefined();
  await act(async () => item!.click());
  expect(document.querySelector("[data-content-search-scope]")?.textContent).toBe("In demo / src");
  type(query(), "foo");
  await act(async () => { vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS); });
  expect(fake.latest()).toMatchObject({ directory: "src" });
});

it("closes the modal and opens a result in the viewer at its line, with its matches marked", async () => {
  fakeTimers();
  const { fake, onViewFile } = await mount();
  await act(async () => findButton()!.click());
  type(query(), "foo");
  await act(async () => { vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS); });
  const match = contentMatch("src/a.ts", 7, "let foo = 1;", "foo");
  await act(async () => fake.matches([match]));
  await act(async () => {
    query().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  });
  expect(onViewFile).toHaveBeenCalledTimes(1);
  const [file, returnFocus] = onViewFile.mock.calls[0];
  expect(file).toEqual({ project: "p", path: "src/a.ts", line: 7, marks: match.ranges });
  // The very ranges the daemon sent, so the viewer marks them without a new array each render.
  expect((file as { marks: unknown }).marks).toBe(match.ranges);
  await act(async () => { vi.runOnlyPendingTimers(); });
  expect(dialog()).toBeNull();
  // Focus goes back to the button once the viewer closes.
  let took = false;
  act(() => { took = returnFocus!(); });
  expect(took).toBe(true);
  expect(document.activeElement).toBe(findButton());
});

const toggle = (label: string) => document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;

it("keeps the query and options for the browser's lifetime, and shows them when the modal is opened again, on another scope too", async () => {
  fakeTimers();
  const { fake } = await mount();
  await act(async () => findButton()!.click());
  type(query(), "needle");
  act(() => toggle("Match case").click());
  type(document.querySelector<HTMLInputElement>('input[aria-label="File mask"]')!, "*.ts");
  await act(async () => { vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS); });
  expect(fake.latest()).toMatchObject({ query: "needle", match_case: true, file_mask: "*.ts" });
  const close = [...document.querySelectorAll<HTMLButtonElement>("[role=dialog] button")].find((button) => button.getAttribute("aria-label") === "Close")!;
  expect(close).toBeDefined();
  await act(async () => close.click());
  await act(async () => { vi.runOnlyPendingTimers(); });
  expect(dialog()).toBeNull();

  await act(async () => findButton()!.click());
  expect(query().value).toBe("needle");
  expect(toggle("Match case").getAttribute("aria-pressed")).toBe("true");
  expect(document.querySelector<HTMLInputElement>('input[aria-label="File mask"]')!.value).toBe("*.ts");
  await act(async () => { vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS); });
  expect(fake.latest()).toMatchObject({ query: "needle", match_case: true, file_mask: "*.ts" });
  expect(fake.latest()).not.toHaveProperty("directory");
});

it("offers no Find in files button, nor a folder's Find in folder, in the Git mode", async () => {
  fakeTimers();
  await mount({ mode: "git" });
  expect(document.querySelector('[role="tab"][aria-selected="true"]')?.getAttribute("data-key")).not.toBe("files");
  // The header's other controls are there, so the button is missing and not the header.
  expect(document.querySelector('button[aria-label="Filter by file name"]')).not.toBeNull();
  expect(findButton()).toBeNull();
});

it("switches to the Files mode from the window's shortcut in the Git mode, so focus comes back to the button after a result opens", async () => {
  fakeTimers();
  const { fake, onViewFile } = await mount({ mode: "git" });
  expect(findButton()).toBeNull();
  await act(async () => {
    window.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyF", key: "F", shiftKey: true, metaKey: IS_MAC, ctrlKey: !IS_MAC, bubbles: true, cancelable: true }));
  });
  expect(dialog()?.textContent).toContain("Find in files");
  expect(document.querySelector('[role="tab"][aria-selected="true"]')?.getAttribute("data-key")).toBe("files");
  expect(findButton()).not.toBeNull();
  type(query(), "foo");
  await act(async () => { vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS); });
  await act(async () => fake.matches([contentMatch("src/a.ts", 7, "let foo = 1;", "foo")]));
  await act(async () => {
    query().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  });
  await act(async () => { vi.runOnlyPendingTimers(); });
  let took = false;
  act(() => { took = onViewFile.mock.calls[0]![1]!(); });
  expect(took).toBe(true);
  expect(document.activeElement).toBe(findButton());
});

it("leaves the window's shortcut alone while its project browser is not the one on screen", async () => {
  fakeTimers();
  await mount({ active: false });
  await act(async () => {
    window.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyF", key: "F", shiftKey: true, metaKey: IS_MAC, ctrlKey: !IS_MAC, bubbles: true, cancelable: true }));
  });
  expect(dialog()).toBeNull();
});
