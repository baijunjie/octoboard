// @vitest-environment jsdom
import { act } from "react";
import ReactDOM from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

vi.mock("./fixtures", () => {
  const scenarios = [
    { id: "first", title: "First preview", group: "Processes", category: "normal", description: "The first preview's full explanation." },
    { id: "second", title: "Second preview", group: "Processes", category: "exceptional", description: "The second preview's full explanation." },
    { id: "brief", title: "Brief preview", group: "Processes", category: "normal" },
  ];
  return { DEFAULT_SCENARIO: scenarios[0], SCENARIOS: scenarios, GROUPS: [{ title: "Processes", icon: () => null }] };
});

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("collapses a preview's description by default, preserves its expansion across viewing controls, and resets it for another preview", async () => {
  const previousAddress = window.location.href;
  window.history.replaceState(null, "", "/gallery.html?scenario=first");
  const container = document.body.appendChild(document.createElement("div"));
  container.id = "root";
  const createRoot = vi.spyOn(ReactDOM, "createRoot");
  onTestFinished(() => {
    const root = createRoot.mock.results.find((result) => result.type === "return")?.value;
    if (root) act(() => root.unmount());
    createRoot.mockRestore();
    container.remove();
    window.history.replaceState(null, "", previousAddress);
  });
  await act(async () => { await import("./shell"); });
  const description = () => container.querySelector("details");
  const title = () => container.querySelector("main h1")!;
  const controls = () => container.querySelector('header[aria-label="Preview controls"]')!;
  const choose = (name: string) => act(() => [...controls().querySelectorAll("button")].find((button) => button.textContent === name)!.click());
  const selectPreview = (name: string) => act(() => [...container.querySelectorAll("nav a")].find((link) => link.textContent === name)!.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true })));

  const first = description()!;
  expect(first.open).toBe(false);
  expect(first.querySelector("summary")?.textContent).toBe("Description");
  expect(first.querySelector("p")?.textContent).toBe("The first preview's full explanation.");
  expect(title().textContent).toBe("First preview");
  expect(container.querySelector(".preview-location")?.textContent).toContain("Processes");
  expect(container.querySelector(".category-badge")?.textContent).toBe("Normal");
  expect(first.contains(title())).toBe(false);
  expect(first.contains(controls())).toBe(false);

  act(() => first.querySelector("summary")!.click());
  expect(first.open).toBe(true);
  choose("dark");
  choose("Phone 390");
  act(() => {
    const language = controls().querySelector("select")!;
    language.value = "zh-Hans";
    language.dispatchEvent(new Event("change", { bubbles: true }));
  });
  expect(description()).toBe(first);
  expect(first.open).toBe(true);
  const selected = new URLSearchParams(window.location.search);
  expect(selected.get("theme")).toBe("dark");
  expect(selected.get("width")).toBe("390");
  expect(selected.get("lang")).toBe("zh-Hans");

  selectPreview("Second preview");
  expect(description()).not.toBe(first);
  expect(description()?.open).toBe(false);
  expect(description()?.querySelector("p")?.textContent).toBe("The second preview's full explanation.");
  expect(title().textContent).toBe("Second preview");
  expect(container.querySelector(".category-badge")?.textContent).toBe("Exceptional");
  selectPreview("Brief preview");
  expect(description()).toBeNull();
  expect(title().textContent).toBe("Brief preview");
  expect(controls()).not.toBeNull();
});
