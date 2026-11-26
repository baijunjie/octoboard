// @vitest-environment jsdom
// The system notification of a session that starts waiting for the user, and the app icon's badge count: raised once per
// transition into waiting, after the first snapshot only, and choosing a notification opens the session it names.
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { consoleOf, sessionOf } from "../fixtures/builders";
import { setLanguage, t } from "../i18n/language";
import type { PlatformAdapter } from "../platform";
import type { Session } from "../protocol";
import { applyDaemonEvent } from "../store";
import { withoutIsolates } from "../test/dom";
import { flushPromises } from "../test/clock";
import { BROWSER_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { useWaitingNotifications } from "./useWaitingNotifications";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const main = consoleOf("c-main", "Main");
const second = consoleOf("c-second", "Second");
const fix = sessionOf("s-fix", main.id, undefined, "Fix header", "idle");
const triage = sessionOf("s-triage", second.id, undefined, "Triage", "working");
const waiting = (session: Session): Session => ({ ...session, status: "waiting_user" });

beforeEach(() => setLanguage("en"));

function Probe({ ready, onOpen }: { ready: boolean; onOpen: (session: Session) => void }): null {
  useWaitingNotifications(ready, onOpen);
  return null;
}

type Notification = { title: string; body: string; target?: string };

function mount({ sessions = [fix, triage], ready = true, withChosen = true }: { sessions?: Session[]; ready?: boolean; withChosen?: boolean } = {}) {
  let choose!: (target: string) => void;
  const unsubscribe = vi.fn();
  const notify = vi.fn((_: Notification) => Promise.resolve());
  const setBadge = vi.fn((_: number) => Promise.resolve());
  const platform: PlatformAdapter = {
    kind: "browser",
    host: BROWSER_HOST,
    notifications: {
      ensurePermission: () => Promise.resolve(true),
      notify,
      ...(withChosen
        ? {
          onChosen: (listener: (target: string) => void) => {
            choose = listener;
            return unsubscribe;
          },
        }
        : {}),
    },
    badge: { set: setBadge },
  };
  const daemon = testDaemon({ state: { consoles: new Map([main, second].map((c) => [c.id, c])), sessions: new Map(sessions.map((s) => [s.id, s])) } });
  const onOpen = vi.fn<(session: Session) => void>();
  const host = mountHost(<Probe ready={ready} onOpen={onOpen} />, { daemon, platform });
  const settled = () => act(async () => void (await flushPromises()));
  return {
    notify,
    setBadge,
    onOpen,
    unsubscribe,
    settled,
    render: (props: { ready: boolean; onOpen: (session: Session) => void }) => host.render(<Probe {...props} />),
    unmount: () => host.render(null),
    change: async (session: Session) => {
      await act(async () => applyDaemonEvent(daemon.store, { type: "session_upserted", session }));
      await settled();
    },
    remove: async (id: string) => {
      await act(async () => applyDaemonEvent(daemon.store, { type: "session_deleted", session: id }));
      await settled();
    },
    choose: (target: string) => act(async () => choose(target)),
  };
}

it("notifies a session that starts waiting, naming it and where it is, and carrying it as the target", async () => {
  const { notify, change, settled } = mount();
  await settled();
  expect(notify).not.toHaveBeenCalled();

  await change(waiting(triage));
  expect(notify).toHaveBeenCalledTimes(1);
  const [shown] = notify.mock.calls[0]!;
  expect(shown.title).toBe(t("notification.waiting.title"));
  expect(withoutIsolates(shown.body)).toContain(triage.title);
  expect(withoutIsolates(shown.body)).toContain(second.name);
  expect(shown.target).toBe(`session:${triage.id}`);
});

it("notifies once for each transition into waiting, so a session answered and waiting again notifies again", async () => {
  const { notify, change } = mount();
  await change(waiting(fix));
  await change({ ...waiting(fix), title: "Fix header again" });
  expect(notify).toHaveBeenCalledTimes(1);

  await change(fix);
  await change(waiting(fix));
  expect(notify).toHaveBeenCalledTimes(2);
  expect(notify.mock.calls.map(([shown]) => shown.target)).toEqual([`session:${fix.id}`, `session:${fix.id}`]);
});

it("notifies nothing for sessions already waiting when the first snapshot arrives, nor before it has", async () => {
  const { notify, setBadge, render, onOpen, change, settled } = mount({ sessions: [waiting(fix), triage], ready: false });
  await settled();
  expect(notify).not.toHaveBeenCalled();
  expect(setBadge).not.toHaveBeenCalled();
  // A session starting to wait before the snapshot is no transition against anything either.
  await change(waiting(triage));
  expect(notify).not.toHaveBeenCalled();

  render({ ready: true, onOpen });
  await settled();
  expect(notify).not.toHaveBeenCalled();
  expect(setBadge).toHaveBeenLastCalledWith(2);
});

it("keeps the badge at the number of waiting sessions", async () => {
  const { setBadge, change } = mount({ sessions: [waiting(fix), triage] });
  await change(waiting(triage));
  await change(fix);
  expect(setBadge.mock.calls.map(([count]) => count)).toEqual([1, 2, 1]);
});

it("opens the session whose notification is chosen, and nothing for another target or a session gone", async () => {
  const { onOpen, choose, change, remove } = mount();
  await change(waiting(triage));
  await choose("scheduled-task:t-1");
  // Another prefix of the same length, naming a session that is held.
  await choose(`project:${triage.id}`);
  await choose("something-else");
  expect(onOpen).not.toHaveBeenCalled();

  await choose(`session:${triage.id}`);
  expect(onOpen).toHaveBeenCalledTimes(1);
  expect(onOpen).toHaveBeenCalledWith(expect.objectContaining({ id: triage.id, console_id: second.id, status: "waiting_user" }));

  await remove(triage.id);
  await choose(`session:${triage.id}`);
  await choose("session:s-never-existed");
  expect(onOpen).toHaveBeenCalledTimes(1);
});

it("calls the latest onOpen it was given", async () => {
  const { onOpen: first, render, choose } = mount();
  const latest = vi.fn<(session: Session) => void>();
  render({ ready: true, onOpen: latest });
  await choose(`session:${fix.id}`);
  expect(latest).toHaveBeenCalledWith(expect.objectContaining({ id: fix.id }));
  expect(first).not.toHaveBeenCalled();
});

it("stops listening for chosen notifications when it unmounts", () => {
  const { unsubscribe, render, unmount } = mount();
  render({ ready: true, onOpen: () => {} });
  expect(unsubscribe).not.toHaveBeenCalled();
  unmount();
  expect(unsubscribe).toHaveBeenCalledTimes(1);
});

it("still notifies where the platform cannot hear a notification chosen", async () => {
  const { notify, change } = mount({ withChosen: false });
  await change(waiting(fix));
  expect(notify).toHaveBeenCalledTimes(1);
  expect(notify.mock.calls[0]![0].target).toBe(`session:${fix.id}`);
});
