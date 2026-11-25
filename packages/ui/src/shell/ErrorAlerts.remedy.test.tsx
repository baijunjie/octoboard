// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { raiseError } from "../components/alerts/raiseError";
import { recoveryParams } from "../components/alerts/ErrorGuidance";
import { CodedError } from "../errors/codedError";
import { translatorFor } from "../i18n/catalog";
import { mountHost } from "../test/mountHost";
import { FIX_CODE, NO_REMEDY_CODE, REMEDY_REFERENCES } from "../test/remedyProvider";
import { testDaemon } from "../test/testDaemon";
import { ErrorAlerts } from "./ErrorAlerts";

vi.mock("../providers/registry", async (importOriginal) => (await import("../test/remedyProvider")).withRemedyProvider(await importOriginal()));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const APP_NAME = recoveryParams(translatorFor("en")).appName;
const REMEDY = `Repair /tmp/test-agent.toml for Test Agent with ${REMEDY_REFERENCES.command}, check 9.9 and careful, then retry in ${APP_NAME}.`;

const alertText = () => (document.querySelector('[role="alertdialog"]')?.textContent ?? "").replace(/[⁦-⁩]/g, "");

it("says a provider code's own remedy once in the error alert dialog, filled with the code's params", () => {
  mountHost(<ErrorAlerts />, { daemon: testDaemon() });
  const failure = new CodedError("Raw sentence.", FIX_CODE, { agent: "Test Agent", path: "/tmp/test-agent.toml", note: "careful", detail: "error: file unusable" });
  act(() => raiseError(failure, { title: "Could not launch the session" }));
  const text = alertText();
  expect(text).toContain("Test Agent could not use /tmp/test-agent.toml.");
  expect(text.split(REMEDY).length - 1).toBe(1);
  expect(text).not.toMatch(/\{\w+\}/);
});

it("says no remedy in the error alert dialog for a code of the provider that has none", () => {
  mountHost(<ErrorAlerts />, { daemon: testDaemon() });
  act(() => raiseError(new CodedError("Raw sentence.", NO_REMEDY_CODE, { agent: "Test Agent" }), { title: "Could not launch the session" }));
  expect(alertText()).toContain("Test Agent refused.");
  expect(alertText()).not.toContain("Repair");
});
