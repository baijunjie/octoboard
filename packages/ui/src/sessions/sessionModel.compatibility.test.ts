import { expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import { TEST_PROVIDERS } from "../test/testProviders";
import { inheritedLaunch, sessionModelEffort } from "./sessionModel";

it("uses held session compatibility over inherited switches, independently of observed model and effort", () => {
  const session = sessionOf("s", "c", "p", "Session", "idle", { compatibility: { shared: false, own: true }, observed_model: "observed", observed_effort: "high" });
  const inherited = { model: "inherited", effort: "low", compatibility: { shared: true, kept: false } };
  expect(sessionModelEffort(session, inherited, undefined)).toEqual({ model: "observed", effort: "high", compatibility: { shared: false, kept: false, own: true } });
});

it("inherits a missing project's console project-session switches and follows no chain without a console", () => {
  const session = sessionOf("s", "c", "p", "Session", "idle");
  const owner = consoleOf("c", "Console", { launch: { [session.agent]: { compatibility: { wrong: true } } }, project_session_launch: { [session.agent]: { compatibility: { held: false } } } });
  expect(inheritedLaunch(session, new Map([["c", owner]]), new Map(), TEST_PROVIDERS).compatibility).toEqual({ held: false });
  const project = projectOf("p", "c", "Project", { launch: { [session.agent]: { compatibility: { held: true } } } });
  expect(inheritedLaunch(session, new Map([["c", owner]]), new Map([["p", project]]), TEST_PROVIDERS).compatibility).toEqual({ held: true });
  expect(inheritedLaunch(session, new Map(), new Map([["p", project]]), TEST_PROVIDERS).compatibility).toEqual({});
});
