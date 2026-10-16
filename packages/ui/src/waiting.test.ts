import { expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "./gallery/fixtures/builders";
import { countWaitingInTree, waitingSessionsInTreeOrder } from "./waiting";

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
  expect(count).toBe(waitingSessionsInTreeOrder([main, other], [web], sessions).length);
  expect(count).toBe(2);
});
