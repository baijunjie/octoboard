// @vitest-environment jsdom
import { act, useLayoutEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { fakeTimers } from "../test/clock";
import type { DialogRequest } from "../dialogs/dialogRequest";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { consoleOf, projectOf, snapshotState } from "../fixtures/builders";
import { DaemonProvider } from "../store";
import { testDaemon } from "../test/testDaemon";
import { CREATED_CONSOLE_WAIT_MS, useDialogRouting } from "./useDialogRouting";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const first = consoleOf("c1", "First");
const created = consoleOf("c2", "Created");

function mount() {
  fakeTimers();
  const switchConsole = vi.fn();
  const setDialogRequest = vi.fn();
  const daemon = testDaemon();
  let routing!: ReturnType<typeof useDialogRouting>;
  let consoleList = [first];
  const Probe = () => {
    routing = useDialogRouting({ dialogRequest: undefined, windowModalOpen: false, consoles: new Map(consoleList.map((c) => [c.id, c])), consoleList, sidebarOpen: false, setDialogRequest, switchConsole });
    return null;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  const render = () => act(() => root.render(<DaemonProvider value={daemon}><Probe /></DaemonProvider>));
  render();
  onTestFinished(() => {
    act(() => root.unmount());
  });
  return {
    routing: () => routing,
    switchConsole,
    appear: () => {
      consoleList = [first, created];
      render();
    },
  };
}

const newConsole: DialogRequest = { kind: "new-console" };

it("switches to the console the new-console dialog created, when its broadcast trails the close within the wait", () => {
  const hook = mount();
  act(() => hook.routing().openDialog(newConsole));
  act(() => hook.routing().closeDialog());
  act(() => void vi.advanceTimersByTime(CREATED_CONSOLE_WAIT_MS - 1));
  hook.appear();
  expect(hook.switchConsole).toHaveBeenCalledWith(created.id);
});

it("keeps looking for the created console when another dialog is opened before the wait after the close ends", () => {
  const hook = mount();
  act(() => hook.routing().openDialog(newConsole));
  act(() => hook.routing().closeDialog());
  act(() => void vi.advanceTimersByTime(CREATED_CONSOLE_WAIT_MS - 1));
  act(() => hook.routing().openDialog({ kind: "edit-console", console: first }));
  // The wait that started at the close would have ended by now, and the one the other dialog's opening restarted has not.
  act(() => void vi.advanceTimersByTime(CREATED_CONSOLE_WAIT_MS - 1));
  hook.appear();
  expect(hook.switchConsole).toHaveBeenCalledWith(created.id);
});

it("stops looking for the created console once the wait after the close has ended", () => {
  const hook = mount();
  act(() => hook.routing().openDialog(newConsole));
  act(() => hook.routing().closeDialog());
  act(() => void vi.advanceTimersByTime(CREATED_CONSOLE_WAIT_MS));
  hook.appear();
  expect(hook.switchConsole).not.toHaveBeenCalled();
});

/** The hook over the window's own state for the open dialog, as `App` holds it, with every value that state had at a
 * render. */
function mountWithState(afterCommit?: (dialogRequest: DialogRequest | undefined) => void) {
  fakeTimers();
  const daemon = testDaemon({ state: snapshotState({ projects: ["website", "api", "docs"].map((id) => projectOf(id, first.id, id)) }) });
  let routing!: ReturnType<typeof useDialogRouting>;
  let setWindowModal!: (open: boolean) => void;
  const rendered: (DialogRequest | undefined)[] = [];
  const Probe = () => {
    const [dialogRequest, setDialogRequest] = useState<DialogRequest>();
    const [windowModalOpen, setModal] = useState(false);
    setWindowModal = setModal;
    rendered.push(dialogRequest);
    routing = useDialogRouting({ dialogRequest, windowModalOpen, consoles: new Map([[first.id, first]]), consoleList: [first], sidebarOpen: false, setDialogRequest, switchConsole: vi.fn() });
    // Runs in the commit that shows `dialogRequest`: after its render and before the passive effects of that commit,
    // where the hook's release effect runs.
    useLayoutEffect(() => {
      afterCommit?.(dialogRequest);
    }, [dialogRequest]);
    return null;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  act(() => root.render(<DaemonProvider value={daemon}><Probe /></DaemonProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
  });
  return {
    daemon,
    routing: () => routing,
    setWindowModal: (open: boolean) => act(() => setWindowModal(open)),
    shown: () => rendered.at(-1),
    rendered,
    open: (dialog: DialogRequest) => act(() => routing.openDialog(dialog)),
    openWhenFree: (...dialogs: DialogRequest[]) => act(() => dialogs.forEach((dialog) => routing.stableOpenDialogWhenFree(dialog))),
    close: () => act(() => routing.closeDialog()),
  };
}

const edit: DialogRequest = { kind: "edit-console", console: first };
const pullOf = (name: string): DialogRequest => ({ kind: "pull-merge-project-branch", project: projectOf(name.toLowerCase(), first.id, name), ahead: 1, behind: 1 });
const website = pullOf("Website");
const api = pullOf("API");

it("opens a dialog that arrives on its own at once when no dialog is open", () => {
  const hook = mountWithState();
  hook.openWhenFree(website);
  expect(hook.shown()).toBe(website);
});

it("holds a dialog that arrives while another is open, leaving the open one in place until it closes", () => {
  const hook = mountWithState();
  hook.open(edit);
  hook.openWhenFree(website);
  expect(hook.shown()).toBe(edit);
  expect(hook.rendered).not.toContain(website);
  hook.close();
  expect(hook.shown()).toBe(website);
});

it("mounts the held dialog afresh, after a render that showed no dialog", () => {
  const hook = mountWithState();
  hook.open(pullOf("Docs"));
  hook.openWhenFree(website);
  hook.close();
  const at = hook.rendered.indexOf(website);
  expect(at).toBeGreaterThan(0);
  expect(hook.rendered[at - 1]).toBeUndefined();
});

it("holds a dialog that arrives in the same event as the one just opened", () => {
  const hook = mountWithState();
  act(() => {
    hook.routing().openDialog(edit);
    hook.routing().stableOpenDialogWhenFree(website);
  });
  expect(hook.shown()).toBe(edit);
  hook.close();
  expect(hook.shown()).toBe(website);
});

it("opens several held dialogs one after another in the order they arrived, and then none", () => {
  const hook = mountWithState();
  hook.open(edit);
  hook.openWhenFree(api, website);
  hook.close();
  expect(hook.shown()).toBe(api);
  hook.close();
  expect(hook.shown()).toBe(website);
  hook.close();
  expect(hook.shown()).toBeUndefined();
});

it("keeps a dialog waiting behind one that another dialog replaced, and queues a later arrival behind it", () => {
  const hook = mountWithState();
  hook.open(edit);
  hook.openWhenFree(website);
  hook.open({ kind: "new-console" });
  expect(hook.shown()).toEqual({ kind: "new-console" });
  hook.openWhenFree(api);
  hook.close();
  expect(hook.shown()).toBe(website);
  hook.close();
  expect(hook.shown()).toBe(api);
});

it("opens nothing at a close when nothing is held", () => {
  const hook = mountWithState();
  hook.open(edit);
  hook.close();
  expect(hook.shown()).toBeUndefined();
  hook.openWhenFree(api);
  expect(hook.shown()).toBe(api);
});

it("holds a dialog that arrives while a window modal is open, and opens it once that is closed", () => {
  const hook = mountWithState();
  hook.setWindowModal(true);
  hook.openWhenFree(website);
  expect(hook.shown()).toBeUndefined();
  hook.setWindowModal(false);
  expect(hook.shown()).toBe(website);
});

it("waits for the dialog and the window modal both, whichever closes last", () => {
  const hook = mountWithState();
  hook.open(edit);
  hook.openWhenFree(website);
  hook.setWindowModal(true);
  hook.close();
  expect(hook.shown()).toBeUndefined();
  hook.setWindowModal(false);
  expect(hook.shown()).toBe(website);
});

it("holds a dialog while an error alert is open", () => {
  const hook = mountWithState();
  act(() => errorAlerts.report({ message: "Could not do it", title: "Could not do it" }));
  hook.openWhenFree(website);
  expect(hook.shown()).toBeUndefined();
  act(() => errorAlerts.acknowledge());
  expect(hook.shown()).toBe(website);
});

it("drops a held confirmation whose project was removed meanwhile, and opens the next one", () => {
  const hook = mountWithState();
  hook.open(edit);
  hook.openWhenFree(website, api);
  act(() => hook.daemon.store.setState((state) => ({ projects: new Map([...state.projects].filter(([id]) => id !== "website")) })));
  hook.close();
  expect(hook.shown()).toBe(api);
  hook.close();
  expect(hook.shown()).toBeUndefined();
});

it("asks once about a project: a newer arrival replaces the held one, and one for the open confirmation is dropped", () => {
  const hook = mountWithState();
  hook.open(edit);
  const newer: DialogRequest = { ...website, ahead: 4, behind: 5 } as DialogRequest;
  hook.openWhenFree(website, api, newer);
  hook.close();
  expect(hook.shown()).toBe(newer);
  hook.openWhenFree(website);
  hook.close();
  expect(hook.shown()).toBe(api);
  hook.close();
  expect(hook.shown()).toBeUndefined();
});

it.each([
  ["the window modal first", true],
  ["the alert first", false],
])("opens a held dialog only once the window modal and an error alert are both gone, %s", (_, modalFirst) => {
  const hook = mountWithState();
  hook.setWindowModal(true);
  act(() => errorAlerts.report({ message: "Could not do it", title: "Could not do it" }));
  hook.openWhenFree(website);
  const closeModal = () => hook.setWindowModal(false);
  const closeAlert = () => act(() => errorAlerts.acknowledge());
  (modalFirst ? closeModal : closeAlert)();
  expect(hook.shown()).toBeUndefined();
  (modalFirst ? closeAlert : closeModal)();
  expect(hook.shown()).toBe(website);
});

it("drops every held confirmation whose project was removed, opening nothing, and then serves a later arrival", () => {
  const hook = mountWithState();
  hook.setWindowModal(true);
  hook.openWhenFree(website, api);
  act(() => hook.daemon.store.setState({ projects: new Map() }));
  hook.setWindowModal(false);
  expect(hook.shown()).toBeUndefined();
  act(() => hook.daemon.store.setState(snapshotState({ projects: [projectOf("api", first.id, "API")] })));
  hook.openWhenFree(api);
  expect(hook.shown()).toBe(api);
});

it("holds one confirmation per project behind a window modal, the newer arrival replacing a held one", () => {
  const hook = mountWithState();
  hook.setWindowModal(true);
  const newer: DialogRequest = { ...website, ahead: 4, behind: 5 } as DialogRequest;
  hook.openWhenFree(website, api, newer);
  hook.setWindowModal(false);
  expect(hook.shown()).toBe(newer);
  hook.close();
  expect(hook.shown()).toBe(api);
  hook.close();
  expect(hook.shown()).toBeUndefined();
});

it("asks about a project again once its confirmation has closed", () => {
  const hook = mountWithState();
  hook.openWhenFree(website);
  hook.close();
  expect(hook.shown()).toBeUndefined();
  const again: DialogRequest = { ...website, ahead: 2, behind: 2 } as DialogRequest;
  hook.openWhenFree(again);
  expect(hook.shown()).toBe(again);
});

it("ignores the close of a dialog that is no longer the open one, and closes the one that is", () => {
  const hook = mountWithState();
  hook.open(website);
  act(() => hook.routing().closeDialog(website));
  expect(hook.shown()).toBeUndefined();
  hook.open(edit);
  act(() => hook.routing().closeDialog(website));
  expect(hook.shown()).toBe(edit);
  act(() => hook.routing().closeDialog(edit));
  expect(hook.shown()).toBeUndefined();
});

it("holds a dialog asked for in the same event as an error alert was raised, before the window has rendered it", () => {
  const hook = mountWithState();
  act(() => {
    errorAlerts.report({ message: "Could not do it", title: "Could not do it" });
    hook.routing().stableOpenDialogWhenFree(website);
  });
  expect(hook.shown()).toBeUndefined();
  act(() => errorAlerts.acknowledge());
  expect(hook.shown()).toBe(website);
});

/** The hook with a `setDialogRequest` that is only a spy, so the dialog state stays undefined and every release effect
 * runs as one that comes late. `toggleModal` runs the release effect again, as a render that follows a close does. */
function mountWithLateEffects() {
  fakeTimers();
  const daemon = testDaemon({ state: snapshotState({ projects: [projectOf("website", first.id, "Website")] }) });
  const setDialogRequest = vi.fn();
  let routing!: ReturnType<typeof useDialogRouting>;
  const Probe = ({ windowModalOpen }: { windowModalOpen: boolean }) => {
    routing = useDialogRouting({ dialogRequest: undefined, windowModalOpen, consoles: new Map([[first.id, first]]), consoleList: [first], sidebarOpen: false, setDialogRequest, switchConsole: vi.fn() });
    return null;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  const render = (windowModalOpen: boolean) => act(() => root.render(<DaemonProvider value={daemon}><Probe windowModalOpen={windowModalOpen} /></DaemonProvider>));
  onTestFinished(() => act(() => root.unmount()));
  render(false);
  return {
    routing: () => routing,
    setDialogRequest,
    toggleModal: () => {
      render(true);
      render(false);
    },
  };
}

it("lets a late release effect neither forget the dialog opened in the gap nor let a held one replace it", () => {
  const hook = mountWithLateEffects();
  act(() => hook.routing().openDialog(edit));
  act(() => hook.routing().stableOpenDialogWhenFree(website));
  hook.setDialogRequest.mockClear();
  hook.toggleModal();
  expect(hook.setDialogRequest).not.toHaveBeenCalled();
  act(() => hook.routing().closeDialog(edit));
  expect(hook.setDialogRequest).toHaveBeenCalledExactlyOnceWith(undefined);
});

it("lets a late release effect leave a dialog opened after a close in place, held confirmations and all, and lets that dialog close", () => {
  const hook = mountWithLateEffects();
  const other: DialogRequest = { kind: "new-console" };
  act(() => hook.routing().openDialog(edit));
  act(() => hook.routing().stableOpenDialogWhenFree(website));
  act(() => hook.routing().closeDialog(edit));
  act(() => hook.routing().openDialog(other));
  hook.setDialogRequest.mockClear();
  hook.toggleModal();
  expect(hook.setDialogRequest).not.toHaveBeenCalled();
  // The dialog opened in the gap is the open one: a close that names it goes through, one that names the earlier dialog does not.
  act(() => hook.routing().closeDialog(edit));
  expect(hook.setDialogRequest).not.toHaveBeenCalled();
  act(() => hook.routing().closeDialog(other));
  expect(hook.setDialogRequest).toHaveBeenCalledExactlyOnceWith(undefined);
});

it("keeps a held confirmation for the release after the gap's dialog closes", () => {
  const hook = mountWithLateEffects();
  act(() => hook.routing().openDialog(edit));
  act(() => hook.routing().stableOpenDialogWhenFree(website));
  act(() => hook.routing().closeDialog(edit));
  act(() => hook.routing().openDialog({ kind: "new-console" }));
  hook.toggleModal();
  expect(hook.setDialogRequest).not.toHaveBeenCalledWith(website);
  act(() => hook.routing().closeDialog());
  hook.setDialogRequest.mockClear();
  hook.toggleModal();
  expect(hook.setDialogRequest).toHaveBeenCalledExactlyOnceWith(website);
});

it("opens a held confirmation after a close that names the dialog, as a dialog's own close does", () => {
  const hook = mountWithState();
  hook.open(edit);
  hook.openWhenFree(api, website);
  act(() => hook.routing().closeDialog(edit));
  expect(hook.shown()).toBe(api);
  act(() => hook.routing().closeDialog(api));
  expect(hook.shown()).toBe(website);
  act(() => hook.routing().closeDialog(website));
  expect(hook.shown()).toBeUndefined();
});

it("ignores the stale close of a replaced dialog, keeping the held confirmation waiting until the replacing one closes", () => {
  const hook = mountWithState();
  hook.open(edit);
  hook.openWhenFree(website);
  hook.open({ kind: "new-console" });
  act(() => hook.routing().closeDialog(edit));
  expect(hook.shown()).toEqual({ kind: "new-console" });
  hook.close();
  expect(hook.shown()).toBe(website);
});

it("releases a confirmation held for an alert that was reported and taken back before the window rendered", () => {
  const hook = mountWithState();
  act(() => {
    errorAlerts.report({ message: "Could not do it", title: "Could not do it" });
    hook.routing().stableOpenDialogWhenFree(website);
    errorAlerts.acknowledge();
  });
  expect(hook.shown()).toBe(website);
});

it("keeps a held confirmation back when an alert is reported in the commit that clears the dialog, and opens it after the alert", () => {
  // Reported after the render that showed no dialog and before the release effect of that commit runs, which therefore
  // cannot know of it from what it rendered.
  let reportOnClear = false;
  const hook = mountWithState((shown) => {
    if (reportOnClear && shown === undefined) errorAlerts.report({ message: "Could not do it", title: "Could not do it" });
  });
  hook.open(edit);
  hook.openWhenFree(website);
  reportOnClear = true;
  hook.close();
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(hook.shown()).toBeUndefined();
  act(() => errorAlerts.acknowledge());
  expect(hook.shown()).toBe(website);
});
