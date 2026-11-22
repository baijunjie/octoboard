import { expect, it } from "vitest";

import { uncoverTerminal, type TerminalCovers } from "./uncoverTerminal";

function covers(shown: { archiveOpen?: boolean; pageShown?: boolean; leavePage?: TerminalCovers["leavePage"] } = {}) {
  const calls: string[] = [];
  const leaveGuarded = (go: () => void) => { calls.push("leavePage"); go(); };
  const parts: TerminalCovers = {
    uncoverPanels: () => calls.push("uncoverPanels"),
    archiveOpen: false,
    closeArchive: () => calls.push("closeArchive"),
    pageShown: false,
    leavePage: leaveGuarded,
    pageLeft: () => calls.push("pageLeft"),
    ...shown,
  };
  return { parts, calls };
}

it("clears the panels and nothing else where nothing else covers the terminal", () => {
  const { parts, calls } = covers();
  uncoverTerminal(parts);
  expect(calls).toEqual(["uncoverPanels"]);
});

it("closes the archive view when it is open, after the panels", () => {
  const { parts, calls } = covers({ archiveOpen: true });
  uncoverTerminal(parts);
  expect(calls).toEqual(["uncoverPanels", "closeArchive"]);
});

it("leaves the Remote devices or Scheduled tasks page through the guarded leave, which may keep it", () => {
  const { parts, calls } = covers({ pageShown: true });
  uncoverTerminal(parts);
  expect(calls).toEqual(["uncoverPanels", "leavePage", "pageLeft"]);
});

it("does both when both cover the terminal", () => {
  const { parts, calls } = covers({ archiveOpen: true, pageShown: true });
  uncoverTerminal(parts);
  expect(calls).toEqual(["uncoverPanels", "closeArchive", "leavePage", "pageLeft"]);
});

it("hands `pageLeft` to the guarded leave to run, so it does not run where the user keeps the page", () => {
  const { parts, calls } = covers({ pageShown: true, leavePage: () => {} });
  uncoverTerminal(parts);
  expect(calls).toEqual(["uncoverPanels"]);
});
