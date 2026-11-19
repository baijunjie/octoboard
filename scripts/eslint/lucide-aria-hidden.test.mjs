import { test } from "node:test";
import { RuleTester } from "eslint";
import tseslint from "typescript-eslint";

import { lucideAriaHidden } from "./lucide-aria-hidden.mjs";

RuleTester.describe = (_name, run) => run();
RuleTester.it = (name, run) => test(name, run);
RuleTester.itOnly = (name, run) => test(name, { only: true }, run);

const tester = new RuleTester({
  languageOptions: { parser: tseslint.parser, parserOptions: { ecmaFeatures: { jsx: true } } },
});

const POWER = 'import { Power } from "lucide-react"; ';

tester.run("lucide-aria-hidden", lucideAriaHidden, {
  valid: [
    { name: "no aria-hidden", code: POWER + 'const a = <Power className="size-4" />;' },
    { name: "not from lucide-react", code: 'import { Power } from "./icons"; const a = <Power aria-hidden="true" />;' },
    { name: "a plain element", code: 'const a = <span aria-hidden="true">x</span>;' },
    { name: "another namespace", code: 'import * as Icons from "lucide-react"; const a = <Other.Power aria-hidden />;' },
    { name: "another aria prop", code: POWER + 'const a = <Power aria-hidden="true" aria-label="x" />;' },
    { name: "a role", code: POWER + 'const a = <Power aria-hidden="true" role="img" />;' },
    { name: "a title", code: POWER + 'const a = <Power aria-hidden="true" title="x" />;' },
    { name: "children", code: POWER + 'const a = <Power aria-hidden="true"><title>x</title></Power>;' },
    { name: "a spread", code: POWER + 'const a = <Power {...p} aria-hidden="true" />;' },
    { name: "false", code: POWER + "const a = <Power aria-hidden={false} />;" },
    { name: "a dynamic value", code: POWER + "const a = <Power aria-hidden={hidden} />;" },
    { name: "a namespace used as an element", code: 'import * as Icons from "lucide-react"; const a = <Icons aria-hidden />;' },
    { name: "a named import used as a namespace", code: POWER + "const a = <Power.X aria-hidden />;" },
    { name: "a shadowing binding", code: POWER + 'function F({ Power }: any) { return <Power aria-hidden="true" />; }' },
  ],
  invalid: [
    {
      name: "a named import",
      code: POWER + 'const a = <Power aria-hidden="true" className="size-4" />;',
      output: POWER + 'const a = <Power className="size-4" />;',
      errors: [{ messageId: "redundant", data: { name: "Power" } }],
    },
    {
      name: "an alias, value absent",
      code: 'import { RefreshCw as Reload } from "lucide-react"; const a = <Reload aria-hidden />;',
      output: 'import { RefreshCw as Reload } from "lucide-react"; const a = <Reload />;',
      errors: [{ messageId: "redundant", data: { name: "Reload" } }],
    },
    {
      name: "a namespace member, {true}",
      code: 'import * as Icons from "lucide-react"; const a = <Icons.Power aria-hidden={true} />;',
      output: 'import * as Icons from "lucide-react"; const a = <Icons.Power />;',
      errors: [{ messageId: "redundant", data: { name: "Icons.Power" } }],
    },
    {
      name: "an empty element with a closing tag",
      code: POWER + 'const a = <Power aria-hidden="true"></Power>;',
      output: POWER + "const a = <Power></Power>;",
      errors: [{ messageId: "redundant" }],
    },
    {
      name: "several lines keep the layout of the next attribute",
      code: POWER + 'const a = (\n  <Power\n    aria-hidden="true"\n    className="size-4"\n  />\n);',
      output: POWER + 'const a = (\n  <Power\n    className="size-4"\n  />\n);',
      errors: [{ messageId: "redundant" }],
    },
    {
      name: "a comment before the attribute is kept",
      code: POWER + "const a = <Power /* why */ aria-hidden />;",
      output: POWER + "const a = <Power /* why */ />;",
      errors: [{ messageId: "redundant" }],
    },
  ],
});
