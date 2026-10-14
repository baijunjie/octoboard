// @vitest-environment jsdom
import { afterEach, expect, it } from "vitest";
import { installPreviewStorage } from "./storage";

const originals = Object.fromEntries(
  ["localStorage", "sessionStorage"].map((key) => [
    key,
    Object.getOwnPropertyDescriptor(window, key)!,
  ]),
);
afterEach(() => {
  for (const [key, descriptor] of Object.entries(originals))
    Object.defineProperty(window, key, descriptor);
  localStorage.clear();
});

it("isolates application preferences from the host origin and resets on every load", () => {
  const host = window.localStorage;
  host.setItem("octoboard.language", "ja");
  installPreviewStorage();
  expect(localStorage.getItem("octoboard.language")).toBeNull();
  localStorage.setItem("octoboard.language", "zh-Hans");
  sessionStorage.setItem("example", "session");
  expect(host.getItem("octoboard.language")).toBe("ja");
  expect(localStorage.getItem("example")).toBeNull();
  installPreviewStorage();
  expect(localStorage.length).toBe(0);
  expect(host.getItem("octoboard.language")).toBe("ja");
});
