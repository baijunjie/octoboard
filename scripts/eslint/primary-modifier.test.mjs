import { test } from "node:test";
import { RuleTester } from "eslint";

import { primaryModifier } from "./primary-modifier.mjs";

RuleTester.describe = (_name, run) => run();
RuleTester.it = (name, run) => test(name, run);
RuleTester.itOnly = (name, run) => test(name, { only: true }, run);

const tester = new RuleTester({ languageOptions: { ecmaVersion: "latest", sourceType: "module" } });

tester.run("primary-modifier", primaryModifier, {
  valid: [
    { name: "one key", code: "const a = event.metaKey;" },
    { name: "a host-chosen key", code: 'const a = host.modifier === "meta" ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;' },
    { name: "a guard that also tests another key", code: 'if (event.key !== "Tab" || event.ctrlKey || event.metaKey) run();' },
    { name: "a guard on every modifier", code: "if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) run();" },
    { name: "neither, with another test", code: "const a = !event.ctrlKey && !event.metaKey && !event.altKey;" },
    { name: "positive reads joined by and", code: "const a = event.metaKey && event.ctrlKey;" },
    { name: "the same key twice", code: "const a = first.metaKey || second.metaKey;" },
    { name: "one of the two with a shift test", code: "const a = event.metaKey || event.shiftKey;" },
    { name: "neither, with the keys of another kind", code: "const a = !event.metaKey && !event.shiftKey;" },
    { name: "a computed member", code: 'const a = event["metaKey"] || event["ctrlKey"];' },
  ],
  invalid: [
    { name: "meta or ctrl", code: "const a = event.metaKey || event.ctrlKey;", errors: [{ messageId: "either" }] },
    { name: "ctrl or meta", code: "const a = event.ctrlKey || event.metaKey;", errors: [{ messageId: "either" }] },
    { name: "destructured names", code: "const a = metaKey || ctrlKey;", errors: [{ messageId: "either" }] },
    { name: "neither key, meta first", code: "if (!event.metaKey && !event.ctrlKey) run();", errors: [{ messageId: "either" }] },
    { name: "neither key, destructured", code: "const a = !metaKey && !ctrlKey;", errors: [{ messageId: "either" }] },
    { name: "keys of two different objects", code: "const a = first.metaKey || second.ctrlKey;", errors: [{ messageId: "either" }] },
    { name: "as a call argument", code: "run(event.metaKey || event.ctrlKey);", errors: [{ messageId: "either" }] },
    { name: "reported once for the whole chain", code: "const a = (event.metaKey || event.ctrlKey) ? 1 : 2;", errors: [{ messageId: "either" }] },
    { name: "inside a condition", code: "if ((e.metaKey || e.ctrlKey) && e.key === 'k') run();", errors: [{ messageId: "either" }] },
    { name: "neither key", code: "if (!event.ctrlKey && !event.metaKey) run();", errors: [{ messageId: "either" }] },
    { name: "a negated either", code: "const a = !(event.metaKey || event.ctrlKey);", errors: [{ messageId: "either" }] },
  ],
});
