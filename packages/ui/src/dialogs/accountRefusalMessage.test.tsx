// @vitest-environment jsdom
import { expect, it } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { mountHost } from "../test/mountHost";
import { TEST_PROVIDERS, UNPRESENTED_PROVIDER } from "../test/testProviders";
import { accountRefusalMessage, signedOutWarning } from "./accountRefusalMessage";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const mount = (message: React.ReactElement) => mountHost(<LanguageProvider>{message}</LanguageProvider>);
const claude = { providers: TEST_PROVIDERS, agent: "claude", dir: "/home/dev/.claude-work" };
const chipsOf = (root: ParentNode) => [...root.querySelectorAll("[data-inline-reference]")];

it("names a disabled account and the Settings that enable it again as references", () => {
  const container = mount(accountRefusalMessage("accountChoice.disabled.description", "Work"));
  expect(container.textContent).toBe("Account Work is disabled, so a session cannot use it. Choose another account, or enable it again in Settings.");
  expect(chipsOf(container).map((chip) => chip.textContent)).toEqual(["Work", "Settings"]);
});

it("names a signed-out account as a reference and points to no Settings", () => {
  const container = mount(accountRefusalMessage("accountChoice.signedOut.description", "Work", claude));
  expect(container.textContent).toBe("Account Work is not logged in, so a session cannot use it. Choose another account, or log in to it.");
  expect(chipsOf(container).map((chip) => chip.textContent)).toEqual(["Work"]);
});

it("says how to log in with the provider's own command, set apart as a reference, for a signed-out account of a provider with no login flow", () => {
  const container = mount(accountRefusalMessage("accountChoice.signedOut.description", "Work", { providers: TEST_PROVIDERS, agent: "test_agent", dir: "/home/dev/.test-work" }));
  expect(container.textContent).toBe("Account Work is not logged in, so a session cannot use it. To log in, run test-agent login in a terminal, with /home/dev/.test-work as its configuration directory.");
  expect(chipsOf(container).map((chip) => chip.textContent)).toEqual(["Work", "test-agent login", "/home/dev/.test-work"]);
});

it("keeps the advice to log in for a provider with a login flow", () => {
  const container = mount(accountRefusalMessage("accountChoice.signedOut.description", "Work", claude));
  expect(container.textContent).toBe("Account Work is not logged in, so a session cannot use it. Choose another account, or log in to it.");
});

it("words a provider without a sign-in hint of its own by a sentence that names no agent and no command", () => {
  const container = mount(accountRefusalMessage("accountChoice.signedOut.description", "Work", { providers: [UNPRESENTED_PROVIDER, ...TEST_PROVIDERS].map((provider) => provider.id === "unpresented_agent" ? { ...provider, capabilities: { ...provider.capabilities, login_flow: false } } : provider), agent: "unpresented_agent", dir: "/home/dev/.unpresented" }));
  expect(container.textContent).toBe("Account Work is not logged in, so a session cannot use it. Octoboard cannot log in this agent's accounts. In the environment where sessions run, log in with the agent's own CLI, using /home/dev/.unpresented as its configuration directory.");
  expect(chipsOf(container).map((chip) => chip.textContent)).toEqual(["Work", "Octoboard", "/home/dev/.unpresented"]);
});

it("keeps the advice to log in where the provider has no login flow but the account's directory is not known, since the hint names it", () => {
  const container = mount(accountRefusalMessage("accountChoice.signedOut.description", "Work", { providers: TEST_PROVIDERS, agent: "test_agent", dir: "" }));
  expect(container.textContent).toBe("Account Work is not logged in, so a session cannot use it. Choose another account, or log in to it.");
  expect(chipsOf(container).map((chip) => chip.textContent)).toEqual(["Work"]);
});

it("lets a name of any direction take its own, in both messages", () => {
  for (const message of [accountRefusalMessage("accountChoice.disabled.description", "حساب العمل"), accountRefusalMessage("accountChoice.signedOut.description", "حساب العمل", claude)]) {
    const container = mount(message);
    const [name] = chipsOf(container);
    expect(name!.textContent).toBe("حساب العمل");
    expect(name!.getAttribute("dir")).toBe("auto");
  }
});

it("words the warning of a signed-out account that may be chosen all the same, naming it as a reference of its own direction", () => {
  const warning = signedOutWarning("حساب العمل");
  const title = mount(<>{warning.title}</>);
  expect(title.textContent).toBe("Account حساب العمل was last read as logged out");
  const [name] = chipsOf(title);
  expect(name!.textContent).toBe("حساب العمل");
  expect(name!.getAttribute("dir")).toBe("auto");
  expect(mount(<>{warning.message}</>).textContent).toBe("The session opens all the same, and its agent may ask you to log in.");
  expect(mount(<>{warning.remedy}</>).textContent).toBe("If the agent asks, log in in the terminal, or choose another account.");
});
