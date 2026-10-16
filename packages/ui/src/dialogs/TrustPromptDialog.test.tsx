// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { DaemonRequestError } from "../daemon-client";
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

// A folder refused as too broad answered nothing, so the dialog stays for the user to untick it
// and go on without it.
it("a refused folder keeps the dialog open", async () => {
  const refused = new DaemonRequestError("too broad", "trust_directory_too_broad", { session: "s-1" });
  const dialog = mount([prompt], () => Promise.reject(refused));
  await tick();
  await confirm();
  expect(dialog.onScreen()).toBe(true);
  expect(checkbox()?.checked).toBe(true);
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
