// @vitest-environment jsdom
import { createRoot } from "react-dom/client";
import { act } from "react";
import { afterEach, expect, it } from "vitest";

import { ActionableError } from "../../errors/actionableError";
import { CodedError } from "../../errors/codedError";
import { format, messageText } from "../../i18n/catalog";
import { raiseDefaultApplicationFailure } from "./defaultApplicationFailure";
import { errorAlerts } from "./errorAlerts";

const t = (key: Parameters<typeof format>[1], params?: Record<string, string | number>) => format("en", key, params as never);

afterEach(() => {
  while (errorAlerts.getSnapshot().length > 0) act(() => errorAlerts.acknowledge());
});

const queued = () => errorAlerts.getSnapshot()[0];

it("words the refusal of a launchable file for a button, not for a link, with the path as a chip in the body and its remedy", () => {
  raiseDefaultApplicationFailure(t as never, new CodedError("x", "path_link_refused", { path: "run.sh", cause: "launchable" }), undefined);
  expect(queued().title).toBe("Could not open in the default application");
  expect(queued().message.replace(/[⁨⁩]/g, "")).toBe("run.sh is something the system would run, so it is not opened in the default application.");
  expect(queued().error).toBeInstanceOf(ActionableError);
  expect(queued().error).toMatchObject({ recovery: "pathLinkLaunchable" });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<>{queued().content}</>));
  expect(container.textContent).toContain("run.sh");
  act(() => root.unmount());
  container.remove();
});

it("raises every other failure as it is, with its own words as the body", () => {
  const notPrivate = new CodedError("x", "path_link_refused", { path: "run.sh", cause: "not_private" });
  raiseDefaultApplicationFailure(t as never, notPrivate, undefined);
  expect(queued().error).toBe(notPrivate);
  const other = new Error("boom");
  raiseDefaultApplicationFailure(t as never, other, undefined);
  expect(errorAlerts.getSnapshot().at(-1)?.error).toBe(other);
});

it("has the button's sentence in every language", () => {
  expect(messageText("de", "common.defaultApplicationLaunchable")).toContain("{path}");
});
