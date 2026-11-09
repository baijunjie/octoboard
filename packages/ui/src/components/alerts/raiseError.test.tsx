// @vitest-environment jsdom
import { createRoot } from "react-dom/client";
import { act, type ReactNode } from "react";
import { expect, it } from "vitest";

import { CodedError } from "../../errors/codedError";
import { errorAlerts } from "./errorAlerts";
import { alertFailure, errorAlertOf, raiseError } from "./raiseError";
import { errorNotification } from "../../store";

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
  const error = new CodedError("Trust failed.", "trust_answer_failed", {});
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

it("keeps the session an alert is about by id, for the alert to name when it shows, with the name its notice carried", () => {
  raiseError(new Error("boom"), { title: "Could not do it", session: "s" });
  expect(queued()).toMatchObject({ session: "s", sessionName: undefined });
  expect(textOf(queued().content)).not.toContain("Session: ");
  const request = errorNotification(new Error("boom"), { title: "Could not do it", session: "s9" });
  expect(errorAlertOf({ ...request, daemonTitle: new CodedError("", "x", { title: "Fix login" }) })).toMatchObject({ session: "s9", sessionName: "Fix login" });
});
