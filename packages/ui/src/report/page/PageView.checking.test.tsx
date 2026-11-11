// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import type { PageContent } from "../../protocol";
import { click, type } from "../../test/dom";
import type { PatternResult } from "./patternCheck";
import { renderPage } from "./pageTestSupport";

vi.mock("../../preferences/theme", () => ({ useOctoboardTheme: () => ({ resolved: "light" }) }));

// Each test's pattern check waits here until the test settles it.
const checks: Array<(result: PatternResult) => void> = [];
vi.mock("./patternCheck", () => ({
  checkPattern: () => new Promise<PatternResult>((resolve) => checks.push(resolve)),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const PAGE: PageContent = { body: [{ type: "text", name: "e", label: "E", pattern: "\\d+" }, { type: "submit" }] };

const button = (host: ParentNode, name: string) => [...host.querySelectorAll("button")].find((candidate) => candidate.textContent === name)!;
const field = (host: ParentNode) => host.querySelector<HTMLInputElement>("input")!;
const settle = (result: PatternResult) => act(async () => checks.shift()!(result));

async function pressed() {
  checks.length = 0;
  const page = await renderPage(PAGE);
  type(field(page.host), "12");
  await click(button(page.host, "Submit"));
  return page;
}

it("locks the fields and shows the pressed button pending while the pattern checks run, then submits what they held", async () => {
  const { host, onSubmit } = await pressed();
  expect(field(host).readOnly).toBe(true);
  expect(button(host, "Submit").getAttribute("aria-busy")).toBe("true");
  await click(button(host, "Submit"));
  expect(checks).toHaveLength(1);
  await settle("match");
  expect(onSubmit).toHaveBeenCalledTimes(1);
  expect(onSubmit).toHaveBeenCalledWith({ e: "12" });
  expect(field(host).readOnly).toBe(false);
});

it("unlocks a form whose value did not match, with the value kept and the error shown", async () => {
  const { host, onSubmit } = await pressed();
  await settle("mismatch");
  expect(onSubmit).not.toHaveBeenCalled();
  expect(field(host).readOnly).toBe(false);
  expect(field(host).value).toBe("12");
  expect(host.querySelector("[role=alert]")?.textContent).toBe("The value does not match the format this field asks for.");
});

it("submits nothing when the page was locked while the checks ran", async () => {
  const { host, onSubmit, rerender } = await pressed();
  await rerender({ pending: true });
  await settle("match");
  expect(onSubmit).not.toHaveBeenCalled();
  expect(field(host).readOnly).toBe(true);
});

it("submits nothing when the view is gone before the checks settle", async () => {
  const { host, onSubmit } = await pressed();
  await act(async () => host.render(<></>));
  await settle("match");
  expect(onSubmit).not.toHaveBeenCalled();
});

it("unlocks a form whose value could not be checked in time, with nothing sent and the error saying so apart from a mismatch", async () => {
  const { host, onSubmit } = await pressed();
  await settle("unchecked");
  expect(onSubmit).not.toHaveBeenCalled();
  expect(field(host).readOnly).toBe(false);
  expect(field(host).value).toBe("12");
  const message = host.querySelector("[role=alert]")?.textContent;
  expect(message).toBeTruthy();
  expect(message).not.toBe("The value does not match the format this field asks for.");
  expect(message).toContain("could not be checked");
});

it("can be submitted again once the value is checked and matches, after one that could not be checked", async () => {
  const { host, onSubmit } = await pressed();
  await settle("unchecked");
  await click(button(host, "Submit"));
  await settle("match");
  expect(onSubmit).toHaveBeenCalledExactlyOnceWith({ e: "12" });
});

it("tests every pattern field of the form before it submits, and submits nothing while one of them did not match", async () => {
  checks.length = 0;
  const { host, onSubmit } = await renderPage({
    body: [
      { type: "text", name: "a", label: "A", pattern: "\\d+" },
      { type: "text", name: "b", label: "B", pattern: "[a-z]+" },
      { type: "submit" },
    ],
  });
  const [a, b] = host.querySelectorAll<HTMLInputElement>("input");
  type(a, "1");
  type(b, "x");
  await click(button(host, "Submit"));
  expect(checks).toHaveLength(2);
  await settle("match");
  await settle("mismatch");
  expect(onSubmit).not.toHaveBeenCalled();
  expect(host.querySelectorAll("[role=alert]")).toHaveLength(1);
});

it("tests no pattern of an empty optional field or of a hidden field", async () => {
  checks.length = 0;
  const { host } = await renderPage({
    body: [
      { type: "toggle", name: "more", label: "More" },
      { type: "text", name: "empty", label: "Empty", pattern: "\\d+" },
      { type: "text", name: "hidden", label: "Hidden", pattern: "\\d+", default: "1", visible_when: { field: "more", equals: true } },
      { type: "submit" },
    ],
  });
  await click(button(host, "Submit"));
  expect(checks).toHaveLength(0);
});
