import { expect, it } from "vitest";

import { agentFieldPaths, baseVersions } from "./writeConflict";

it.each(["launch", "project_session_launch"])("checks only changed %s switches at individual field paths", (prefix) => {
  const changes = { custom: { compatibility: { on: true, off: false, follow: null } }, other: { effort: "high", compatibility: {} } };
  const paths = agentFieldPaths(prefix, changes);
  expect(paths).toEqual([`${prefix}.custom.compatibility.on`, `${prefix}.custom.compatibility.off`, `${prefix}.custom.compatibility.follow`, `${prefix}.other.effort`]);
  const versions = Object.fromEntries([...paths, `${prefix}.custom.compatibility.untouched`].map((path, index) => [path, index + 2]));
  expect(baseVersions(versions, paths)).toEqual(Object.fromEntries(paths.map((path, index) => [path, index + 2])));
});
