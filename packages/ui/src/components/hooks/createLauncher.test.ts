import { expect, it, vi } from "vitest";

import { createLauncher } from "./createLauncher";

it("does nothing while no window has registered", () => {
  const launcher = createLauncher<[string]>();
  expect(() => launcher.open("a")).not.toThrow();
});

it("calls the registered opener with the arguments, until it unregisters", () => {
  const launcher = createLauncher<[string, number?]>();
  const open = vi.fn();
  const unregister = launcher.register(open);
  launcher.open("a", 1);
  expect(open).toHaveBeenCalledWith("a", 1);
  unregister();
  launcher.open("b");
  expect(open).toHaveBeenCalledTimes(1);
});

it("keeps a newer registration when an older one unregisters late", () => {
  const launcher = createLauncher<[]>();
  const first = vi.fn();
  const second = vi.fn();
  const unregisterFirst = launcher.register(first);
  launcher.register(second);
  unregisterFirst();
  launcher.open();
  expect(second).toHaveBeenCalledTimes(1);
  expect(first).not.toHaveBeenCalled();
});
