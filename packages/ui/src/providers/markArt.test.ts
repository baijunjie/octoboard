import { describe, expect, it } from "vitest";

import { markArt } from "./markArt";
import { BUILT_IN } from "./registry";
import type { ProviderMark } from "./types";

const ids = (art: string) => [...art.matchAll(/id="([^"]+)"/g)].map((match) => match[1]);
const references = (art: string) => [...art.matchAll(/url\(#([^)]+)\)/g)].map((match) => match[1]);

const withArt = BUILT_IN.flatMap((module) => {
  const { art } = module.mark as ProviderMark;
  return art ? [[module.id, art] as const] : [];
});

describe("markArt", () => {
  it("gives a mark drawn as its path alone no art", () => {
    expect(markArt({ d: "M0 0h24v24H0z", fill: "currentColor" }, "a")).toBeUndefined();
  });

  it("makes the ids the art defines and refers to unique to the prefix", () => {
    const mark: ProviderMark = { d: "M0 0h24v24H0z", fill: "currentColor", art: '<mask id="{id}-m"/><g mask="url(#{id}-m)"/>' };
    expect(markArt(mark, "first")).toBe('<mask id="first-m"/><g mask="url(#first-m)"/>');
    expect(markArt(mark, "second")).toBe('<mask id="second-m"/><g mask="url(#second-m)"/>');
  });

  // An id written without the placeholder would be shared by two drawings of the mark on one page.
  it.each(withArt)("%s's art writes every id it defines or refers to with the placeholder", (_id, art) => {
    expect(ids(art).length).toBeGreaterThan(0);
    for (const id of [...ids(art), ...references(art)]) expect(id.startsWith("{id}")).toBe(true);
  });
});
