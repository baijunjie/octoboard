// @vitest-environment jsdom
import { act, useEffect, useState } from "react";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import { fakeTimers, runOutTimers } from "../../test/clock";
import { buttons, click } from "../../test/dom";
import { mountHost } from "../../test/mountHost";
import { MODAL_CLOSE_SETTLE_MS, setFocusAfterModalLoss } from "../focus/modalFocus";
import { framesAsTimers, settleFrames, withModality } from "./dialogTestSupport";
import { useLeaveGuard, UnsavedChangesProvider, useUnsavedChanges, useUnsavedChangesGuard } from "./UnsavedChanges";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const ASKING = '[role="alertdialog"]';

interface Probe {
  /** Sets whether the form holds changes, from outside the question's modal. */
  setUnsaved: (unsaved: boolean) => void;
  setSaving: (saving: boolean) => void;
}

/** What the form mounted by the running test hands out. */
const probe: Probe = { setUnsaved: () => {}, setSaving: () => {} };

/** A form that holds changes while it says so, and a control that leaves it through the guard. */
function Form({ discard }: { discard: () => void }): React.ReactElement {
  const [unsaved, setUnsaved] = useState(true);
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    probe.setUnsaved = setUnsaved;
    probe.setSaving = setSaving;
  }, []);
  useUnsavedChanges({ unsaved, saving, discard: () => {
    discard();
    setUnsaved(false);
  } });
  return <input aria-label="Name" />;
}

function Leaver({ leave }: { leave: () => void }): React.ReactElement {
  const run = useLeaveGuard();
  return <button onClick={() => run(leave)}>Leave</button>;
}

function Host({ leave, discard }: { leave: () => void; discard: () => void }): React.ReactElement {
  const unsaved = useUnsavedChangesGuard();
  return (
    <UnsavedChangesProvider guard={unsaved.guard}>
      <Form discard={discard} />
      <Leaver leave={leave} />
      {unsaved.confirmation}
    </UnsavedChangesProvider>
  );
}

function mount(leave: () => void = () => {}, discard: () => void = () => {}): Probe {
  withModality("pointer");
  framesAsTimers();
  fakeTimers();
  mountHost(<Host leave={leave} discard={discard} />);
  return probe;
}

const leaveButton = () => buttons().find((button) => button.textContent === "Leave")!;
const answer = (label: string) => click(buttons().find((button) => button.textContent === label));
/** Presses Leave from where focus stands on it, as a user does, and lets the question open. */
async function askToLeave(): Promise<void> {
  leaveButton().focus();
  await click(leaveButton());
  runOutTimers();
}

describe("the question", () => {
  it("leaves at once when no form holds changes", async () => {
    const leave = vi.fn();
    const probe = mount(leave);
    act(() => probe.setUnsaved(false));
    await click(leaveButton());
    expect(leave).toHaveBeenCalledTimes(1);
    expect(document.querySelector(ASKING)).toBeNull();
  });

  it("opens with focus on the dialog, and an Enter pressed there answers nothing", async () => {
    const leave = vi.fn();
    const discard = vi.fn();
    mount(leave, discard);
    await askToLeave();
    const dialog = document.querySelector<HTMLElement>(ASKING)!;
    expect(dialog.textContent).toContain("The changes in this form have not been saved. They will be lost.");
    expect(document.activeElement).toBe(dialog);
    act(() => {
      dialog.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
      dialog.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true, cancelable: true }));
    });
    expect(document.querySelector(ASKING)).not.toBeNull();
    expect(leave).not.toHaveBeenCalled();
    expect(discard).not.toHaveBeenCalled();
  });

  it("is kept editing by Escape: nothing leaves, nothing is discarded, and focus returns to where it was", async () => {
    const leave = vi.fn();
    const discard = vi.fn();
    mount(leave, discard);
    await askToLeave();
    act(() => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
    await settleFrames();
    runOutTimers();
    expect(document.querySelector(ASKING)).toBeNull();
    expect(leave).not.toHaveBeenCalled();
    expect(discard).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(leaveButton());
  });

  it("is kept editing by its Keep editing button, and asks again for the next leave", async () => {
    const leave = vi.fn();
    mount(leave);
    await askToLeave();
    await answer("Keep editing");
    runOutTimers();
    expect(document.querySelector(ASKING)).toBeNull();
    expect(leave).not.toHaveBeenCalled();
    await askToLeave();
    expect(document.querySelector(ASKING)).not.toBeNull();
  });

  it("hands focus left on <body> to the window's fallback once the question has closed, after Keep editing", async () => {
    const fallback = vi.fn();
    onTestFinished(setFocusAfterModalLoss(fallback));
    mount();
    // Pressed without focus on the control, so the question has no control to return it to.
    await click(leaveButton());
    runOutTimers();
    expect(document.querySelector(ASKING)).not.toBeNull();
    await answer("Keep editing");
    expect(fallback).not.toHaveBeenCalled();
    act(() => void vi.advanceTimersByTime(MODAL_CLOSE_SETTLE_MS - 1));
    expect(fallback).not.toHaveBeenCalled();
    act(() => void vi.advanceTimersByTime(1));
    expect(document.activeElement).toBe(document.body);
    expect(fallback).toHaveBeenCalledOnce();
  });

  it("gives up the changes on Discard, with the dialog gone before the leave runs", async () => {
    const order: string[] = [];
    const discard = vi.fn(() => void order.push("discard"));
    const leave = vi.fn(() => void order.push(document.querySelector(ASKING) === null ? "leave, dialog gone" : "leave, dialog still there"));
    mount(leave, discard);
    await askToLeave();
    await answer("Discard");
    runOutTimers();
    expect(order).toEqual(["discard", "leave, dialog gone"]);
    // The form no longer holds anything, so the next leave goes through.
    await click(leaveButton());
    expect(leave).toHaveBeenCalledTimes(2);
    expect(document.querySelector(ASKING)).toBeNull();
  });

  it("does nothing for a leave pressed while a save is in flight", async () => {
    const leave = vi.fn();
    const probe = mount(leave);
    act(() => probe.setSaving(true));
    await click(leaveButton());
    expect(leave).not.toHaveBeenCalled();
    expect(document.querySelector(ASKING)).toBeNull();
  });

  it("closes without a word when the changes go while it is open", async () => {
    const leave = vi.fn();
    const discard = vi.fn();
    const probe = mount(leave, discard);
    await askToLeave();
    act(() => probe.setUnsaved(false));
    runOutTimers();
    expect(document.querySelector(ASKING)).toBeNull();
    expect(leave).not.toHaveBeenCalled();
    expect(discard).not.toHaveBeenCalled();
  });
});

it("passes a leave straight through where no guard stands", async () => {
  const leave = vi.fn();
  mountHost(<Leaver leave={leave} />);
  await click(leaveButton());
  expect(leave).toHaveBeenCalledTimes(1);
});
