// @vitest-environment jsdom
import { act, useEffect, useState } from "react";
import { describe, expect, it, vi } from "vitest";

import { CodedError } from "../errors/codedError";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { accessibleName, framesAsTimers, settleFrames } from "../components/dialog/dialogTestSupport";
import { MIXED_RESULT, REVIEW } from "../fixtures/configurationReview";
import { registerDialogLauncher } from "../dialogs/dialogLauncher";
import type { DialogRequest } from "../dialogs/dialogRequest";
import { RequestedDialog } from "../dialogs/RequestedDialog";
import type { ClientRequestBody, Event, ImportItem } from "../protocol";
import { createImportProgressListeners, type ImportProgressEvent } from "../store";
import { ConfigurationSection } from "../settings/ConfigurationSection";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { ImportConfigurationDialog } from "./ImportConfigurationDialog";
import type { ItemProgress } from "./progressState";
import { mountDialog, press } from "./testSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const DOCUMENT = { format: "octoboard-configuration", version: 1, consoles: [{ name: "Work laptop" }] };

/** A daemon that inspects as `REVIEW` and holds the import's reply until `finish` is called, with `report` delivering a
 * progress event, as the store does, to the listener of the import the request named. */
function runningImport() {
  const progress = createImportProgressListeners();
  let finish: () => void = () => {};
  const sent: ClientRequestBody[] = [];
  const reply = (body: ClientRequestBody): Promise<Event> => {
    sent.push(body);
    if (body.type === "inspect_configuration") return Promise.resolve({ type: "configuration_inspected", ...REVIEW });
    if (body.type === "import_configuration") return new Promise((resolve) => (finish = () => resolve({ type: "configuration_imported", ...MIXED_RESULT })));
    return Promise.reject(new Error(`unexpected ${body.type}`));
  };
  let listening = 0;
  const members = {
    onImportProgress: (id: string, listener: Parameters<typeof progress.subscribe>[1]) => {
      listening += 1;
      const unsubscribe = progress.subscribe(id, listener);
      return () => {
        listening -= 1;
        unsubscribe();
      };
    },
  };
  const importId = () => (sent.find((body) => body.type === "import_configuration") as Extract<ClientRequestBody, { type: "import_configuration" }>).import_id!;
  const report = (item: { item: ImportItem } & ItemProgress) =>
    act(() => progress.deliver({ type: "configuration_import_progress", import_id: importId(), ...item } as ImportProgressEvent));
  return { reply, members, listening: () => listening, daemon: testDaemon({ request: reply, ...members }), report, finish: () => act(async () => finish()) };
}

const configurationFile = () => ({ save: vi.fn(), open: vi.fn(async () => ({ name: "mine.json", text: JSON.stringify(DOCUMENT) })) });
const text = () => document.body.textContent ?? "";
const rowOf = (name: string) => Array.from(document.body.querySelectorAll("li")).find((li) => li.textContent?.startsWith(name))!;
const spinnerIn = (element: Element) => element.querySelector("[data-slot=spinner]");
const buttonLabelled = (label: string) => Array.from(document.body.querySelectorAll("button")).find((button) => button.textContent === label);
const progressBar = () => document.body.querySelector<HTMLElement>("[role=progressbar]")!;

function mountImportDialog(running: ReturnType<typeof runningImport>) {
  return mountDialog((onClose) => <ImportConfigurationDialog onClose={onClose} />, running.reply, configurationFile(), {}, running.members);
}

async function startImport() {
  await press("Choose file");
  await press("Import");
}

describe("the progress view", () => {
  it("replaces the review with the items being imported, each with its state, as the daemon reports them", async () => {
    const running = runningImport();
    mountImportDialog(running);
    await startImport();

    // Before the first event the bar has no count.
    expect(progressBar().getAttribute("aria-valuenow")).toBeNull();
    expect(progressBar().textContent).toContain("Starting the import…");
    expect(accessibleName(progressBar())).toBe("Importing configuration");

    await running.report({ item: { kind: "console", console: 0 }, state: "waiting" });
    await running.report({ item: { kind: "project", console: 0, project: 0 }, state: "waiting" });
    await running.report({ item: { kind: "project", console: 0, project: 1 }, state: "waiting" });
    expect(progressBar().getAttribute("aria-valuetext")).toBe("0 of 3 processed");
    expect(rowOf("Website").textContent).toContain("Waiting");

    await running.report({ item: { kind: "console", console: 0 }, state: "creating" });
    await running.report({ item: { kind: "project", console: 0, project: 0 }, state: "done" });
    await running.report({ item: { kind: "project", console: 0, project: 1 }, state: "cloning" });
    expect(progressBar().getAttribute("aria-valuetext")).toBe("1 of 3 processed");
    expect(progressBar().getAttribute("aria-valuenow")).toBe("1");
    expect(progressBar().getAttribute("aria-valuemax")).toBe("3");
    expect(rowOf("Website").textContent).toContain("Done");
    expect(rowOf("Search API").textContent).toContain("Cloning…");
    // Only the items being worked on spin, and a spinner is not read out.
    expect(spinnerIn(rowOf("Website"))).toBeNull();
    expect(spinnerIn(rowOf("Search API"))?.getAttribute("aria-hidden")).toBe("true");
    // The daemon cannot stop an import: Cancel stays, disabled, beside the pending Import, and so does the close button.
    expect(buttonLabelled("Cancel")!.disabled).toBe(true);
    expect(document.body.querySelector<HTMLButtonElement>('[role=dialog] button[aria-label="Close"]')!.disabled).toBe(true);
    expect(document.body.querySelector("button[aria-busy=true]")?.textContent).toBe("Import");
  });

  it("keeps focus on the Import button that was pressed, now pending", async () => {
    const running = runningImport();
    mountImportDialog(running);
    await press("Choose file");
    const button = buttonLabelled("Import")!;
    act(() => button.focus());
    await act(async () => button.click());

    expect(text()).toContain("Importing configuration");
    expect(buttonLabelled("Import")).toBe(button);
    expect(button.getAttribute("aria-busy")).toBe("true");
    expect(document.activeElement).toBe(button);
    await running.finish();
  });

  it("disables the close button while the import runs, and enables it on the result", async () => {
    const running = runningImport();
    mountImportDialog(running);
    await startImport();
    const close = () => document.querySelector<HTMLButtonElement>('button[aria-label="Close"]')!;
    expect(close().disabled).toBe(true);
    await running.finish();
    expect(close().disabled).toBe(false);
  });

  it("shows a failed item with the reason the result gives, and moves to the result when the reply comes", async () => {
    const running = runningImport();
    mountImportDialog(running);
    await startImport();
    await running.report({ item: { kind: "console", console: 0 }, state: "waiting" });
    await running.report({ item: { kind: "project", console: 0, project: 0 }, state: "waiting" });
    await running.report({
      item: { kind: "project", console: 0, project: 0 },
      state: "failed",
      error: { code: "path_not_found", params: { path: "/gone" }, message: "no such path /gone" },
    });
    expect(rowOf("Website").textContent).toContain("Failed");
    expect(rowOf("Website").textContent).toContain("/gone");
    expect(progressBar().getAttribute("aria-valuetext")).toBe("1 of 2 processed");

    expect(running.listening()).toBe(1);
    await running.finish();
    expect(text()).toContain("Import finished");
    // A report that arrives after the reply is not heard: nothing listens for it any more.
    expect(running.listening()).toBe(0);
    await running.report({ item: { kind: "settings" }, state: "done" });
    expect(text()).toContain("Import finished");
  });

  it("goes back to the review, with the choices made, and reports the failure when the daemon refuses the request", async () => {
    const mounted = mountDialog(
      (onClose) => <ImportConfigurationDialog onClose={onClose} />,
      (body): Promise<Event> =>
        body.type === "inspect_configuration" ? Promise.resolve({ type: "configuration_inspected", ...REVIEW }) : Promise.reject(new CodedError("body did not parse", "configuration_not_recognised", {})),
      configurationFile(),
    );
    await press("Choose file");
    const website = () => rowOf("Website");
    await act(async () => website().querySelector<HTMLButtonElement>("button[aria-haspopup=listbox]")!.click());
    const option = Array.from(document.body.querySelectorAll<HTMLElement>("[role=option]")).find((o) => o.textContent === "Default clone directory")!;
    await act(async () => option.click());
    const placed = () => Array.from(website().querySelectorAll("dl > dt")).map((dt) => dt.nextElementSibling?.textContent);
    expect(placed()).toContain("/Users/dev/Projects/website");

    await press("Import");

    expect(mounted.sent().at(-1)).toMatchObject({ type: "import_configuration" });
    expect(errorAlerts.getSnapshot()).toHaveLength(1);
    expect(document.body.querySelector('[role="alertdialog"]')!.textContent).toContain("This is not an Octoboard configuration file.");
    expect(text()).toContain("Work laptop");
    expect(buttonLabelled("Import")).toBeDefined();
    expect(placed()).toContain("/Users/dev/Projects/website");
  });
});

describe("the dialog Settings opens", () => {
  /** What the window does: owns the import dialog, and Settings' section is only what asks for it. */
  function Window({ closeSettings }: { closeSettings: { current: () => void } }) {
    const [dialog, setDialog] = useState<DialogRequest>();
    const [settingsOpen, setSettingsOpen] = useState(true);
    useEffect(() => registerDialogLauncher(setDialog), []);
    closeSettings.current = () => setSettingsOpen(false);
    return (
      <>
        {settingsOpen && <ConfigurationSection />}
        {dialog && (
          <RequestedDialog
            dialog={dialog}
            onClose={() => setDialog(undefined)}
            onSessionOpened={() => {}}
            onSwitchAccounts={() => {}}
            onChangeSessionModel={() => {}}
            onResumeSession={async () => {}}
          />
        )}
      </>
    );
  }
  const platform = () => ({ kind: "browser" as const, configurationFile: configurationFile() });

  async function startImportFromChoose() {
    await press("Choose file");
    await act(async () => (Array.from(document.body.querySelectorAll("[role=dialog] button")).find((b) => b.textContent === "Import") as HTMLElement).click());
  }

  it("is not Settings' to take down: an import under way outlives the section and still reports its result", async () => {
    const running = runningImport();
    const closeSettings = { current: () => {} };
    mountHost(<Window closeSettings={closeSettings} />, { daemon: running.daemon, platform: platform() });

    await press("Import");
    await startImportFromChoose();
    await running.report({ item: { kind: "console", console: 0 }, state: "waiting" });
    await running.report({ item: { kind: "project", console: 0, project: 0 }, state: "creating" });

    act(() => closeSettings.current());
    expect(Array.from(document.body.querySelectorAll("button")).some((button) => button.textContent === "Export")).toBe(false);
    expect(progressBar().getAttribute("aria-valuetext")).toBe("0 of 2 processed");

    await running.report({ item: { kind: "project", console: 0, project: 0 }, state: "done" });
    expect(progressBar().getAttribute("aria-valuetext")).toBe("1 of 2 processed");
    await running.finish();
    expect(text()).toContain("Import finished");
  });

  it("gives focus back to Settings' Import button when it is closed", async () => {
    framesAsTimers();
    const running = runningImport();
    mountHost(<Window closeSettings={{ current: () => {} }} />, { daemon: running.daemon, platform: platform() });
    const opener = buttonLabelled("Import")!;
    // The press a pointer or key makes: it focuses the button, which `preventFocusOnPress` would not.
    act(() => opener.focus());
    await act(async () => opener.click());
    await settleFrames();
    expect(buttonLabelled("Cancel")).toBeDefined();

    await act(async () => buttonLabelled("Cancel")!.click());
    await settleFrames();

    expect(document.activeElement).toBe(opener);
  });
});
