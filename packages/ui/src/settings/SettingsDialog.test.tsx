// @vitest-environment jsdom
import { useContext, useEffect } from "react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { AddAgentOnOpenContext } from "./AddAgentOnOpenContext";
import { SettingsDialog, type SettingsSectionId } from "./SettingsDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const taken = vi.hoisted(() => ({ count: 0, dismiss: 0 }));
vi.mock("./AgentAccountsSection", () => ({
  AGENT_STRIP_HEIGHT: 45,
  AgentAccountsSection: () => {
    const request = useContext(AddAgentOnOpenContext);
    taken.dismiss = request.dismiss;
    useEffect(() => {
      if (!request.pending) return;
      taken.count += 1;
      request.consume();
    }, [request.pending]);
    return <p>accounts section</p>;
  },
}));
vi.mock("./GeneralSection", () => ({ GeneralSection: () => <p>General</p> }));
vi.mock("./GitSection", () => ({ GitSection: () => <p>Git section</p> }));
vi.mock("./TrustedFoldersSection", () => ({ TrustedFoldersSection: () => <p>Trusted</p> }));
vi.mock("./PermissionsSection", () => ({ PermissionsSection: () => <p>Permissions</p> }));
vi.mock("./NotificationsSection", () => ({ NotificationsSection: () => <p>Notifications</p> }));
vi.mock("./ConfigurationSection", () => ({ ConfigurationSection: () => <p>Configuration</p> }));

function mount(first: { nonce: number; section: SettingsSectionId | undefined; addAgent: boolean }) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const show = (navigation: typeof first) => act(() => root.render(<SettingsDialog navigation={navigation} onClose={() => {}} />));
  taken.count = 0;
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: media.startsWith("(min-width:"), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  onTestFinished(() => {
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--breakpoint-docked");
  });
  show(first);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const selected = () => document.querySelector('[role="tab"][aria-selected="true"]');
  return { show, selected };
}

it("opens on the section it is asked for and takes the Add agent request up once", async () => {
  const { selected } = mount({ nonce: 1, section: "accounts", addAgent: true });
  await act(async () => {});
  expect(selected()?.textContent).toContain("Agent accounts");
  expect(taken.count).toBe(1);
});

it("follows a later request: another section is shown and its tab takes focus", async () => {
  const { show, selected } = mount({ nonce: 1, section: "accounts", addAgent: false });
  show({ nonce: 2, section: "git", addAgent: false });
  await act(async () => { await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined))); });
  expect(selected()?.textContent).toContain("Git");
  expect(document.activeElement?.textContent).toBe(selected()?.textContent);
});

it("opens the Add agent dialog again for a later request made while the accounts section is showing", async () => {
  const { show, selected } = mount({ nonce: 1, section: "accounts", addAgent: true });
  await act(async () => {});
  expect(taken.count).toBe(1);
  show({ nonce: 2, section: "accounts", addAgent: true });
  await act(async () => {});
  expect(selected()?.textContent).toContain("Agent accounts");
  expect(taken.count).toBe(2);
  // The same navigation again, with no new request, takes nothing up.
  show({ nonce: 2, section: "accounts", addAgent: true });
  await act(async () => {});
  expect(taken.count).toBe(2);
});

it("leaves the Add agent request for the accounts section to take up when another section is asked for first", async () => {
  const { show, selected } = mount({ nonce: 1, section: "git", addAgent: false });
  show({ nonce: 2, section: "accounts", addAgent: true });
  await act(async () => {});
  expect(selected()?.textContent).toContain("Agent accounts");
  expect(taken.count).toBe(1);
});

it("asks an open Add agent dialog to close for a later request for anything else, and keeps it for one for Add agent", async () => {
  const { show } = mount({ nonce: 1, section: "accounts", addAgent: true });
  await act(async () => {});
  expect(taken.dismiss).toBe(0);
  show({ nonce: 2, section: "accounts", addAgent: true });
  await act(async () => {});
  expect(taken.dismiss).toBe(0);
  show({ nonce: 3, section: "accounts", addAgent: false });
  await act(async () => {});
  expect(taken.dismiss).toBe(1);
});

it("waits for a modal still in front, such as the Add agent dialog being dismissed, before the new tab takes focus", async () => {
  const { show, selected } = mount({ nonce: 1, section: "accounts", addAgent: false });
  await act(async () => {});
  // What a modal in front of Settings does to it: makes it inert to assistive technology.
  const settings = document.querySelector('[role="dialog"]')!;
  settings.setAttribute("aria-hidden", "true");
  onTestFinished(() => settings.removeAttribute("aria-hidden"));
  const frames = (count: number) => act(async () => {
    for (let i = 0; i < count; i++) await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
  });
  show({ nonce: 2, section: "git", addAgent: false });
  await frames(4);
  expect(selected()?.textContent).toContain("Git");
  expect(document.activeElement).not.toBe(selected());
  settings.removeAttribute("aria-hidden");
  await frames(3);
  expect(document.activeElement).toBe(selected());
});
