// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { CodedError } from "../../errors/codedError";
import type { FieldVersions } from "../../errors/writeConflict";
import { useCheckedEdit } from "./useCheckedEdit";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Mounts a form that opened on `opened` and returns what the hook gave it, with `show` putting the store's versions. */
function mount(opened: FieldVersions, same: (path: string) => boolean = () => false, edited: (path: string) => boolean = () => true) {
  const reset = vi.fn();
  let edit!: ReturnType<typeof useCheckedEdit>;
  function Form({ current, writing }: { current: FieldVersions | undefined; writing?: boolean }) {
    edit = useCheckedEdit(opened, current, { same, edited, reset, writing });
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root: Root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const show = (current: FieldVersions | undefined, writing = false) => act(() => root.render(<Form current={current} writing={writing} />));
  return { reset, show, base: (paths: string[]) => edit.base(paths), replaced: () => edit.replaced, said: () => act(() => edit.said()), refused: (error: unknown) => act(() => edit.refused(error)) };
}

it("bases a write on the versions the form opened with", () => {
  const form = mount({ name: 1, tags: 1 });
  form.show({ name: 1, tags: 1 });
  expect(form.base(["name"])).toEqual({ name: 1 });
  expect(form.reset).not.toHaveBeenCalled();
});

it("leaves a form alone while the store has no record of it", () => {
  const form = mount({ name: 1 });
  form.show(undefined);
  expect(form.reset).not.toHaveBeenCalled();
  expect(form.base(["name"])).toEqual({ name: 1 });
});

it("resets every path the store moved past, once, and bases it on the new version, edited or not", () => {
  const form = mount({ name: 1, tags: 1, "launch.claude.model": 1 });
  form.show({ name: 1, tags: 1, "launch.claude.model": 1 });
  form.show({ name: 4, tags: 1, "launch.claude.model": 9 });
  expect(form.reset).toHaveBeenCalledExactlyOnceWith(["name", "launch.claude.model"]);
  expect(form.base(["name", "tags", "launch.claude.model"])).toEqual({ name: 4, tags: 1, "launch.claude.model": 9 });
  // A later render of the same versions resets nothing again.
  form.show({ name: 4, tags: 1, "launch.claude.model": 9 });
  expect(form.reset).toHaveBeenCalledTimes(1);
  form.show({ name: 5, tags: 1, "launch.claude.model": 9 });
  expect(form.reset).toHaveBeenLastCalledWith(["name"]);
});

it("resets after a conflict whose broadcast arrives after the reply: the write is refused, then the store moves", () => {
  const form = mount({ name: 1 });
  form.show({ name: 1 });
  // The refused write changes nothing here: the store still holds the version the form is based on.
  expect(form.base(["name"])).toEqual({ name: 1 });
  expect(form.reset).not.toHaveBeenCalled();
  form.show({ name: 8 });
  expect(form.reset).toHaveBeenCalledExactlyOnceWith(["name"]);
  expect(form.base(["name"])).toEqual({ name: 8 });
});

it("resets after a conflict whose broadcast arrived before the reply: the form was reset already, so the retry is current", () => {
  const form = mount({ name: 1 });
  form.show({ name: 1 });
  form.show({ name: 8 });
  expect(form.reset).toHaveBeenCalledExactlyOnceWith(["name"]);
  // The refusal comes now, and the form is on the current version already.
  expect(form.base(["name"])).toEqual({ name: 8 });
  expect(form.reset).toHaveBeenCalledTimes(1);
});

it("re-bases a path whose version moved over the value the form read, without resetting it", () => {
  const form = mount({ name: 1, tags: 1 }, (path) => path === "name");
  form.show({ name: 1, tags: 1 });
  form.show({ name: 7, tags: 9 });
  expect(form.reset).toHaveBeenCalledExactlyOnceWith(["tags"]);
  expect(form.base(["name", "tags"])).toEqual({ name: 7, tags: 9 });
  expect(form.replaced()).toEqual({ count: 1, paths: ["tags"] });
});

it("counts nothing when every moved path holds the value the form read", () => {
  const form = mount({ name: 1 }, () => true);
  form.show({ name: 1 });
  form.show({ name: 7 });
  expect(form.reset).not.toHaveBeenCalled();
  expect(form.base(["name"])).toEqual({ name: 7 });
  expect(form.replaced().count).toBe(0);
});

it("announces only the fields the user had edited, and none while the form's own write is out", () => {
  const form = mount({ name: 1, tags: 1, icon: 1 }, () => false, (path) => path !== "tags");
  form.show({ name: 1, tags: 1, icon: 1 });
  form.show({ name: 2, tags: 2, icon: 1 });
  expect(form.reset).toHaveBeenCalledExactlyOnceWith(["name", "tags"]);
  expect(form.replaced()).toEqual({ count: 1, paths: ["name"] });
  form.show({ name: 2, tags: 2, icon: 2 }, true);
  expect(form.replaced().count).toBe(1);
});

it("does not announce a reset that follows a refused write, and announces the next change", () => {
  const form = mount({ name: 1 });
  form.show({ name: 1 });
  form.refused(new CodedError("changed elsewhere", "write_conflict", { fields: "name", versions: '{"name":8}' }));
  form.show({ name: 8 });
  expect(form.reset).toHaveBeenCalledExactlyOnceWith(["name"]);
  expect(form.replaced().count).toBe(0);
  form.show({ name: 9 });
  expect(form.replaced()).toEqual({ count: 1, paths: ["name"] });
});

it("gathers the fields replaced while the announcer has not written, and starts afresh once it has", () => {
  const form = mount({ name: 1, tags: 1, icon: 1 });
  form.show({ name: 1, tags: 1, icon: 1 });
  form.show({ name: 2, tags: 1, icon: 1 });
  form.show({ name: 2, tags: 2, icon: 1 });
  expect(form.replaced()).toEqual({ count: 2, paths: ["name", "tags"] });
  form.said();
  form.show({ name: 2, tags: 2, icon: 2 });
  expect(form.replaced()).toEqual({ count: 3, paths: ["icon"] });
});
