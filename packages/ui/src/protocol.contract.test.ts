import { readFileSync } from "node:fs";

import { expect, it } from "vitest";

const read = (name: string) => readFileSync(new URL(name, import.meta.url), "utf8");

// `protocol.contract.ts` checks `protocol.ts` against `protocol.generated.ts` under `tsc`, but only for the generated
// types it names: a type the daemon gains would otherwise be checked by nothing until someone remembered to add a line.
// A name counts only when it reaches the `Checked` tuple, directly or through a type alias the tuple uses; a name in a
// comment, or in an alias nothing asserts, does not.
it("asserts every generated type in the contract", () => {
  const generated = [...read("./protocol.generated.ts").matchAll(/^export type (\w+)/gm)].map((match) => match[1]);
  const contract = read("./protocol.contract.ts").replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "");
  const declarations = contract.split(/^(?=(?:export )?type \w)/m);
  const checked = declarations.find((declaration) => /^(?:export )?type Checked\b/.test(declaration));
  expect(checked).toBeDefined();
  let reached = checked!;
  for (let grew = true; grew;) {
    grew = false;
    for (const declaration of declarations) {
      const alias = /^(?:export )?type (\w+)/.exec(declaration)?.[1];
      if (alias && !reached.includes(declaration) && new RegExp(`\\b${alias}\\b`).test(reached)) {
        reached += declaration;
        grew = true;
      }
    }
  }
  const named = new Set([...reached.matchAll(/\bG\.(\w+)/g)].map((match) => match[1]));
  expect(generated.length).toBeGreaterThan(100);
  expect(generated.filter((name) => !named.has(name))).toEqual([]);
});
