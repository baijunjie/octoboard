// @vitest-environment jsdom
// What the viewer offers for a live file outside itself: Download and open in a remote window, Default application and
// File manager locally, the stand-ins of a file it cannot show or finds too large, and the copy's life with the viewer.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { CodedError } from "../errors/codedError";
import { formatFileSize } from "../format/fileSize";
import type { PlatformAdapter, RemoteFilesCapability } from "../platform";
import { RemoteFileError } from "../platform";
import { ThemeProvider } from "../preferences/theme";
import { BROWSER_HOST } from "../test/hosts";
import { testDaemon } from "../test/testDaemon";
import { TestHost } from "../test/TestHost";
import type { ViewerBody, ViewerSubject } from "./content";
import { FileViewer } from "./FileViewer";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

window.matchMedia ??= ((query: string) =>
  ({ matches: false, media: query, addEventListener() {}, removeEventListener() {} }) as unknown as MediaQueryList);

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  document.documentElement.style.setProperty("--breakpoint-tablet", "768px");
  document.documentElement.style.setProperty("--breakpoint-phone", "390px");
  localStorage.clear();
  container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
});

afterEach(() => {
  vi.useRealTimers();
  act(() => root.unmount());
  container.remove();
  while (errorAlerts.getSnapshot().length > 0) act(() => errorAlerts.acknowledge());
});

const BIN: ViewerBody = { kind: "binary", size: 10 };
const LARGE: ViewerBody = { kind: "tooLarge", size: 9_000_000 };

const projectFile = (body: ViewerBody | undefined, over: Partial<ViewerSubject> = {}, path = "assets/a.dat"): ViewerSubject => ({
  key: `p\0${path}`,
  path,
  host: { project: "p", path },
  content: body ? { state: "file", body } : { state: "loading" },
  ...over,
});
const linkedFile = (body: ViewerBody | undefined, over: Partial<ViewerSubject> = {}, path = "/tmp/out/a.dat"): ViewerSubject => ({
  key: `link\0${path}`,
  path,
  linkHost: { project: "p", path },
  content: body ? { state: "file", body } : { state: "loading" },
  ...over,
});

/** The daemon's side of Download and open: `bytes` served in parts of `part` bytes. */
function daemon(bytes: Uint8Array, { part = 4, version = "v1" }: { part?: number; version?: string } = {}) {
  const handler = vi.fn(async (body: { type: string; offset?: number }) => {
    if (body.type !== "read_file_range") return { type: "ack", id: null };
    const offset = body.offset ?? 0;
    const slice = bytes.slice(offset, offset + part);
    return { type: "file_range", id: null, offset, size: bytes.length, version, data: btoa(String.fromCharCode(...slice)) };
  });
  return { handler };
}

/** The shell's files commands; `hold` makes `write` wait for `release()`. */
function shell() {
  const stored: number[] = [];
  const writes: (() => void)[] = [];
  const state = { hold: false };
  const files = {
    begin: vi.fn(async () => "copy"),
    write: vi.fn(async (_id: string, part: Uint8Array) => {
      stored.push(...part);
      if (state.hold) await new Promise<void>((resolve) => writes.push(resolve));
    }),
    open: vi.fn(async () => {}),
    discard: vi.fn(async () => {}),
  } satisfies RemoteFilesCapability;
  return { files, stored, state, release: () => act(async () => writes.splice(0).forEach((resolve) => resolve())) };
}

type Mount = {
  isLocal?: boolean;
  remoteFiles?: RemoteFilesCapability;
  request?: (body: never) => Promise<unknown>;
  state?: Record<string, unknown>;
  clipboard?: PlatformAdapter["clipboard"];
  /** The system of this computer, the desktop's. */
  desktop?: "macos" | "windows";
};

function show(subject: ViewerSubject, { isLocal = false, remoteFiles, request, state, clipboard, desktop }: Mount = {}) {
  const host = desktop ? { ...BROWSER_HOST, os: desktop } : BROWSER_HOST;
  const platform: PlatformAdapter = { kind: remoteFiles ? "tauri" : "browser", host, remoteFiles, clipboard };
  const daemonHandle = testDaemon({ isLocal, request: request as never, state: state as never });
  act(() => {
    root.render(
      <TestHost daemon={daemonHandle} platform={platform}>
        <ThemeProvider>
          <FileViewer subject={subject} onClose={() => {}} />
        </ThemeProvider>
      </TestHost>,
    );
  });
}

const dialog = () => document.querySelector<HTMLElement>("[role=dialog]")!;
const headerButtons = () => [...dialog().querySelector("[data-viewer-description]")!.querySelectorAll("button")].map((b) => b.getAttribute("aria-label"));
const headerButton = (label: string) => dialog().querySelector<HTMLButtonElement>(`[data-viewer-description] button[aria-label="${label}"]`)!;
const standIn = () => dialog().querySelector<HTMLElement>("[data-find-content]")!;
const standInButtons = () => [...standIn().querySelectorAll("button")];
const standInButton = (text: string) => standInButtons().find((b) => b.textContent?.includes(text));
const progress = () => standIn().querySelector<HTMLElement>('[role="progressbar"]');
const settle = () => act(async () => {});
/** A pending button is exposed as unavailable (`aria-disabled`) and busy. */
const unavailable = (button: HTMLButtonElement) => button.disabled || button.getAttribute("aria-disabled") === "true";

// ---- A remote window: Download and open

it("offers Download and open in the header of a remote window for a live file, in place of File manager and Default application", () => {
  const { files } = shell();
  for (const subject of [projectFile(undefined), linkedFile(undefined), projectFile({ kind: "text", text: "x", size: 1 })]) {
    show(subject, { remoteFiles: files });
    const labels = headerButtons();
    expect(labels).toContain("Download and open");
    expect(labels).not.toContain("Open in file manager");
    expect(labels).not.toContain("Open in default application");
    // It ends the controls, where the two stand locally.
    expect(labels.at(-1)).toBe("Download and open");
  }
});

it("offers it for no subject that is not a live file, and for none the system could launch, by the daemon's word or by name", () => {
  const { files } = shell();
  show({ key: "c", path: "a.txt", content: { state: "file", body: { kind: "text", text: "x", size: 1 } } }, { remoteFiles: files });
  expect(headerButtons()).not.toContain("Download and open");
  show(projectFile(undefined, { launchable: true }), { remoteFiles: files });
  expect(headerButtons()).not.toContain("Download and open");
  show(linkedFile(undefined, { launchable: true }), { remoteFiles: files });
  expect(headerButtons()).not.toContain("Download and open");
  for (const path of ["tools/run.sh", "Setup.EXE", "x.py"]) {
    show(projectFile(undefined, {}, path), { remoteFiles: files });
    expect(headerButtons(), path).not.toContain("Download and open");
  }
});

it("offers nothing outside the viewer in a browser on another machine, whose page has no shell to copy to", () => {
  show(projectFile(BIN), { isLocal: false });
  const labels = headerButtons();
  for (const label of ["Download and open", "Open in file manager", "Open in default application"]) expect(labels).not.toContain(label);
  expect(dialog().textContent).toContain("This file cannot be displayed");
  expect(standInButtons()).toEqual([]);
});

it.each([
  ["a project file", projectFile(undefined), { type: "read_file_range", file: { kind: "project", project: "p", path: "assets/a.dat" } }],
  ["a project file of a worktree", projectFile(undefined, { host: { project: "p", worktree: "w", path: "assets/a.dat" } }), { type: "read_file_range", file: { kind: "project", project: "p", worktree: "w", path: "assets/a.dat" } }],
  ["a file a path link names", linkedFile(undefined), { type: "read_file_range", file: { kind: "path_link", project: "p", path: "/tmp/out/a.dat" } }],
])("copies %s through the daemon and the shell when the header's button is pressed, then opens the copy and says nothing", async (_name, subject, firstRequest) => {
  const bytes = Uint8Array.from({ length: 10 }, (_, i) => i);
  const d = daemon(bytes);
  const { files, stored } = shell();
  show(subject, { remoteFiles: files, request: d.handler as never });
  act(() => headerButton("Download and open").click());
  await settle();
  expect(d.handler.mock.calls[0][0]).toMatchObject(firstRequest);
  expect(d.handler.mock.calls.slice(1).every(([body]) => (body as { version?: string }).version === "v1")).toBe(true);
  expect(files.begin).toHaveBeenCalledExactlyOnceWith({ name: "a.dat", size: 10, launchable: false });
  expect(stored).toEqual([...bytes]);
  expect(files.open).toHaveBeenCalledExactlyOnceWith("copy");
  expect(files.discard).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()).toEqual([]);
  expect(unavailable(headerButton("Download and open"))).toBe(false);
});

it("makes the header's button busy while the file is copied, and a second press begins no second copy", async () => {
  const d = daemon(new Uint8Array(10));
  const { files, state, release } = shell();
  state.hold = true;
  show(projectFile(undefined), { remoteFiles: files, request: d.handler as never });
  act(() => headerButton("Download and open").click());
  await settle();
  const busy = headerButton("Download and open");
  expect(unavailable(busy)).toBe(true);
  expect(busy.getAttribute("aria-busy")).toBe("true");
  act(() => busy.click());
  await settle();
  expect(files.begin).toHaveBeenCalledOnce();
  state.hold = false;
  await release();
  await settle();
  expect(files.open).toHaveBeenCalledOnce();
  expect(unavailable(headerButton("Download and open"))).toBe(false);
  expect(headerButton("Download and open").getAttribute("aria-busy")).not.toBe("true");
});

// ---- The stand-ins

it.each([
  ["cannot be displayed", BIN, "This file cannot be displayed", formatFileSize("en", 10)],
  ["is too large to display", LARGE, "This file is too large to display", formatFileSize("en", 9_000_000)],
])("says a file that %s with its size, and in a remote window offers Download and open under it", (_name, body, message, size) => {
  const { files } = shell();
  show(projectFile(body), { remoteFiles: files });
  expect(standIn().textContent).toContain(message);
  expect(standIn().textContent).toContain(size);
  expect(standInButtons().map((b) => b.textContent)).toEqual(["Download and open"]);
});

it("offers no button under a stand-in for what the system could launch, or for what is not a live file", () => {
  const { files } = shell();
  for (const subject of [projectFile(BIN, { launchable: true }), projectFile(BIN, {}, "tool.py"), { key: "c", path: "a.dat", content: { state: "file", body: BIN } } as ViewerSubject]) {
    show(subject, { remoteFiles: files });
    expect(standIn().textContent).toContain("This file cannot be displayed");
    expect(standInButtons()).toEqual([]);
    show(subject, { isLocal: true });
    expect(standInButtons()).toEqual([]);
  }
});

it("shows how far the copy has come in the stand-in's place of its button, with Cancel, and tells assistive technology", async () => {
  const d = daemon(Uint8Array.from({ length: 10 }, (_, i) => i));
  const { files, state, release } = shell();
  state.hold = true;
  show(projectFile(BIN), { remoteFiles: files, request: d.handler as never });
  act(() => standInButton("Download and open")!.click());
  await settle();
  expect(standInButton("Download and open")).toBeUndefined();
  expect(standInButtons().map((b) => b.textContent)).toEqual(["Cancel"]);
  // Until the first part is in, the size is not known: a bar with no amount.
  expect(progress()!.getAttribute("aria-valuetext")).toBeNull();
  // The bar is described by the text saying what it waits for.
  expect(document.getElementById(progress()!.getAttribute("aria-describedby")!)!.textContent).toBe("Size not known yet");
  await release();
  await settle();
  const bar = progress()!;
  expect(bar.hasAttribute("aria-describedby")).toBe(false);
  expect(bar.getAttribute("aria-valuemax")).toBe("10");
  expect(bar.getAttribute("aria-valuenow")).toBe("4");
  expect(bar.getAttribute("aria-valuetext")).toBe(`${formatFileSize("en", 4)} of ${formatFileSize("en", 10)}`);
  const text = standIn().textContent!.replace(/[⁨⁩]/g, "");
  expect(text).toContain("Downloading a.dat");
  expect(text).toContain(`${formatFileSize("en", 4)} of ${formatFileSize("en", 10)}`);
  state.hold = false;
  await release();
  await settle();
  expect(files.open).toHaveBeenCalledOnce();
  expect(progress()).toBeNull();
  expect(standInButton("Download and open")).toBeDefined();
});

const spoken = () => [...standIn().querySelectorAll('[role="status"]')].map((region) => region.textContent).join("").replace(/[⁨⁩]/g, "");

it("announces the start of a copy once, after the focus move when the stand-in's button began it and soon after when the header did", async () => {
  vi.useFakeTimers();
  const d = daemon(new Uint8Array(10));
  const { files, state } = shell();
  state.hold = true;
  show(projectFile(BIN), { remoteFiles: files, request: d.handler as never });
  act(() => standInButton("Download and open")!.focus());
  act(() => standInButton("Download and open")!.click());
  await settle();
  await act(async () => void vi.advanceTimersByTime(1000));
  expect(spoken()).toBe("");
  await act(async () => void vi.advanceTimersByTime(600));
  expect(spoken()).toBe("Downloading a.dat");
  act(() => standInButton("Cancel")!.click());
  await settle();
  act(() => headerButton("Download and open").click());
  await settle();
  await act(async () => void vi.advanceTimersByTime(200));
  expect(spoken()).toBe("Downloading a.dat");
});

it("draws the progress of a copy as valid markup: a block is no child of a paragraph", async () => {
  const d = daemon(new Uint8Array(10));
  const { files, state } = shell();
  state.hold = true;
  show(projectFile(BIN), { remoteFiles: files, request: d.handler as never });
  act(() => standInButton("Download and open")!.click());
  await settle();
  expect(progress()).not.toBeNull();
  expect(progress()!.closest("p")).toBeNull();
});

it("ends the copy at Cancel, removing what had arrived and saying nothing, and puts focus back on the button that began it", async () => {
  const d = daemon(new Uint8Array(10));
  const { files, state, release } = shell();
  state.hold = true;
  show(projectFile(BIN), { remoteFiles: files, request: d.handler as never });
  // Focus is on the button that begins the copy, which gives way to the progress with Cancel.
  act(() => standInButton("Download and open")!.focus());
  act(() => standInButton("Download and open")!.click());
  await settle();
  expect(document.activeElement).toBe(standInButton("Cancel"));
  act(() => standInButton("Cancel")!.click());
  expect(progress()).toBeNull();
  expect(document.activeElement).toBe(standInButton("Download and open"));
  // The part still on its way is waited out, then the copy removed; it is never opened.
  state.hold = false;
  await release();
  await settle();
  expect(files.discard).toHaveBeenCalledExactlyOnceWith("copy");
  expect(files.open).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()).toEqual([]);
  // Choosing it again copies the file again.
  act(() => standInButton("Download and open")!.click());
  await settle();
  expect(files.begin).toHaveBeenCalledTimes(2);
  expect(files.open).toHaveBeenCalledOnce();
});

it("leaves focus where it is when the copy is begun from the header, and hands it back to the button that began a copy from the stand-in when the copy is done", async () => {
  const d = daemon(new Uint8Array(10));
  const { files, state, release } = shell();
  state.hold = true;
  show(projectFile(BIN), { remoteFiles: files, request: d.handler as never });
  act(() => headerButton("Download and open").focus());
  act(() => headerButton("Download and open").click());
  await settle();
  expect(progress()).not.toBeNull();
  expect(document.activeElement).toBe(headerButton("Download and open"));
  act(() => standInButton("Cancel")!.click());
  state.hold = false;
  await release();
  await settle();
  // Begun from the stand-in and finished: Cancel goes, and focus is the button's again.
  act(() => standInButton("Download and open")!.focus());
  act(() => standInButton("Download and open")!.click());
  await settle();
  await release();
  await settle();
  expect(files.open).toHaveBeenCalledOnce();
  expect(document.activeElement).toBe(standInButton("Download and open"));
});

it("cancels from the header's busy state too: the header and the stand-in share one copy", async () => {
  const d = daemon(new Uint8Array(10));
  const { files, state, release } = shell();
  state.hold = true;
  show(projectFile(BIN), { remoteFiles: files, request: d.handler as never });
  act(() => headerButton("Download and open").click());
  await settle();
  expect(progress()).not.toBeNull();
  expect(unavailable(headerButton("Download and open"))).toBe(true);
  act(() => standInButton("Cancel")!.click());
  expect(unavailable(headerButton("Download and open"))).toBe(false);
  state.hold = false;
  await release();
  await settle();
  expect(files.begin).toHaveBeenCalledOnce();
  expect(files.discard).toHaveBeenCalledOnce();
});

it("ends the copy when the viewer closes", async () => {
  const d = daemon(new Uint8Array(10));
  const { files, state, release } = shell();
  state.hold = true;
  show(projectFile(BIN), { remoteFiles: files, request: d.handler as never });
  act(() => standInButton("Download and open")!.click());
  await settle();
  act(() => root.render(<div />));
  state.hold = false;
  await release();
  await settle();
  expect(files.discard).toHaveBeenCalledExactlyOnceWith("copy");
  expect(files.open).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("ends the copy when the viewer moves to another file, whose stand-in is idle", async () => {
  const d = daemon(new Uint8Array(10));
  const { files, state, release } = shell();
  state.hold = true;
  show(projectFile(BIN), { remoteFiles: files, request: d.handler as never });
  act(() => standInButton("Download and open")!.click());
  await settle();
  show(projectFile(BIN, {}, "assets/b.dat"), { remoteFiles: files, request: d.handler as never });
  expect(progress()).toBeNull();
  expect(standInButton("Download and open")).toBeDefined();
  state.hold = false;
  await release();
  await settle();
  expect(files.discard).toHaveBeenCalledExactlyOnceWith("copy");
  expect(files.open).not.toHaveBeenCalled();
});

it.each([
  ["what this computer would run", new RemoteFileError("launchable"), "Could not download the file"],
  ["a copy that could not be written", new RemoteFileError("storage", "No space left on device (os error 28)"), "Could not download the file"],
  ["a copy that no application opened", new RemoteFileError("open", "none"), "Could not open the downloaded file"],
])("raises an error alert for %s, titled by the step that failed, and is idle again", async (_name, failure, title) => {
  const d = daemon(new Uint8Array(10));
  const { files } = shell();
  (failure.kind === "open" ? files.open : files.begin).mockRejectedValueOnce(failure);
  show(projectFile(BIN), { remoteFiles: files, request: d.handler as never });
  act(() => standInButton("Download and open")!.click());
  await settle();
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual([title]);
  expect(progress()).toBeNull();
  expect(standInButton("Download and open")).toBeDefined();
});

it("raises an error alert for a file that changed while it was copied, and removes the copy", async () => {
  const { files } = shell();
  const request = vi.fn(async (body: { offset?: number }) => {
    if ((body.offset ?? 0) === 0) return { type: "file_range", id: null, offset: 0, size: 10, version: "v1", data: btoa("abcd") };
    throw new CodedError("changed", "source_changed", { path: "assets/a.dat" });
  });
  show(projectFile(BIN), { remoteFiles: files, request: request as never });
  act(() => standInButton("Download and open")!.click());
  await settle();
  const [alert] = errorAlerts.getSnapshot();
  expect(alert.title).toBe("Could not download the file");
  expect(alert.message).toBe("This file changed while it was being read.");
  expect(files.discard).toHaveBeenCalledExactlyOnceWith("copy");
  expect(files.open).not.toHaveBeenCalled();
});

// ---- The main window and a browser: the host's own default application

it.each([
  ["a project file", projectFile(BIN), { type: "open_project_path", project: "p", path: "assets/a.dat" }],
  ["a file a path link names", linkedFile(BIN), { type: "open_path_link", project: "p", path: "/tmp/out/a.dat" }],
])("offers %s that the viewer cannot show Open in default application under it, which does what the header's button does", async (_name, subject, sent) => {
  const request = vi.fn(async () => ({ type: "ack", id: null }));
  show(subject, { isLocal: true, request: request as never });
  expect(standInButtons().map((b) => b.textContent)).toEqual(["Open in default application"]);
  act(() => standInButton("Open in default application")!.click());
  await settle();
  expect(request).toHaveBeenCalledExactlyOnceWith(sent);
});

it("offers a file too large to display the same button, and the header's Default application beside File manager", async () => {
  const request = vi.fn(async () => ({ type: "ack", id: null }));
  show(projectFile(LARGE), { isLocal: true, request: request as never });
  expect(standIn().textContent).toContain("This file is too large to display");
  expect(standInButtons().map((b) => b.textContent)).toEqual(["Open in default application"]);
  expect(headerButtons()).toEqual(expect.arrayContaining(["Open in file manager", "Open in default application"]));
  expect(headerButtons()).not.toContain("Download and open");
});

it("sends a browser on the daemon's own machine, which is local, the same actions", () => {
  show(linkedFile(BIN), { isLocal: true });
  expect(headerButtons().slice(-2)).toEqual(["Open in file manager", "Open in default application"]);
});

// ---- The header of a file only a path link names

it("sends File manager as reveal_path_link and Default application as open_path_link for it, and Default application is left out for a file the system could launch", () => {
  const request = vi.fn(async (_body: unknown) => ({ type: "ack", id: null }));
  show(linkedFile(undefined), { isLocal: true, request: request as never });
  act(() => headerButton("Open in file manager").click());
  act(() => headerButton("Open in default application").click());
  expect(request.mock.calls.map(([body]) => body)).toEqual([
    { type: "reveal_path_link", project: "p", path: "/tmp/out/a.dat" },
    { type: "open_path_link", project: "p", path: "/tmp/out/a.dat" },
  ]);
  show(linkedFile(undefined, { launchable: true }), { isLocal: true, request: request as never });
  expect(headerButtons().slice(-1)).toEqual(["Open in file manager"]);
  expect(headerButtons()).not.toContain("Open in default application");
  show(linkedFile(undefined, {}, "/tmp/out/run.sh"), { isLocal: true, request: request as never });
  expect(headerButtons()).not.toContain("Open in default application");
});

it("leaves Default application out of the header of a project file the system could launch, by the daemon's word or by name, keeping File manager", () => {
  show(projectFile(undefined, { launchable: true }, "bin/tool"), { isLocal: true });
  expect(headerButtons().slice(-1)).toEqual(["Open in file manager"]);
  show(projectFile(undefined, {}, "scripts/run.sh"), { isLocal: true });
  expect(headerButtons().slice(-1)).toEqual(["Open in file manager"]);
  show(projectFile(undefined, {}, "scripts/notes.txt"), { isLocal: true });
  expect(headerButtons().slice(-2)).toEqual(["Open in file manager", "Open in default application"]);
});

it.each([
  ["a Unix host, home shortened", { homeDir: "/home/u", host: { path_style: "unix" } }, "/home/u/out/a.txt", "~/out/a.txt"],
  ["a Unix host, outside home", { homeDir: "/home/u", host: { path_style: "unix" } }, "/tmp/out/a.txt", "/tmp/out/a.txt"],
  ["a Windows host", { homeDir: "C:\\Users\\u", host: { path_style: "windows" } }, "C:\\Users\\u\\out\\a.txt", "C:\\Users\\u\\out\\a.txt"],
])("names a linked file by its last name and shows and copies its path as the host spells it (%s)", async (_name, state, path, shown) => {
  const writeText = vi.fn(async () => {});
  show(linkedFile({ kind: "text", text: "x", size: 1 }, {}, path), { isLocal: true, state, clipboard: { writeText } });
  expect(dialog().querySelector("h2")?.textContent).toContain("a.txt");
  expect(dialog().querySelector("[data-viewer-file-summary]")!.textContent).toContain(shown);
  await act(async () => dialog().querySelector<HTMLButtonElement>('button[aria-label="Copy path"]')!.click());
  expect(writeText).toHaveBeenCalledExactlyOnceWith(shown);
});

// ---- What launches depends on the system that opens the file

const script = (over: Partial<ViewerSubject> = {}) => projectFile({ kind: "text", text: "x", size: 1 }, over, "src/app.js");

it("offers Default application for a .js file on a Unix daemon host, which opens it in an editor, and not on a Windows one, which runs it", () => {
  show(script(), { isLocal: true });
  expect(headerButtons()).toContain("Open in default application");
  show(script(), { isLocal: true, state: { host: { path_style: "windows" } } });
  expect(headerButtons()).not.toContain("Open in default application");
  expect(headerButtons()).toContain("Open in file manager");
});

it("refuses to download a .js file to a Windows desktop, which would run the copy, and offers it for a macOS one", () => {
  const { files } = shell();
  show(script(), { remoteFiles: files, desktop: "windows" });
  expect(headerButtons()).not.toContain("Download and open");
  show(script(), { remoteFiles: files, desktop: "macos" });
  expect(headerButtons()).toContain("Download and open");
  // A name every system launches is refused on both.
  show(projectFile({ kind: "text", text: "x", size: 1 }, {}, "bin/run.sh"), { remoteFiles: files, desktop: "macos" });
  expect(headerButtons()).not.toContain("Download and open");
});
