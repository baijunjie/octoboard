import { test } from "node:test";
import { RuleTester } from "eslint";
import tseslint from "typescript-eslint";

import { nativeTitle } from "./native-title.mjs";

RuleTester.describe = (_name, run) => run();
RuleTester.it = (name, run) => test(name, run);
RuleTester.itOnly = (name, run) => test(name, { only: true }, run);

const tester = new RuleTester({
  languageOptions: { parser: tseslint.parser, parserOptions: { ecmaFeatures: { jsx: true } } },
});

const BUTTON = 'import { Button } from "../primitives"; ';

tester.run("native-title", nativeTitle, {
  valid: [
    { name: "no title", code: 'const a = <button aria-label="x" />;' },
    { name: "a component of the application's own", code: 'import { ErrorAlert } from "./ErrorAlert"; const a = <ErrorAlert title="x" />;' },
    { name: "a local component", code: 'function Part({ title }: any) { return <section>{title}</section>; } const a = <Part title="x" />;' },
    { name: "a member element", code: BUTTON + 'const a = <Button.Group title="x" />;' },
    { name: "a shadowing binding", code: BUTTON + 'function F({ Button }: any) { return <Button title="x" />; }' },
    { name: "a component of another module whose name only ends alike", code: 'import { Button } from "./morePrimitives2"; const a = <Button title="x" />;' },
    { name: "a namespace member of the barrel", code: 'import * as P from "../primitives"; const a = <P.Button title="x" />;' },
    { name: "another attribute", code: 'const a = <div data-title="x" />;' },
  ],
  invalid: [
    { name: "a DOM element", code: 'const a = <span title="x" />;', errors: [{ messageId: "title", data: { name: "span" } }] },
    { name: "a form control", code: 'const a = <input title="x" />;', errors: [{ messageId: "title", data: { name: "input" } }] },
    { name: "a default import of the barrel", code: 'import Button from "../primitives"; const a = <Button title="x" />;', errors: [{ messageId: "title", data: { name: "Button" } }] },
    { name: "a primitive used inside a nested function", code: BUTTON + 'function F() { return () => <Button title="x" />; }', errors: [{ messageId: "title", data: { name: "Button" } }] },
    { name: "a title among other attributes", code: 'const a = <button type="button" title="x" aria-label="y" />;', errors: [{ messageId: "title", data: { name: "button" } }] },
    { name: "an expression value", code: "const a = <a href={h} title={t}>x</a>;", errors: [{ messageId: "title", data: { name: "a" } }] },
    { name: "a primitive", code: BUTTON + 'const a = <Button title="x" />;', errors: [{ messageId: "title", data: { name: "Button" } }] },
    { name: "a primitive from the barrel in a deeper directory", code: 'import { Button as Press } from "../../primitives"; const a = <Press title="x" />;', errors: [{ messageId: "title", data: { name: "Press" } }] },
  ],
});
