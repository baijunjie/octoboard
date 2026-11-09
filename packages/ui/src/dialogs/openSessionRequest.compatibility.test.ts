import { expect, it } from "vitest";

import { consoleOf, projectOf } from "../fixtures/builders";
import { translatorFor } from "../i18n/catalog";
import { TEST_AGENTS } from "../test/testProviders";
import { openSessionRequest } from "./openSessionRequest";

it.each(TEST_AGENTS)("carries %s's explicit compatibility into new and external-record sessions without resolving follow", (agent) => {
  const owner = consoleOf("c", "Console"); const project = projectOf("p", "c", "Project");
  const compatibility = { on: true, off: false, follow: null };
  for (const resumeId of [undefined, "external-record"]) {
    const request = openSessionRequest(translatorFor("en"), { console: owner, project, choice: { agent, account: null }, resumeId, appearance: "dark", overrides: { compatibility } });
    expect(request).toMatchObject({ type: "open_session", agent, compatibility });
    if (resumeId) expect(request.resume_agent_session_id).toBe(resumeId);
  }
  expect(openSessionRequest(translatorFor("en"), { console: owner, choice: { agent, account: null }, appearance: "dark" })).not.toHaveProperty("compatibility");
});
