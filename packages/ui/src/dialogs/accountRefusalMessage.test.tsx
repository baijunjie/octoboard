// @vitest-environment jsdom
import { expect, it } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { mountHost } from "../test/mountHost";
import { accountRefusalMessage } from "./accountRefusalMessage";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const mount = (message: React.ReactElement) => mountHost(<LanguageProvider>{message}</LanguageProvider>);
const chipsOf = (root: ParentNode) => [...root.querySelectorAll("[data-inline-reference]")];

it("names a disabled account and the Settings that enable it again as references", () => {
  const container = mount(accountRefusalMessage("accountChoice.disabled.description", "Work"));
  expect(container.textContent).toBe("Account Work is disabled, so a session cannot use it. Choose another account, or enable it again in Settings.");
  expect(chipsOf(container).map((chip) => chip.textContent)).toEqual(["Work", "Settings"]);
});

it("names a signed-out account as a reference and points to no Settings", () => {
  const container = mount(accountRefusalMessage("accountChoice.signedOut.description", "Work"));
  expect(container.textContent).toBe("Account Work is not logged in, so a session cannot use it. Choose another account, or log in to it.");
  expect(chipsOf(container).map((chip) => chip.textContent)).toEqual(["Work"]);
});

it("lets a name of any direction take its own, in both messages", () => {
  for (const id of ["accountChoice.disabled.description", "accountChoice.signedOut.description"] as const) {
    const container = mount(accountRefusalMessage(id, "حساب العمل"));
    const [name] = chipsOf(container);
    expect(name!.textContent).toBe("حساب العمل");
    expect(name!.getAttribute("dir")).toBe("auto");
  }
});
