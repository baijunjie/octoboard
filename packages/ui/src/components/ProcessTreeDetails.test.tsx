// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import type { ProcessTree } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ProcessCommand, ProcessTreeDetails } from "./ProcessTreeDetails";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const tree = (extra: Partial<ProcessTree> = {}): ProcessTree => ({
  id: "1:1", command: "npm run dev", cwd: "/work/search", cwd_missing: false, kept: false, started_at: 0, process_count: 1, memory_bytes: 1_000_000, ports: [], ...extra,
});

async function render(element: React.ReactElement): Promise<HTMLElement> {
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  const daemon = { store: createStateStore({}) } as unknown as Daemon;
  await act(async () => root.render(<DaemonProvider value={daemon}>{element}</DaemonProvider>));
  onTestFinished(() => { act(() => root.unmount()); host.remove(); });
  return host;
}

it("sets a command in monospace, wrapped in full with `wrap`", async () => {
  const command = "node ".concat("--flag=value ".repeat(40)).trim();
  const host = await render(<ProcessCommand tree={tree({ command })} wrap />);
  expect(host.textContent).toBe(command);
  expect(host.querySelector(".font-mono")?.className).toContain("[overflow-wrap:anywhere]");
});

it("says a command it could not read, and keeps the glyph out of a replaced `leading`", async () => {
  const host = await render(<ProcessCommand tree={tree({ command: "" })} leading={<i data-testid="lead" />} />);
  expect(host.textContent).toBe("Unknown command");
  expect(host.querySelector("svg")).toBeNull();
  expect(host.querySelector("[data-testid=lead]")).not.toBeNull();
});

it("starts an unknown working directory in the text column, after a 16px slot, as a known one starts after its glyph", async () => {
  const unknown = await render(<ProcessTreeDetails tree={tree({ cwd: null })} />);
  const slot = unknown.querySelector<HTMLElement>("span[aria-hidden=true].size-4");
  expect(slot).not.toBeNull();
  expect(slot!.nextSibling?.textContent).toBe("Working directory unknown");
  const known = await render(<ProcessTreeDetails tree={tree()} />);
  expect(known.querySelector("svg[width='16']")).not.toBeNull();
  expect(known.textContent).toContain("/work/search");
});

it("marks a deleted working directory", async () => {
  const host = await render(<ProcessTreeDetails tree={tree({ cwd_missing: true })} />);
  expect(host.textContent).toContain("Directory deleted");
});

it("marks known and unknown commands with a decorative execution icon", async () => {
  for (const command of ["node ./node_modules/.bin/vite --host 0.0.0.0", ""]) {
    const host = await render(<ProcessCommand tree={tree({ command })} wrap />);
    expect(host.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
    expect(host.textContent).toBe(command || "Unknown command");
  }
});

it("sets a command on one line, not clamped to two", async () => {
  const host = await render(<ProcessCommand tree={tree()} />);
  expect(host.querySelector(".line-clamp-2")).toBeNull();
  expect(host.textContent).toBe("npm run dev");
});
