// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { PlatformAdapter } from "../platform";
import type { ProcessTree } from "../protocol";
import { BROWSER_HOST } from "../test/hosts";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import { ProcessCommand, ProcessTreeDetails } from "./ProcessTreeDetails";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const tree = (extra: Partial<ProcessTree> = {}): ProcessTree => ({
  id: "1:1", command: "npm run dev", cwd: "/work/search", cwd_missing: false, kept: false, keepable: true, started_at: 0, process_count: 1, memory_bytes: 1_000_000, ports: [], ...extra,
});

async function render(element: React.ReactElement, platform?: PlatformAdapter): Promise<HTMLElement> {
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  const daemon = testDaemon();
  await act(async () => root.render(<TestHost daemon={daemon} platform={platform}>{element}</TestHost>));
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

it("starts an unknown working directory in the text column, after a 14px icon, as a known one starts after its glyph", async () => {
  const unknown = await render(<ProcessTreeDetails tree={tree({ cwd: null })} />);
  const slot = unknown.querySelector<SVGElement>("svg.lucide-folder-x[aria-hidden=true]");
  expect(slot).not.toBeNull();
  expect(slot!.nextSibling?.textContent).toBe("Working directory unknown");
  const known = await render(<ProcessTreeDetails tree={tree()} />);
  expect(known.querySelector("svg[width='14']")).not.toBeNull();
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

const webLinks = (host: HTMLElement) => [...host.querySelectorAll<HTMLAnchorElement>("a[data-external-link]")];

it("shows a line of links between the working directory and the facts, one per web port, each opened through the platform", async () => {
  const open = vi.fn(async () => {});
  const host = await render(
    <ProcessTreeDetails tree={tree({ ports: [3000, 5173, 9229], http_ports: [3000, 5173] })} links />,
    { kind: "tauri", host: BROWSER_HOST, links: { open } },
  );
  const anchors = webLinks(host);
  expect(anchors.map((anchor) => anchor.textContent)).toEqual(["localhost:3000", "localhost:5173"]);
  expect(anchors.map((anchor) => anchor.getAttribute("href"))).toEqual(["http://localhost:3000/", "http://localhost:5173/"]);

  const globe = host.querySelector("svg.lucide-globe[aria-hidden=true]");
  expect(globe).not.toBeNull();
  const line = globe!.parentElement!;
  expect(line.contains(anchors[0]) && line.contains(anchors[1])).toBe(true);
  const lines = [...host.firstElementChild!.children];
  const path = lines.find((child) => child.textContent!.includes("/work/search"))!;
  const facts = lines.find((child) => child.textContent!.includes("memory"))!;
  expect(path.compareDocumentPosition(line) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(line.compareDocumentPosition(facts) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

  await act(async () => anchors[1].click());
  expect(open).toHaveBeenCalledExactlyOnceWith("http://localhost:5173/");
});

it("still lists every port in the facts line, web ports or not", async () => {
  const host = await render(<ProcessTreeDetails tree={tree({ ports: [3000, 5173, 9229], http_ports: [5173] })} links />);
  expect(host.textContent!.replace(/[\u2066-\u2069]/g, "")).toContain("listening on ports 3000, 5173, and 9229");
});

it("shows no links line when no port serves web pages, whether the daemon left them out or none answered", async () => {
  for (const http_ports of [undefined, []]) {
    const host = await render(<ProcessTreeDetails tree={tree({ ports: [9229], http_ports })} links />);
    expect(webLinks(host)).toEqual([]);
    expect(host.querySelector("svg.lucide-globe")).toBeNull();
    expect(host.textContent!.replace(/[\u2066-\u2069]/g, "")).toContain("listening on port 9229");
  }
});

it("shows no links line unless `links` is set, even for a tree with web ports", async () => {
  const host = await render(<ProcessTreeDetails tree={tree({ ports: [3000], http_ports: [3000] })} />);
  expect(webLinks(host)).toEqual([]);
  expect(host.querySelector("svg.lucide-globe")).toBeNull();
  expect(host.textContent).not.toContain("localhost:3000");
});

it("parts the links from the path and the facts for a screen reader with a visually hidden comma", async () => {
  const hiddenCommas = (host: HTMLElement) => [...host.querySelectorAll(".sr-only")].filter((element) => element.textContent === ", ");
  const without = await render(<ProcessTreeDetails tree={tree({ ports: [3000], http_ports: [] })} links />);
  expect(hiddenCommas(without)).toHaveLength(1);
  const host = await render(<ProcessTreeDetails tree={tree({ ports: [3000], http_ports: [3000] })} links />);
  const commas = hiddenCommas(host);
  expect(commas).toHaveLength(2);
  const links = host.querySelector("svg.lucide-globe")!.parentElement!;
  expect(commas[0].compareDocumentPosition(links) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(links.compareDocumentPosition(commas[1]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});
