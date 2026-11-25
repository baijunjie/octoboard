// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { CodedError } from "../../errors/codedError";
import { globalParam, translatorFor } from "../../i18n/catalog";
import { LanguageProvider } from "../../i18n/react";
import { mountHost } from "../../test/mountHost";
import { CLASH_CODE, FIX_CODE, GLOBAL_CODE, NO_REMEDY_CODE, REMEDY_REFERENCES, TEST_BRAND_PARAMS } from "../../test/remedyProvider";
import { testDaemon } from "../../test/testDaemon";
import { ErrorGuidance, RecoveryGuidance, recoveryParams } from "./ErrorGuidance";
import { errorRemedy, FailureBody } from "./errorParts";

vi.mock("../../providers/registry", async (importOriginal) => (await import("../../test/remedyProvider")).withRemedyProvider(await importOriginal()));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const APP_NAME = recoveryParams(translatorFor("en")).appName;
const FIX_REMEDY = `${FIX_CODE}.remedy` as const;
const CLASH_REMEDY = `${CLASH_CODE}.remedy` as const;

const fixFailure = (params: Record<string, string> = {}) =>
  new CodedError("Raw sentence.", FIX_CODE, { agent: "Test Agent", path: "/tmp/test-agent.toml", note: "careful", ...params });

function mount(ui: React.ReactElement): HTMLElement {
  return mountHost(<LanguageProvider>{ui}</LanguageProvider>, { daemon: testDaemon() });
}

/** The text with the isolation marks around values left out, which only direct how a value is read. */
const plain = (element: Element): string => (element.textContent ?? "").replace(/[⁦-⁩]/g, "");

/** What each reference in `element` says, and whether it is drawn as code (monospace). */
const references = (element: Element) => [...element.querySelectorAll("[data-inline-reference]")].map((chip) => [chip.textContent, chip.querySelector("code") !== null]);

const FILLED = `Repair /tmp/test-agent.toml for Test Agent with ${REMEDY_REFERENCES.command}, check 9.9 and careful, then retry in ${APP_NAME}.`;

it("fills a provider remedy with the params of the error's code, a code param drawn as a monospace reference and the rest as text", () => {
  for (const ui of [<RecoveryGuidance recovery={FIX_REMEDY} error={fixFailure()} />, <ErrorGuidance error={fixFailure()} />]) {
    const container = mount(ui);
    expect(plain(container)).toBe(FILLED);
    // The path is a code param (code), the provider and the application are names, the module's command a literal; the
    // UI's `version` and the daemon's unlisted `note` stay plain text.
    expect(references(container)).toEqual([
      ["/tmp/test-agent.toml", true],
      ["Test Agent", false],
      [REMEDY_REFERENCES.command, true],
      [APP_NAME, false],
    ]);
  }
});

it("lets the module's own constants win over a param of the code with the same name, the agent, a reference and a shared param", () => {
  const error = new CodedError("Raw sentence.", CLASH_CODE, { agent: "Impostor", command: "impostor", appName: "Impostor", path: "/tmp/clash.toml" });
  const container = mount(<RecoveryGuidance recovery={CLASH_REMEDY} error={error} />);
  expect(plain(container)).toBe(`Run ${REMEDY_REFERENCES.command} for Test Agent in ${APP_NAME}, as /tmp/clash.toml says.`);
  expect(plain(container)).not.toContain("Impostor");
  expect(plain(container)).not.toContain("impostor");
});

it("draws a remedy with no error, or with a non-coded one, with the module's words and the record's own UI params, and none of the code's params", () => {
  const without = plain(mount(<RecoveryGuidance recovery={FIX_REMEDY} />));
  expect(without).toContain(`for Test Agent with ${REMEDY_REFERENCES.command}, check 9.9`);
  expect(without).toContain(`retry in ${APP_NAME}.`);
  expect(without).not.toContain("/tmp/test-agent.toml");
  expect(without).not.toContain("careful");
  // An error that is no coded failure names no code, so it has no params to give.
  expect(plain(mount(<RecoveryGuidance recovery={FIX_REMEDY} error={new Error("boom")} />))).toBe(without);
});

it("fills the global placeholders of a remedy itself, as references, never from a param of the failure with the same name", () => {
  const error = new CodedError("Raw sentence.", GLOBAL_CODE, { agent: "Impostor", osName: "Impostor", testAgentBrand: "Impostor" });
  const container = mount(<RecoveryGuidance recovery={`${GLOBAL_CODE}.remedy`} error={error} />);
  const osName = globalParam("osName")!;
  const brand = TEST_BRAND_PARAMS.testAgentBrand;
  expect(plain(container)).toBe(`Check ${osName} and ${brand} for Test Agent.`);
  expect(references(container).map(([text]) => text)).toEqual([osName, brand, "Test Agent"]);
});

it("leaves no placeholder in the remedy of an error that carries the code's params", () => {
  expect(plain(mount(<ErrorGuidance error={fixFailure()} />))).not.toMatch(/\{\w+\}/);
});

it("gives a provider code's remedy through errorRemedy, and none for a code of the provider with no remedy, a plain string or a core code with none", () => {
  const remedy = errorRemedy(fixFailure());
  expect(remedy).toBeDefined();
  expect(plain(mount(<p>{remedy}</p>))).toBe(FILLED);
  expect(errorRemedy(new CodedError("Raw sentence.", NO_REMEDY_CODE, { agent: "Test Agent" }))).toBeUndefined();
  expect(errorRemedy("Raw sentence.")).toBeUndefined();
  // A core code whose recovery the failure's sentence does not carry and which is no Git install gives none.
  expect(errorRemedy(new CodedError("Raw sentence.", "account_order_invalid", {}))).toBeUndefined();
});

it("keeps the Git install remedy as it was, drawn by errorRemedy and not through the provider's words", () => {
  const remedy = errorRemedy(new CodedError("Raw sentence.", "binary_not_found", { binary: "git" }));
  expect(remedy).toBeDefined();
  const text = plain(mount(<p>{remedy}</p>));
  expect(text).toContain("Install Git in the environment where sessions run");
  expect(text).not.toContain("Test Agent");
});

it("says the sentence, then the provider's remedy once, then the details in a failure mark's body", () => {
  const detail = "error: file unusable\n    /tmp/مراجعة";
  const container = mount(<FailureBody error={fixFailure({ detail })} />);
  const [sentence, remedy, ...rest] = [...container.children];
  expect(plain(sentence)).toBe("Test Agent could not use /tmp/test-agent.toml.");
  expect(sentence.tagName).toBe("P");
  expect(remedy.tagName).toBe("P");
  expect(plain(remedy)).toBe(FILLED);
  // The raw diagnostic is the details, after the remedy.
  expect(rest.map((node) => node.textContent).join("")).toContain(detail);
  expect(container.querySelector("pre")?.textContent).toBe(detail);
  const text = plain(container);
  expect(text.indexOf(plain(sentence))).toBeLessThan(text.indexOf(plain(remedy)));
  expect(text.indexOf(plain(remedy))).toBeLessThan(text.indexOf("error: file unusable"));
  expect(text.split("Repair /tmp/test-agent.toml").length - 1).toBe(1);
});

it("shows no remedy paragraph in a failure mark's body for a code with none, as before", () => {
  const container = mount(<FailureBody error={new CodedError("Raw sentence.", NO_REMEDY_CODE, { agent: "Test Agent" })} />);
  expect(plain(container)).toBe("Test Agent refused.");
  expect(container.querySelectorAll("p")).toHaveLength(1);
});
