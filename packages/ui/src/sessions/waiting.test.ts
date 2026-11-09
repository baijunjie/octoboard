import { expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import { DEFAULT_SESSION_ORDER } from "./order";
import { countWaiting, countWaitingInTree, waitingSessionsInTreeOrder } from "./waiting";

it("counts the waiting sessions the tree lists, and leaves out those of a console or project that is not held", () => {
  const main = consoleOf("c-1", "Main");
  const other = consoleOf("c-2", "Other");
  const web = projectOf("p-web", main.id, "Website");
  const sessions = [
    sessionOf("hub", main.id, undefined, "Hub", "waiting_user"),
    sessionOf("web-1", main.id, web.id, "One", "waiting_user"),
    sessionOf("web-2", main.id, web.id, "Two", "idle"),
    sessionOf("orphan-project", main.id, "p-gone", "Orphan", "waiting_user"),
    sessionOf("orphan-console", "c-gone", undefined, "Orphan", "waiting_user"),
    // A project that is held, but under another console than the session's.
    sessionOf("crossed", other.id, web.id, "Crossed", "waiting_user"),
  ];
  const consoles = new Map([main, other].map((c) => [c.id, c]));
  const projects = new Map([[web.id, web]]);
  const count = countWaitingInTree(consoles, projects, new Map(sessions.map((s) => [s.id, s])));
  expect(count).toBe(waitingSessionsInTreeOrder([main, other], [web], sessions, new Set(), new Set(), DEFAULT_SESSION_ORDER).length);
  expect(count).toBe(2);
});

it("leaves out a waiting session whose process cannot be reached, which shows reconnecting instead", () => {
  const main = consoleOf("c-1", "Main");
  const web = projectOf("p-web", main.id, "Website");
  const sessions = [
    sessionOf("waiting", main.id, web.id, "Waiting", "waiting_user"),
    sessionOf("pending", main.id, web.id, "Pending", "waiting_user", { holder_link: "pending" }),
  ];
  const consoles = new Map([[main.id, main]]);
  const projects = new Map([[web.id, web]]);
  expect(countWaitingInTree(consoles, projects, new Map(sessions.map((s) => [s.id, s])))).toBe(1);
  expect(waitingSessionsInTreeOrder([main], [web], sessions, new Set(), new Set(), DEFAULT_SESSION_ORDER).map((s) => s.id)).toEqual(["waiting"]);
});

it("counts every waiting session for the app icon, leaving out one whose process cannot be reached until the link is back", () => {
  const waiting = sessionOf("waiting", "c-1", undefined, "Waiting", "waiting_user");
  const pending = sessionOf("pending", "c-gone", "p-gone", "Pending", "waiting_user", { holder_link: "pending" });
  expect(countWaiting([waiting, pending])).toBe(1);
  const { holder_link: _link, ...reached } = pending;
  expect(countWaiting([waiting, reached])).toBe(2);
});
