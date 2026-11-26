// @vitest-environment jsdom
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import { press } from "../components/alerts/detailsTestSupport";
import { sessionOf, standingNoticeOf } from "../fixtures/builders";
import { isolate } from "../i18n/isolate";
import { dismissStandingNotice, useDaemonStore } from "../store";
import { standingNoticeKey } from "../store/standingNotices";
import { fakeTimers } from "../test/clock";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { StandingNoticeBanner, useShownStandingNotices } from "./StandingNoticeBanner";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const shellParams = { shell: "/bin/zsh", command: "env -0", timeout: "10s" };
// A launch that fell back to a plain terminal because its additions could not be prepared: the failure it nests is its
// reason, and carries its own remedy and details.
const plain = standingNoticeOf("launch_enhancements_unavailable", { agent: "Codex", reason_code: "shell_environment_timeout", ...shellParams }, "Could not prepare.", 1);
// An account read as logged out whose provider has no login flow of its own: the remedy is the provider's own way in.
const signedOut = standingNoticeOf("account_sign_in_unconfirmed", { agent: "Test Agent", agent_id: "test_agent", account: "Work", config_dir: "/accounts/work" }, "Maybe signed out.", 2);
const hooks = standingNoticeOf("hooks_not_received", { agent: "Claude Code" }, "No hooks.", 3);

function mountBanner(notices = [plain, signedOut], onDismiss = vi.fn()) {
  fakeTimers();
  const container = mountHost(<StandingNoticeBanner notices={notices} onDismiss={onDismiss} />, { daemon: testDaemon() });
  const button = (name: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find((candidate) => candidate.textContent === name || candidate.getAttribute("aria-label") === name);
  const list = () => document.querySelector<HTMLElement>("[role=dialog][aria-label='Notices about this session']");
  const entries = () => [...list()!.querySelectorAll("section")];
  return { container, button, list, entries, onDismiss };
}

describe("the banner", () => {
  it("is a warning bar of the newest notice's sentence, hidden from assistive technology, with the warning mark", () => {
    const { container } = mountBanner([plain, hooks]);
    const sentence = [...container.querySelectorAll("span[aria-hidden=true]")].find((candidate) => candidate.textContent?.includes("has not reported"))!;
    expect(sentence.textContent).toContain(`${isolate("Claude Code")} has not reported to Octoboard`);
    expect(sentence.textContent).not.toContain("Reason:");
    expect(container.querySelector("svg[class*=triangle-alert][aria-hidden=true]")).not.toBeNull();
  });

  it("offers Details for one notice and All notices for several, each opening the list", () => {
    const one = mountBanner([hooks]);
    expect(one.button("Details")).toBeDefined();
    expect(one.button("All notices")).toBeUndefined();
    press(one.button("Details")!);
    expect(one.entries()).toHaveLength(1);
  });

  it("lists the notices newest first, the earlier one with the failure it nests as its reason, its own remedy and the details of that failure", () => {
    const { button, list, entries } = mountBanner([plain, signedOut]);
    expect(button("Details")).toBeUndefined();
    press(button("All notices")!);
    expect(list()).not.toBeNull();
    const [newest, earlier] = entries();
    expect(newest!.textContent).toContain("was last read as logged out");
    // The earlier one nests the failure that made the launch plain: its reason in the sentence, its own remedy under it
    // and its details (the command that did not finish) after.
    expect(earlier!.textContent).toContain("Reason: A shell startup file is probably blocked");
    expect(earlier!.textContent).toContain("Fix or skip the slow step in the shell");
    expect(earlier!.querySelector("code")?.textContent).toBe("/bin/zsh -l -i -c 'env -0'");
  });

  // `account_sign_in_unconfirmed` says in its own sentence to log in in the terminal; the remedy its record names for
  // the dialogs that refuse on it (choose another account, or log in) is no step to take in the banner, so the list adds
  // no remedy line under the sentence.
  it("adds no remedy line under the sentence of an account read as logged out, which says what to do itself", () => {
    const { button, entries } = mountBanner([signedOut, hooks]);
    press(button("All notices")!);
    const entry = entries().find((candidate) => candidate.textContent?.includes("was last read as logged out"))!;
    expect(entry.querySelectorAll("p")).toHaveLength(1);
    expect(entry.textContent).not.toContain("Choose another account");
    expect(entry.textContent).not.toContain("To log in, run");
  });

  it("shows the daemon's own text for a code this window has no words for, with no remedy of its own", () => {
    const unknown = standingNoticeOf("provider.claude.from_the_future", {}, "A notice this window has no words for.", 4);
    const { button, entries } = mountBanner([unknown]);
    press(button("Details")!);
    expect(entries().map((entry) => entry.textContent)).toEqual(["A notice this window has no words for."]);
  });

  it("dismisses the notice the bar shows, which is the newest, and no other", () => {
    const { button, onDismiss } = mountBanner([plain, hooks]);
    press(button("Dismiss notice")!);
    expect(onDismiss).toHaveBeenCalledTimes(1);
    expect(onDismiss).toHaveBeenCalledWith(hooks);
  });
});

describe("the notices a window shows", () => {
  const session = sessionOf("s", "c", undefined, "Work", "working", { standing_notices: [plain, hooks] });
  function Probe({ id }: { id: string }) {
    const shown = useShownStandingNotices(useDaemonStore((state) => state.sessions.get(id)));
    return <output>{shown.map((notice) => notice.code).join(",")}</output>;
  }
  const daemon = testDaemon({ state: { sessions: new Map([["s", session]]) } });

  it("are those of the session's record that this window has not dismissed", () => {
    const container = mountHost(<Probe id="s" />, { daemon });
    expect(container.textContent).toBe("launch_enhancements_unavailable,hooks_not_received");
    act(() => dismissStandingNotice(daemon.store, standingNoticeKey("s", hooks)));
    expect(container.textContent).toBe("launch_enhancements_unavailable");
  });

  it("are none for a session that is gone", () => {
    expect(mountHost(<Probe id="missing" />, { daemon }).textContent).toBe("");
  });

  it("are none for an archived session, whose history covers its terminal", () => {
    const archived = testDaemon({ state: { sessions: new Map([["s", { ...session, status: "archived" as const }]]) } });
    expect(mountHost(<Probe id="s" />, { daemon: archived }).textContent).toBe("");
  });
});
