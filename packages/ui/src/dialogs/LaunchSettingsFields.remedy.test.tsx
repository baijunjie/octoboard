// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { providerModelDisplay } from "../agents/providers";
import { press } from "../components/alerts/detailsTestSupport";
import { CodedError } from "../errors/codedError";
import { translatorFor } from "../i18n/catalog";
import { recoveryParams } from "../components/alerts/ErrorGuidance";
import { mountHost } from "../test/mountHost";
import { FIX_CODE, REMEDY_REFERENCES } from "../test/remedyProvider";
import { testDaemon } from "../test/testDaemon";
import { TEST_PROVIDERS } from "../test/testProviders";
import { ModelPicker, type LaunchModels } from "./LaunchSettingsFields";

vi.mock("../providers/registry", async (importOriginal) => (await import("../test/remedyProvider")).withRemedyProvider(await importOriginal()));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const APP_NAME = recoveryParams(translatorFor("en")).appName;

it("shows the step of a provider code's own remedy in the Model field's mark, once, between the sentence and the details", () => {
  const detail = "error: models unavailable\n    /tmp/مراجعة";
  const modelFailure = new CodedError("Raw sentence.", FIX_CODE, { agent: "Test Agent", path: "/tmp/test-agent.toml", note: "careful", detail });
  const launch: LaunchModels = { agent: "test_agent", display: providerModelDisplay(TEST_PROVIDERS, "test_agent"), defaultModel: null, defaultModelSource: null, loading: false, listed: false, models: [], modelText: undefined, modelFailure };
  const container = mountHost(<ModelPicker launch={launch} model={null} onChange={() => {}} />, { daemon: testDaemon() });
  const mark = container.querySelector<HTMLElement>("button[data-status-mark][aria-label='Could not load the models']");
  expect(mark).not.toBeNull();
  press(mark!);
  const popover = document.querySelector("[data-status-mark-popover]");
  expect(popover).not.toBeNull();
  const text = (popover!.textContent ?? "").replace(/[⁦-⁩]/g, "");
  const remedy = `Repair /tmp/test-agent.toml for Test Agent with ${REMEDY_REFERENCES.command}, check 9.9 and careful, then retry in ${APP_NAME}.`;
  expect(text).toContain("Test Agent could not use /tmp/test-agent.toml.");
  expect(text.split(remedy).length - 1).toBe(1);
  expect(popover!.querySelector("pre")?.textContent).toBe(detail);
  expect(text.indexOf("could not use")).toBeLessThan(text.indexOf(remedy));
  expect(text.indexOf(remedy)).toBeLessThan(text.indexOf("error: models unavailable"));
  expect(text).not.toMatch(/\{\w+\}/);
});
