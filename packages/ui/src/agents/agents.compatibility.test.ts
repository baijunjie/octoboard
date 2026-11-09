import { expect, it } from "vitest";

import { consoleOf, projectOf } from "../fixtures/builders";
import { TEST_AGENTS } from "../test/testProviders";
import { inheritedSessionLaunch, launchWithOverrides } from "./agents";

it.each(TEST_AGENTS)("resolves %s's compatibility per setting through the project chain and keeps console sessions separate", (agent) => {
  const owner = { ...consoleOf("c", "Console"),
    launch: { [agent]: { model: null, effort: null, compatibility: { consoleOnly: false, shared: true } } },
    project_session_launch: { [agent]: { model: null, effort: null, compatibility: { inherited: false, shared: true } } },
  };
  const project = { ...projectOf("p", "c", "Project"), launch: { [agent]: { model: null, effort: null, compatibility: { shared: false, projectOnly: true } } } };
  expect(inheritedSessionLaunch(owner, project, agent, undefined).compatibility).toEqual({ inherited: false, shared: false, projectOnly: true });
  expect(inheritedSessionLaunch(owner, undefined, agent, undefined, true).compatibility).toEqual({ inherited: false, shared: true });
  expect(inheritedSessionLaunch(owner, undefined, agent, undefined, false).compatibility).toEqual({ consoleOnly: false, shared: true });
  expect(inheritedSessionLaunch(consoleOf("empty", "Empty"), undefined, agent, undefined).compatibility).toEqual({});
});

it("applies compatibility overrides per switch and preserves the inherited launch", () => {
  const inherited = { model: "model", effort: "high", compatibility: { kept: true, changed: true, cleared: false } };
  expect(launchWithOverrides(inherited, { compatibility: { changed: false, cleared: null, added: true } })).toEqual({
    model: "model", effort: "high", compatibility: { kept: true, changed: false, cleared: false, added: true },
  });
  expect(inherited.compatibility).toEqual({ kept: true, changed: true, cleared: false });
  expect(launchWithOverrides(inherited, undefined)).toEqual(inherited);
});
