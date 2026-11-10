// @vitest-environment jsdom
// Where focus goes, and whether the ring is forced, as the archive view closes: focus the view held, or lost to
// `<body>` as it went, is handed on once the close is committed; a close the user makes leaves the modality alone, one
// nobody made (what the view lists is gone) forces the ring. The real `ArchiveLayer` stands over the hook, as `App`
// has it, with the view's own content stubbed.
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { expect, it, onTestFinished, vi } from "vitest";

import { ArchiveLayer } from "../archive/ArchiveLayer";
import { consoleOf, projectOf } from "../fixtures/builders";
import type { ArchiveScope } from "../sidebar/types";
import { DaemonProvider, useDaemonStore } from "../store";
import { testDaemon } from "../test/testDaemon";
import { useArchiveView } from "./useArchiveView";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

vi.mock("../archive/ArchiveView", () => ({
  ArchiveView: () => <section data-region="archive" tabIndex={-1} />,
}));
// Motion is reduced, so that what leaves with an exit leaves at once.
vi.mock("../components/motion/reducedMotion", () => ({ prefersReducedMotion: () => true, onReducedMotion: () => () => {} }));

const main = consoleOf("c-main", "Main");
const site = projectOf("p-site", main.id, "Website");
const scope: ArchiveScope = { console: main.id, project: site.id };

/** The hook under the layer, with the archive open on `scope`. */
async function mountOpen({ opened = true }: { opened?: boolean } = {}) {
  const daemon = testDaemon({ state: { consoles: new Map([[main.id, main]]), projects: new Map([[site.id, site]]) } });
  const focusTerminal = vi.fn();
  let archive!: ReturnType<typeof useArchiveView>;
  function Probe() {
    const consoles = useDaemonStore((s) => s.consoles);
    const projects = useDaemonStore((s) => s.projects);
    archive = useArchiveView({ consoles, projects, revealArchive: () => {}, sidebarOpen: false, closeSidebar: () => {}, handOffToTerminal: focusTerminal });
    return (
      <ArchiveLayer
        view={archive.archiveOpen && archive.archiveConsole ? ({ console: archive.archiveConsole, project: archive.archiveProject } as never) : undefined}
      />
    );
  }
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  await act(async () => root.render(<DaemonProvider value={daemon}><Probe /></DaemonProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
    host.remove();
  });
  const previous = getInteractionModality();
  onTestFinished(() => void Reflect.apply(setInteractionModality, undefined, [previous]));
  const section = () => document.querySelector<HTMLElement>("[data-region=archive]");
  if (opened) {
    act(() => archive.stableOpenArchive(scope));
    expect(section()).not.toBeNull();
    act(() => section()!.focus());
  }
  act(() => void Reflect.apply(setInteractionModality, undefined, ["pointer"]));
  return { daemon, archive: () => archive, focusTerminal, section };
}

const modalityAfter = (previous: "pointer" | "keyboard") => expect(getInteractionModality()).toBe(previous);

it("leaves the modality alone when the user closes the view with focus in it, and hands focus on once", async () => {
  const { archive, focusTerminal, section } = await mountOpen();
  act(() => archive().closeArchive());
  expect(section()).toBeNull();
  expect(focusTerminal).toHaveBeenCalledOnce();
  modalityAfter("pointer");
});

it("forces the ring when what the view lists goes away with focus in it", async () => {
  const { daemon, focusTerminal, section } = await mountOpen();
  act(() => daemon.store.setState({ projects: new Map() }));
  expect(section()).toBeNull();
  expect(focusTerminal).toHaveBeenCalledOnce();
  modalityAfter("keyboard");
});

it("treats a scope the user set to none like a close of their own", async () => {
  const { archive, focusTerminal, section } = await mountOpen();
  act(() => archive().setArchiveScopeByUser(undefined));
  expect(section()).toBeNull();
  expect(focusTerminal).toHaveBeenCalledOnce();
  modalityAfter("pointer");
});

it("still forces the ring for a close nobody made after the user asked for the scope already open, which changes nothing", async () => {
  const { daemon, archive, focusTerminal, section } = await mountOpen();
  // The very same scope: React bails out of the state, so no commit follows to end the user's move.
  act(() => archive().setArchiveScopeByUser(scope));
  expect(section()).not.toBeNull();
  expect(focusTerminal).not.toHaveBeenCalled();
  act(() => daemon.store.setState({ projects: new Map() }));
  expect(section()).toBeNull();
  modalityAfter("keyboard");
});

it("still forces the ring for a close nobody made after the user moved the view to another scope that stays open", async () => {
  const second = projectOf("p-api", main.id, "API");
  const { daemon, archive, section } = await mountOpen();
  act(() => daemon.store.setState({ projects: new Map([[site.id, site], [second.id, second]]) }));
  act(() => archive().setArchiveScopeByUser({ console: main.id, project: second.id }));
  expect(section()).not.toBeNull();
  act(() => daemon.store.setState({ projects: new Map([[site.id, site]]) }));
  expect(section()).toBeNull();
  modalityAfter("keyboard");
});

it("reads whether the view is open at the time of the call, not when the caller was rendered", async () => {
  // A session opened by a reply that arrives after an await closes the view from a callback rendered before it opened.
  const { archive, focusTerminal, section } = await mountOpen({ opened: false });
  const stale = archive().setArchiveScopeByUser;
  act(() => archive().stableOpenArchive(scope));
  act(() => section()!.focus());
  act(() => void Reflect.apply(setInteractionModality, undefined, ["pointer"]));
  act(() => stale(undefined));
  expect(section()).toBeNull();
  expect(focusTerminal).toHaveBeenCalledOnce();
  modalityAfter("pointer");
});

it.each([
  ["held in the view", "view", true],
  ["lost to <body>", "body", true],
  ["on a control outside the view", "outside", false],
] as const)("hands focus on as the view closes only when it is %s", async (_case, where, handed) => {
  const { daemon, focusTerminal, section } = await mountOpen();
  const outside = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => outside.remove());
  if (where === "outside") act(() => outside.focus());
  if (where === "body") act(() => (document.activeElement as HTMLElement).blur());
  expect(where === "view" ? document.activeElement === section() : where === "outside" ? document.activeElement === outside : document.activeElement === document.body).toBe(true);
  act(() => daemon.store.setState({ projects: new Map() }));
  expect(section()).toBeNull();
  expect(focusTerminal).toHaveBeenCalledTimes(handed ? 1 : 0);
});
