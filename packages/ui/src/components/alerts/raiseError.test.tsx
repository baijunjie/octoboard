// @vitest-environment jsdom
import { createRoot } from "react-dom/client";
import { act, type ReactNode } from "react";
import { afterEach, expect, it } from "vitest";

import { DaemonRequestError } from "../../daemon-client";
import { consoleOf, projectOf, sessionOf } from "../../fixtures/builders";
import { errorAlerts } from "./errorAlerts";
import { alertFailure, errorAlertOf, raiseError } from "./raiseError";
import { createStateStore, errorToast } from "../../store";

afterEach(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });

const queued = () => errorAlerts.getSnapshot()[0];

/** The text `node` renders as. */
function textOf(node: ReactNode): string {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => root.render(<>{node}</>));
    return container.textContent ?? "";
  } finally {
    act(() => root.unmount());
    container.remove();
  }
}

it("queues an alert titled for the call site where the code has no title of its own", () => {
  raiseError(new Error("boom"), { title: "Could not save" });
  expect(queued().title).toBe("Could not save");
  expect(queued().message).toBe("boom");
});

it("lets the code's own alert title win, unless the call site keeps its own", () => {
  const error = new DaemonRequestError("Trust failed.", "trust_answer_failed", {});
  raiseError(error, { title: "Could not save" });
  expect(queued().title).toBe("Could not answer the trust confirmation");
  errorAlerts.acknowledge();
  raiseError(error, { title: "Could not save", keepTitle: true });
  expect(queued().title).toBe("Could not save");
});

it("raises a catch handler's failure as the same alert", async () => {
  await Promise.reject(new Error("denied")).catch(alertFailure({ title: "Could not reorder" }));
  expect(queued().title).toBe("Could not reorder");
  expect(queued().message).toBe("denied");
});

it("words a rejection that is not an Error as the standard unexpected failure, its text under the details", () => {
  raiseError("a bare string", { title: "Could not open" });
  expect(queued().classification.kind).toBe("unexpected");
  expect(queued().message).toBe("a bare string");
  expect(textOf(queued().content)).toContain("Something unexpected went wrong.");
  expect(textOf(queued().details)).toContain("a bare string");
});

it("names the session and its project ahead of the body, from the records", () => {
  const store = createStateStore({
    consoles: new Map([["c", consoleOf("c", "Main")]]),
    projects: new Map([["p", projectOf("p", "c", "Octoboard")]]),
    sessions: new Map([["s", sessionOf("s", "c", "p", "Fix login", "idle")]]),
  });
  const request = errorToast(new Error("boom"), { title: "Could not do it", session: "s" });
  const text = textOf(errorAlertOf(request, store.getState()).content);
  expect(text).toContain("Session: ");
  expect(text).toContain("Fix login");
  expect(text).toContain("Project: ");
  expect(text).toContain("Octoboard");
});

it("names a session that is gone from the name its notice left, with no place", () => {
  const request = errorToast(new Error("boom"), { title: "Could not do it", session: "s9" });
  const gone = errorAlertOf({ ...request, daemonTitle: new DaemonRequestError("", "x", { title: "Fix login" }) }, createStateStore().getState());
  const text = textOf(gone.content);
  expect(text).toContain("Fix login");
  expect(text).not.toContain("Project: ");
  expect(errorAlertOf(request, createStateStore().getState()).session).toBe("s9");
});
