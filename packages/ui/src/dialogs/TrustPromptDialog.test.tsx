// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { DaemonRequestError } from "../daemon-client";
import { errorAlerts } from "../errorAlerts";
import { PlatformProvider } from "../platform/react";
import { ErrorAlerts } from "./ErrorAlerts";
import { type TrustPrompt } from "../store";
import { findButton, mountPendingQuestionDialog, pressButton } from "./pendingQuestionTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const prompt: TrustPrompt = {
  session: "s-1",
  agent: "claude",
  project: "p-1",
  path: "/Users/dev/code/shop",
  trustDir: "/Users/dev/code",
};

const ack = () => Promise.resolve({ type: "ack" });

const mount = (prompts: TrustPrompt[], reply = ack) =>
  mountPendingQuestionDialog({ trustPrompts: prompts, homeDir: "/Users/dev" }, reply);

const checkbox = (): HTMLInputElement | null => document.body.querySelector("input[type=checkbox]");

const tick = async (): Promise<void> => {
  await act(async () => checkbox()?.click());
};

const confirm = () => pressButton("Trust and continue");

// The folder is trusted only when the user ticked the box; left alone, the go-ahead covers this
// project alone, as the two buttons it replaced did.
it.each([
  { what: "the go-ahead alone", tickFirst: false },
  { what: "the go-ahead with the folder ticked", tickFirst: true },
])("$what", async ({ tickFirst }) => {
  const dialog = mount([prompt]);
  if (tickFirst) await tick();
  await confirm();
  expect(dialog.sent()).toEqual([
    { type: "confirm_trust", session: "s-1", remember: true, trust_parent_dir: tickFirst },
  ]);
});

// A project with no folder to offer gets no checkbox at all, rather than one that cannot say why
// it is unavailable.
it("no checkbox when the daemon offers no folder", () => {
  mount([{ ...prompt, trustDir: null }]);
  expect(checkbox()).toBe(null);
});

// The one dialog serves the queue in turn without remounting, so what was ticked for the session
// just answered must not carry over to the next one.
it("the next prompt in the queue starts unticked", async () => {
  const second: TrustPrompt = { ...prompt, session: "s-2", path: "/Users/dev/code/api" };
  const dialog = mount([prompt, second]);
  await tick();
  await confirm();
  await confirm();
  expect(dialog.sent()).toEqual([
    { type: "confirm_trust", session: "s-1", remember: true, trust_parent_dir: true },
    { type: "confirm_trust", session: "s-2", remember: true, trust_parent_dir: false },
  ]);
});

// Put off with "Not now", the same prompt comes back when the application reloads its state; what
// was ticked then is the user's answer to a showing that is over.
it("a prompt asked about again starts unticked", async () => {
  const dialog = mount([prompt]);
  await tick();
  await pressButton("Not now");
  await act(async () => dialog.store.setState({ trustPrompts: [prompt] }));
  expect(checkbox()?.checked).toBe(false);
});

// A folder refused as too broad, or whose project's directory could not be looked at in time,
// answered nothing, so the dialog stays for the user to untick it and go on without it.
it.each(["trust_directory_too_broad", "trust_directory_unreachable"])("a folder refused with %s keeps the dialog open", async (code) => {
  const refused = new DaemonRequestError("refused", code, { session: "s-1" });
  const dialog = mount([prompt], () => Promise.reject(refused));
  await tick();
  await confirm();
  expect(dialog.onScreen()).toBe(true);
  expect(checkbox()?.checked).toBe(true);
});

// A folder that could not be checked in time is a refusal of the trust dialog's own, whose alert
// says what the dialog offers instead: going on without the folder, or trying again later.
it("a folder that could not be checked in time is refused with the trust dialog's own remedy", async () => {
  const clear = () => act(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
  clear();
  onTestFinished(clear);
  const refused = new DaemonRequestError("refused", "trust_directory_unreachable", { path: prompt.path });
  mount([prompt], () => Promise.reject(refused));
  await tick();
  await confirm();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<PlatformProvider value={{ kind: "browser" }}><ErrorAlerts /></PlatformProvider>));
  const alert = [...document.body.querySelectorAll("[role=alertdialog]")].map((node) => node.textContent ?? "").find((text) => text.includes("did not respond in time"));
  expect(alert).toContain("Uncheck the folder option to trust this project on its own");
  expect(alert).not.toContain("Browse");
});

// A trust prompt comes up unasked, perhaps while the user is typing in a terminal: neither it nor the
// next one in the queue takes an Enter until the user moves focus onto a control, even when focus was
// left on the confirm button by the answer before.
it("keeps focus off the confirm button for each prompt in the queue", async () => {
  const second: TrustPrompt = { ...prompt, session: "s-2", path: "/Users/dev/code/api" };
  mount([prompt, second]);
  const dialog = () => document.body.querySelector("[role=alertdialog]");
  expect(document.activeElement).toBe(dialog());
  const button = findButton("Trust and continue");
  expect(button).toBeDefined();
  act(() => button?.focus());
  await confirm();
  expect(document.activeElement).toBe(dialog());
});

// What trusting lets the project's own configuration do is part of the question, said in plain text
// after it rather than as a warning-toned callout.
it("says the caution as plain body text, in order, with no alert frame", () => {
  mount([prompt]);
  const paragraphs = [...document.body.querySelectorAll("[role=alertdialog] p")].map((p) => p.textContent);
  expect(paragraphs.at(-1)).toContain("pre-approve tool permissions");
  expect(paragraphs).not.toContain("The project's own configuration takes effect.");
  expect(document.body.querySelector("[data-slot=alert-root]")).toBeNull();
  expect(document.body.textContent).not.toContain("Caution:");
});

// Trusting a folder is a risk the user opts into by ticking the box, so the warning sits under the
// option while it is ticked, and the option's note follows its state rather than repeating it.
it("warns under the folder option only while it is ticked", async () => {
  mount([prompt]);
  const alert = () => document.body.querySelector<HTMLElement>("[data-slot=alert-root]");
  const note = () => document.getElementById(checkbox()!.getAttribute("aria-describedby")!.split(" ")[0]!)!;
  expect(alert()).toBeNull();
  expect(note().textContent).toBe("Once checked, the folder is trusted instead of this project alone.");
  const box = checkbox();
  await tick();
  expect(checkbox()).toBe(box);
  expect(alert()?.textContent).toContain("Projects added there later will be trusted too.");
  expect(alert()?.textContent).toContain("Trust covers projects at any depth under the folder");
  expect(alert()?.textContent).toContain("Trust only a folder whose contents you control.");
  expect(note().textContent).toBe("The folder will be trusted instead of this project alone.");
  expect(note().compareDocumentPosition(alert()!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  await tick();
  expect(alert()).toBeNull();
});

// The Alert is no ancestor or sibling a screen reader meets by itself, so the checkbox is described by it while it
// shows, and it is spoken once after the tick, held back while the checkbox's own new state is spoken.
it("describes the folder option by its warning while ticked and speaks it once after the tick", async () => {
  vi.useFakeTimers();
  onTestFinished(() => {
    vi.useRealTimers();
  });
  mount([prompt]);
  const box = checkbox()!;
  expect(box.getAttribute("aria-describedby")!.split(" ")).toHaveLength(1);
  await tick();
  expect(checkbox()).toBe(box);
  const alert = document.body.querySelector<HTMLElement>("[data-slot=alert-root]")!.parentElement!;
  const ids = box.getAttribute("aria-describedby")!.split(" ");
  expect(ids).toHaveLength(2);
  expect(ids).toContain(alert.id);
  expect(alert.querySelector("[role=alert]")).toBeNull();
  await act(async () => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  const spoken = alert.querySelector("[role=alert]")!;
  expect(spoken.textContent).toContain("Projects added there later will be trusted too.");
  expect(spoken.textContent).toContain("at any depth");
  expect(spoken.textContent).not.toContain("Trust only a folder");
  await tick();
  expect(box.getAttribute("aria-describedby")!.split(" ")).toHaveLength(1);
});
