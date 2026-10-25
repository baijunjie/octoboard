// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { LanguageProvider } from "../i18n/react";
import { ThirdPartyLicensesDialog, ThirdPartyLicensesList } from "./ThirdPartyLicensesDialog";
import { filterPackages, loadThirdPartyLicenses, type ThirdPartyLicenses } from "./thirdPartyLicenses";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const licenses: ThirdPartyLicenses = {
  texts: ["MIT License text", "Apache License text"],
  packages: [
    { name: "alpha", version: "1.0.0", ecosystem: "npm", license: "MIT", texts: [0] },
    { name: "beta", version: "2.1.0", ecosystem: "cargo", license: "MIT OR Apache-2.0", texts: [0, 1] },
    { name: "gamma", version: "0.3.0", ecosystem: "cargo", license: "See license file", texts: [] },
  ],
};

function mount(data: ThirdPartyLicenses) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<LanguageProvider><ThirdPartyLicensesList licenses={data} /></LanguageProvider>));
  return container;
}

const summaries = (container: HTMLElement) => Array.from(container.querySelectorAll("summary")).map((summary) => summary.textContent);

it("filters packages by name or license, ignoring case and the space around the query", () => {
  expect(filterPackages(licenses.packages, "  ").map((entry) => entry.name)).toEqual(["alpha", "beta", "gamma"]);
  expect(filterPackages(licenses.packages, " BETA ").map((entry) => entry.name)).toEqual(["beta"]);
  expect(filterPackages(licenses.packages, "apache").map((entry) => entry.name)).toEqual(["beta"]);
  expect(filterPackages(licenses.packages, "zzz")).toEqual([]);
});

it("lists every package with its version and license, and narrows the list as the user types", async () => {
  const container = mount(licenses);
  expect(summaries(container)).toHaveLength(3);
  expect(container.textContent).toContain("3 packages");
  const input = container.querySelector("input")!;
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    setter.call(input, "gam");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  expect(summaries(container)).toHaveLength(1);
  expect(container.textContent).toContain("1 package");
});

it("shows a package's license texts only once it is expanded, and says when it ships none", () => {
  const container = mount(licenses);
  expect(container.textContent).not.toContain("Apache License text");
  const [, beta, gamma] = Array.from(container.querySelectorAll("details"));
  act(() => { beta.open = true; beta.dispatchEvent(new Event("toggle")); });
  expect(container.textContent).toContain("MIT License text");
  expect(container.textContent).toContain("Apache License text");
  act(() => { gamma.open = true; gamma.dispatchEvent(new Event("toggle")); });
  expect(container.textContent).toContain("does not ship a license text");
});

it("says so when the build carries no list", () => {
  const container = mount({ texts: [], packages: [] });
  expect(container.querySelector("h2")?.textContent).toBe("No license list in this build");
  expect(container.querySelector("input")).toBeNull();
});

it("says so when no package matches, and points aria-controls at the list only while it is there", async () => {
  const container = mount(licenses);
  const input = container.querySelector("input")!;
  const type = async (value: string) =>
    act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
  const list = container.querySelector("ul")!;
  expect(input.getAttribute("aria-controls")).toBe(list.id);
  await type("zzz");
  expect(container.querySelector("ul")).toBeNull();
  expect(input.hasAttribute("aria-controls")).toBe(false);
  expect(container.querySelector("h2")?.textContent).toBe("No matching packages");
  await act(async () => new Promise((resolve) => setTimeout(resolve, 150)));
  expect(container.querySelector("[role=status]")?.textContent).toBe("No package matches the search.");
});

it("draws a chevron on each package's summary", () => {
  const container = mount(licenses);
  expect(container.querySelectorAll("summary > svg")).toHaveLength(6);
});

it("loads a well-formed list, the committed empty one until the generator has run", async () => {
  const loaded = await loadThirdPartyLicenses();
  expect(Array.isArray(loaded.texts)).toBe(true);
  expect(Array.isArray(loaded.packages)).toBe(true);
});

async function mountDialog(load: () => Promise<ThirdPartyLicenses>) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  let mounted = true;
  // The dialog renders into `document.body`, outside the container, so a mount that is left up shows in the
  // queries of the next.
  const unmount = () => {
    if (!mounted) return;
    mounted = false;
    act(() => root.unmount());
    container.remove();
  };
  onTestFinished(unmount);
  await act(async () => root.render(<LanguageProvider><ThirdPartyLicensesDialog onClose={() => {}} load={load} /></LanguageProvider>));
  return Object.assign(() => document.body.textContent ?? "", { unmount });
}

it("shows loading, then the list or the failure, from the loader it is given", async () => {
  const pending = await mountDialog(() => new Promise(() => {}));
  expect(pending()).toContain("Loading licenses");
  pending.unmount();
  const loaded = await mountDialog(async () => licenses);
  expect(loaded()).toContain("3 packages");
  expect(loaded()).not.toContain("Loading licenses");
  loaded.unmount();
  const failed = await mountDialog(async () => { throw new Error("x"); });
  expect(failed()).toContain("Could not load the licenses");
  expect(document.querySelector("[role=alert]")).not.toBeNull();
});

it("loads again on Try again, keeping the button where it is while it runs, and shows the list once it succeeds", async () => {
  let settle: (result: ThirdPartyLicenses | Error) => void = () => {};
  const load = vi.fn<() => Promise<ThirdPartyLicenses>>()
    .mockRejectedValueOnce(new Error("disk gone"))
    .mockImplementation(() => new Promise((resolve, reject) => (settle = (result) => (result instanceof Error ? reject(result) : resolve(result)))));
  const text = await mountDialog(load);
  expect(text()).toContain("Could not load the licenses");
  const retry = () => Array.from(document.querySelectorAll("button")).find((candidate) => candidate.textContent === "Try again")!;
  await act(async () => retry().click());
  expect(load).toHaveBeenCalledTimes(2);
  expect(text()).toContain("Could not load the licenses");
  expect(retry().getAttribute("aria-disabled")).toBe("true");
  await act(async () => settle(new Error("still gone")));
  expect(retry().getAttribute("aria-disabled")).not.toBe("true");
  await act(async () => retry().click());
  await act(async () => settle(licenses));
  expect(text()).toContain("3 packages");
});

it("announces the loading line from a status region that already exists, after the dialog's focus move", async () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  await mountDialog(() => new Promise(() => {}));
  const status = () => document.querySelector("[role=status]")?.textContent;
  expect(document.querySelector("p[role=status]")).toBeNull();
  expect(status()).toBe("");
  await act(() => vi.advanceTimersByTimeAsync(AFTER_FOCUS_MOVE_DELAY_MS));
  expect(status()).toBe("Loading licenses…");
});
