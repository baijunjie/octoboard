// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { sessionOf } from "../fixtures/builders";
import type { Session } from "../protocol";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import { useChildCanReport } from "./useSessions";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(sessions: Session[]) {
  const daemon = testDaemon({ state: { sessions: new Map(sessions.map((session) => [session.id, session])) } });
  const seen = { current: undefined as boolean | undefined };
  function Subject() {
    seen.current = useChildCanReport("owner");
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<TestHost daemon={daemon}><Subject /></TestHost>));
  return { daemon, seen };
}

const owner = sessionOf("owner", "c", "p", "Owner", "idle");
const child = (status: Session["status"], extra: Partial<Session> = {}) => sessionOf("child", "c", "p", "Child", status, { bound_to: "owner", ...extra });

it("is true while a session bound to the owner is at work, and false while it is at rest", () => {
  expect(mount([owner, child("working")]).seen.current).toBe(true);
  expect(mount([owner, child("starting")]).seen.current).toBe(true);
  expect(mount([owner, child("idle")]).seen.current).toBe(false);
  expect(mount([owner, child("archived")]).seen.current).toBe(false);
});

it("is false for an owner with no session bound to it, and for one a busy session is bound to another owner of", () => {
  expect(mount([owner]).seen.current).toBe(false);
  expect(mount([owner, sessionOf("x", "c", "p", "X", "working", { bound_to: "elsewhere" })]).seen.current).toBe(false);
});

it("follows the child as it goes to work and comes to rest", () => {
  const { daemon, seen } = mount([owner, child("idle")]);
  expect(seen.current).toBe(false);

  act(() => daemon.store.setState({ sessions: new Map([["owner", owner], ["child", child("working")]]) }));
  expect(seen.current).toBe(true);

  act(() => daemon.store.setState({ sessions: new Map([["owner", owner], ["child", child("waiting_user", { at_usage_limit: true })]]) }));
  expect(seen.current).toBe(false);
});
