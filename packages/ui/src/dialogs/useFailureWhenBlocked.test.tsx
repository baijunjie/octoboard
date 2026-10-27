// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";

import { hasCodeParams } from "../errorPresentation";
import { ActionableError } from "../actionableError";
import type { ErrorAlertView } from "../errorAlerts";
import { useFailureWhenBlocked } from "./useFailureWhenBlocked";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function run(blocked: boolean, message: string | ActionableError, title = "Cannot open"): ErrorAlertView | undefined {
  let failure: ErrorAlertView | undefined;
  function Probe() {
    failure = useFailureWhenBlocked(blocked, message, title);
    return null;
  }
  const container = document.createElement("div");
  const root = createRoot(container);
  act(() => root.render(<Probe />));
  act(() => root.unmount());
  return failure;
}

it("gives nothing while the dialog is not blocked", () => {
  expect(run(false, "Gone.")).toBeUndefined();
});

it("words a plain message as an acknowledgement-only failure", () => {
  const failure = run(true, "Gone.");
  expect(failure?.message).toBe("Gone.");
  expect(failure?.title).toBe("Cannot open");
  expect(failure?.classification).toEqual({ kind: "actionable", recovery: "none" });
});

it("keeps a structured error's code and params, so names in the explanation stay rich references", () => {
  const error = new ActionableError("Hub is gone.", "none", { code: "quick_create_owner_gone", params: { name: "Hub" } });
  const failure = run(true, error);
  expect(failure?.error).toBe(error);
  expect(hasCodeParams(failure?.error as ActionableError)).toBe(true);
});
