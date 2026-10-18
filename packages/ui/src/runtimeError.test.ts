// @vitest-environment jsdom
import { expect, it } from "vitest";

import { runtimeError } from "./runtimeError";

it("preserves Error metadata and reports thrown primitives and errors from other realms", () => {
  const error = new Error("ordinary failure");
  expect(runtimeError(new ErrorEvent("error", { error }))).toBe(error);
  expect(runtimeError(new ErrorEvent("error", { error: "thrown text" }))).toBe("thrown text");
  expect(runtimeError(new ErrorEvent("error", { error: { message: "foreign failure", stack: "foreign stack" } }))).toMatchObject({ message: "foreign failure", stack: "foreign stack" });
  expect(runtimeError(new ErrorEvent("error", { message: "fallback diagnostic" }))).toBe("fallback diagnostic");
  expect(runtimeError(new Event("error"))).toBeUndefined();
});

it("ignores the browser's ResizeObserver loop notice but not an exception that shares its text", () => {
  expect(runtimeError(new ErrorEvent("error", { message: "ResizeObserver loop completed with undelivered notifications." }))).toBeUndefined();
  expect(runtimeError(new ErrorEvent("error", { message: "ResizeObserver loop limit exceeded" }))).toBeUndefined();
  const error = new Error("ResizeObserver loop limit exceeded");
  expect(runtimeError(new ErrorEvent("error", { message: error.message, error }))).toBe(error);
});
