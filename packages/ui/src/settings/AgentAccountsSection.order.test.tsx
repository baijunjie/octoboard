// @vitest-environment jsdom
import { act } from "react";
import { describe, expect, it, onTestFinished, vi } from "vitest";
import { DaemonRequestError } from "../daemon-client";
import { request, settings, toastError } from "./agentAccountsSectionTestState";
import { chooseItem, mount, rowOf, selectTab } from "./agentAccountsSectionTestSupport";

vi.mock("../store", async (importOriginal) => (await import("./agentAccountsSectionTestState")).storeStandIn(await importOriginal()));
vi.mock("../components/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../components/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../components/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("./useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../dialogs/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));


const rows = (container: HTMLElement) => [...container.querySelectorAll("tbody tr[data-account-row]")].map((row) => row.getAttribute("data-account-row"));
async function menu(container: HTMLElement, id: string) {
  const trigger = rowOf(container, id).querySelector<HTMLButtonElement>("[data-row-actions] button")!;
  await act(async () => {
    trigger.focus();
    trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.click();
  });
  return trigger;
}
function grip(container: HTMLElement) {
  return rowOf(container, "default:claude").querySelector<HTMLSpanElement>("[data-account-grip]")!;
}
function pointer(target: EventTarget, type: string, y: number, pointerType = "mouse", pointerId = 4) {
  target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, pointerType, pointerId, button: 0, clientY: y }));
}
function geometry(container: HTMLElement) {
  [...container.querySelectorAll<HTMLElement>("tbody tr[data-account-row]")].forEach((row, index) => {
    row.getBoundingClientRect = () => ({ top: index * 50, bottom: (index + 1) * 50, height: 50, left: 0, right: 600, width: 600, x: 0, y: index * 50, toJSON() {} });
  });
}
describe("account order in Settings", () => {
  it("follows the saved order including the default account", () => {
    settings.account_order = { claude: ["claude-work", null] };
    onTestFinished(() => { settings.account_order = undefined; });
    const { container } = mount();
    expect(rows(container)).toEqual(["claude-work", "default:claude"]);
  });

  it("moves the default account from its menu and rolls back a refused write", async () => {
    let reject!: (error: Error) => void;
    const { container } = mount();
    request.mockImplementationOnce(() => new Promise((_, failed) => { reject = failed; }));
    const trigger = await menu(container, "default:claude");
    await chooseItem("Move later");
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
    expect(document.activeElement).toBe(trigger);
    expect(request).toHaveBeenCalledExactlyOnceWith({ type: "reorder_accounts", agent: "claude", accounts: ["claude-work", null] });
    expect(rows(container)).toEqual(["claude-work", "default:claude"]);
    await menu(container, "default:claude");
    const earlier = [...document.querySelectorAll('[role="menuitem"]')].find((item) => item.textContent === "Move earlier")!;
    expect(earlier.getAttribute("aria-disabled")).toBe("true");
    await act(async () => reject(new DaemonRequestError("changed", "account_order_invalid", {})));
    expect(rows(container)).toEqual(["default:claude", "claude-work"]);
    expect(toastError).toHaveBeenCalledWith(expect.anything(), undefined, "Could not reorder the accounts");
  });

  it("drags by the grip and keeps the optimistic order when the reply precedes the broadcast", async () => {
    const { container, render } = mount();
    geometry(container);
    act(() => { pointer(grip(container), "pointerdown", 25); pointer(document, "pointermove", 70); });
    expect(rowOf(container, "default:claude").style.transform).toBe("translateY(45px)");
    expect(request).not.toHaveBeenCalled();
    await act(async () => pointer(document, "pointerup", 70));
    expect(rows(container)).toEqual(["claude-work", "default:claude"]);
    expect(request).toHaveBeenCalledExactlyOnceWith({ type: "reorder_accounts", agent: "claude", accounts: ["claude-work", null] });
    settings.account_order = { claude: ["claude-work", null] };
    onTestFinished(() => { settings.account_order = undefined; });
    render();
    expect(rows(container)).toEqual(["claude-work", "default:claude"]);
    settings.account_order = undefined;
    render();
    expect(rows(container)).toEqual(["default:claude", "claude-work"]);
  });

  it("keeps an in-flight write guarded when the agent tab is reopened", async () => {
    const { container } = mount();
    let finish!: () => void;
    request.mockImplementationOnce(() => new Promise((resolve) => { finish = () => resolve({}); }));
    await menu(container, "default:claude");
    await chooseItem("Move later");
    selectTab(container, "Codex");
    selectTab(container, "Claude Code");
    expect(rows(container)).toEqual(["claude-work", "default:claude"]);
    await menu(container, "default:claude");
    const earlier = [...document.querySelectorAll('[role="menuitem"]')].find((item) => item.textContent === "Move earlier")!;
    expect(earlier.getAttribute("aria-disabled")).toBe("true");
    await chooseItem("Move earlier");
    expect(request).toHaveBeenCalledTimes(1);
    await act(async () => finish());
  });

  it("cancels a drag when its agent's table is unmounted", () => {
    const { container } = mount();
    geometry(container);
    act(() => { pointer(grip(container), "pointerdown", 25); pointer(document, "pointermove", 70); });
    selectTab(container, "Codex");
    expect(rows(container)).toEqual(["default:codex", "codex-work"]);
    act(() => pointer(document, "pointerup", 70));
    expect(request).not.toHaveBeenCalled();
  });

  it("cancels a drag as its panel becomes inert, before the exit finishes", () => {
    const { container } = mount();
    geometry(container);
    act(() => { pointer(grip(container), "pointerdown", 25); pointer(document, "pointermove", 70); });
    const panel = grip(container).closest<HTMLElement>('[role="tabpanel"]')!;
    panel.inert = true;
    panel.setAttribute("inert", "");
    act(() => pointer(document, "pointerup", 70));
    expect(request).not.toHaveBeenCalled();
    expect(rowOf(container, "default:claude").style.transform).toBe("");
  });

  it("stops a running drag transition when reduced motion is enabled", () => {
    const media = new EventTarget();
    let reduced = false;
    Object.defineProperty(media, "matches", { get: () => reduced });
    const matchMedia = window.matchMedia;
    window.matchMedia = (query) => query === "(prefers-reduced-motion: reduce)" ? media as MediaQueryList : matchMedia(query);
    onTestFinished(() => { window.matchMedia = matchMedia; });
    const { container } = mount();
    geometry(container);
    act(() => { pointer(grip(container), "pointerdown", 25); pointer(document, "pointermove", 70); });
    const sibling = rowOf(container, "claude-work");
    expect(sibling.style.transition).toBe("transform 150ms");
    act(() => {
      reduced = true;
      media.dispatchEvent(new Event("change"));
    });
    expect(sibling.style.transition).toBe("");
    act(() => pointer(document, "pointercancel", 70));
    expect(request).not.toHaveBeenCalled();
  });

  it.each(["Escape", "pointercancel", "touch", "threshold", "otherPointer"])("does not save a %s drag", (ending) => {
    const { container } = mount();
    geometry(container);
    act(() => {
      pointer(grip(container), "pointerdown", 25, ending === "touch" ? "touch" : "mouse");
      pointer(document, "pointermove", ending === "threshold" ? 27 : 70, "mouse", ending === "otherPointer" ? 9 : 4);
      if (ending === "Escape") document.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Escape" }));
      pointer(document, ending === "pointercancel" ? "pointercancel" : "pointerup", 70);
    });
    expect(rows(container)).toEqual(["default:claude", "claude-work"]);
    expect(request).not.toHaveBeenCalled();
    expect(rowOf(container, "default:claude").style.transform).toBe("");
  });
});
