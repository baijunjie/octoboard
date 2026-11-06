import { expect, it, vi } from "vitest";

import { createImportProgressListeners, type ImportProgressEvent } from "./importProgressListeners";

const event = (importId: string): ImportProgressEvent => ({
  type: "configuration_import_progress",
  import_id: importId,
  item: { kind: "settings" },
  state: "done",
});

it("delivers an event only to the listeners of the import it names", () => {
  const listeners = createImportProgressListeners();
  const mine = vi.fn();
  const other = vi.fn();
  listeners.subscribe("a", mine);
  listeners.subscribe("b", other);

  listeners.deliver(event("a"));

  expect(mine).toHaveBeenCalledExactlyOnceWith(event("a"));
  expect(other).not.toHaveBeenCalled();
});

it("delivers nothing once the listener has unsubscribed", () => {
  const listeners = createImportProgressListeners();
  const listener = vi.fn();
  const unsubscribe = listeners.subscribe("a", listener);
  unsubscribe();

  listeners.deliver(event("a"));

  expect(listener).not.toHaveBeenCalled();
});
