// @vitest-environment jsdom
import { expect, it } from "vitest";

import { CodedError } from "../errors/codedError";
import { setLanguage } from "../i18n/language";
import { createInertDaemon } from ".";

it("refuses every request as a closed connection does, with the code the UI branches on and the wording of the language", async () => {
  setLanguage("en");
  const failure = await createInertDaemon().request({ type: "list_processes", project_id: "p" }).then(
    () => undefined,
    (error: unknown) => error,
  );
  expect(failure).toBeInstanceOf(CodedError);
  expect(failure).toMatchObject({ code: "connection_closed", params: {}, message: "The daemon control connection is closed." });
});

it("never leaves its first state, and has nothing to connect to", () => {
  const daemon = createInertDaemon();
  expect(daemon.store.getState().hosts).toBeUndefined();
  expect(daemon.store.getState().connectionState).toBe("connecting");
  expect(daemon.terminalUrl("s")).toBe("");
});
