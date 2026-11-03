// @vitest-environment jsdom
import { useEffect } from "react";
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import type { Agent } from "../../protocol";
import { mountHost } from "../../test/mountHost";
import { testDaemon } from "../../test/testDaemon";
import { useAgentList } from "./useAgentList";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);

const ADDED: Agent[] = ["claude", "codex", "grok"];

function mount() {
  const entry = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => entry.remove());
  const list: { current?: ReturnType<typeof useAgentList> } = {};
  function Harness() {
    list.current = useAgentList(ADDED);
    // What a section does when focus is lost, declared after the list as the section declares it.
    useEffect(() => {
      if (document.activeElement === document.body) entry.focus();
    }, [list.current.agents]);
    return <>{list.current.agents.map((agent) => <button key={agent} ref={list.current!.tabRef(agent)}>{agent}</button>)}</>;
  }
  const container = mountHost(<Harness />, { daemon: testDaemon({ request }) });
  const tab = (name: string) => [...container.querySelectorAll("button")].find((el) => el.textContent === name)!;
  return { list, entry, tab, order: () => [...container.querySelectorAll("button")].map((el) => el.textContent) };
}

it("gives the moved tab its focus back once the strip follows the new order, ahead of the section's own handoff", () => {
  const { list, entry, tab, order } = mount();
  // The menu that asked for the move has closed with its focus: nothing holds it.
  act(() => (document.activeElement as HTMLElement).blur());
  act(() => list.current!.move("claude", 1));
  expect(order()).toEqual(["codex", "claude", "grok"]);
  expect(document.activeElement).toBe(tab("claude"));
  expect(document.activeElement).not.toBe(entry);
  // Used once: a later change of the order leaves focus where it is.
  act(() => tab("grok").focus());
  act(() => list.current!.move("codex", 1));
  expect(document.activeElement).toBe(tab("grok"));
});
