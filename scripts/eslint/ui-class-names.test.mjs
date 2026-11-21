import { test } from "node:test";
import { RuleTester } from "eslint";
import tseslint from "typescript-eslint";

import { noBorderDivider, noEllipsis, noFocusVisible, noPhysicalDirection } from "./ui-class-names.mjs";

RuleTester.describe = (_name, run) => run();
RuleTester.it = (name, run) => test(name, run);
RuleTester.itOnly = (name, run) => test(name, { only: true }, run);

const tester = new RuleTester({
  languageOptions: { parser: tseslint.parser, parserOptions: { ecmaFeatures: { jsx: true } } },
});

const forbidden = (name) => ({ messageId: "forbidden", data: { name } });

tester.run("no-focus-visible", noFocusVisible, {
  valid: [
    { name: "react-aria's data attribute", code: 'const a = <i className="data-[focus-visible=true]:ring-2" />;' },
    { name: "a plain focus variant", code: 'const a = <i className="focus:ring-2 focus-within:ring-2" />;' },
    { name: "the attribute selector", code: 'const css = "[data-expand-button][data-focus-visible] { outline: 2px solid; }";' },
    { name: "a name containing the word", code: 'const a = "useFocusVisibleProps";' },
    { name: "the bare word, not a variant or a selector", code: 'const a = "focus-visible";' },
  ],
  invalid: [
    { name: "the variant", code: 'const a = <i className="flex focus-visible:ring-2" />;', errors: [forbidden("focus-visible:ring-2")] },
    { name: "a named group", code: 'const a = <i className="group-focus-visible/item:opacity-100" />;', errors: [forbidden("group-focus-visible/item:opacity-100")] },
    { name: "a named peer", code: 'const a = <i className="peer-focus-visible/x:block" />;', errors: [forbidden("peer-focus-visible/x:block")] },
    { name: "has, in and not", code: 'const a = <i className="has-focus-visible:a in-focus-visible:b not-focus-visible:c" />;', errors: [forbidden("has-focus-visible:a"), forbidden("in-focus-visible:b"), forbidden("not-focus-visible:c")] },
    { name: "a group variant", code: 'const a = <i className="group-focus-visible:opacity-100" />;', errors: [forbidden("group-focus-visible:opacity-100")] },
    { name: "after another variant", code: 'const a = <i className="hover:focus-visible:ring-2" />;', errors: [forbidden("hover:focus-visible:ring-2")] },
    { name: "in a template literal", code: "const a = <i className={`flex ${x} peer-focus-visible:ring-2`} />;", errors: [forbidden("peer-focus-visible:ring-2")] },
    { name: "the selector in a stylesheet string", code: 'const css = "button:focus-visible { outline: none; }";', errors: [forbidden(":focus-visible")] },
    { name: "after an arbitrary variant", code: 'const a = <i className="[&_a]:focus-visible:ring-2" />;', errors: [forbidden("[&_a]:focus-visible:ring-2")] },
    { name: "the selector in a template literal stylesheet", code: "const css = `.row ${x} button:focus-visible { outline: none; }`;", errors: [forbidden(":focus-visible")] },
    { name: "the selector inside :has", code: 'const css = ".row:has(:focus-visible) { outline: none; }";', errors: [forbidden(":focus-visible")] },
  ],
});

tester.run("no-ellipsis", noEllipsis, {
  valid: [
    { name: "no clip", code: 'const a = <i className="min-w-0 flex-1 whitespace-pre" />;' },
    { name: "a word in prose", code: 'const a = "Long titles wrap rather than truncated text";' },
    { name: "a selector that tests for the absence", code: 'const a = ".line-clamp-2";' },
    { name: "line-clamp-none lifts a clamp", code: 'const a = <i className="line-clamp-none" />;' },
    { name: "text-overflow clip", code: 'const a = <i className="text-clip" />;' },
    { name: "a longer word that starts like the utility", code: 'const a = <i className="truncated-label" />;' },
  ],
  invalid: [
    { name: "truncate", code: 'const a = <i className="min-w-0 truncate" />;', errors: [forbidden("truncate")] },
    { name: "text-ellipsis", code: 'const a = <i className="text-ellipsis" />;', errors: [forbidden("text-ellipsis")] },
    { name: "a line clamp", code: 'const a = <i className="line-clamp-2" />;', errors: [forbidden("line-clamp-2")] },
    { name: "an arbitrary line clamp", code: 'const a = <i className="line-clamp-[3]" />;', errors: [forbidden("line-clamp-[3]")] },
    { name: "behind a variant", code: 'const a = <i className="md:truncate!" />;', errors: [forbidden("md:truncate!")] },
    { name: "in a template literal", code: "const a = <i className={`min-w-0 ${x} truncate`} />;", errors: [forbidden("truncate")] },
    { name: "a class constant outside any className", code: 'const CELL = "block truncate";', errors: [forbidden("truncate")] },
    { name: "text-ellipsis behind a variant", code: 'const a = <i className="sm:text-ellipsis" />;', errors: [forbidden("sm:text-ellipsis")] },
  ],
});

tester.run("no-physical-direction", noPhysicalDirection, {
  valid: [
    { name: "logical utilities", code: 'const a = <i className="ms-2 me-2 ps-3 pe-3 start-0 end-0 text-start text-end rounded-s-md rounded-e-md border-s border-e" />;' },
    { name: "a vertical or symmetric utility", code: 'const a = <i className="mt-2 px-3 top-0 rounded-lg border-t text-center" />;' },
    { name: "the twin under rtl", code: 'const a = <i className="rtl:right-0 rtl:left-auto! rtl:pl-(--inset)" />;' },
    { name: "the twin under ltr", code: 'const a = <i className="ltr:ml-2" />;' },
    { name: "prose", code: 'const a = "Turn left, then drive right";' },
    { name: "scroll margin on the logical side", code: 'const a = <i className="scroll-ms-2 scroll-pe-2" />;' },
    { name: "rounded sizes", code: 'const a = <i className="rounded-lg rounded-xl rounded-full" />;' },
    { name: "a colour whose name starts with a side letter", code: 'const a = <i className="border-lime-500 border-red-500 text-lg" />;' },
    { name: "a border under the rtl variant", code: 'const a = <i className="rtl:border-r rtl:rounded-l-md rtl:text-left" />;' },
    { name: "prose in a comment", code: "// right-click opens the menu, left-click selects\nconst a = 1;" },
    { name: "prose in JSX text", code: "const a = <p>Use right-click or left-click.</p>;" },
  ],
  invalid: [
    { name: "margin", code: 'const a = <i className="ml-2 mr-2" />;', errors: [forbidden("ml-2"), forbidden("mr-2")] },
    { name: "padding with an arbitrary value", code: 'const a = <i className="pl-[3px] pr-(--pane-inset)" />;', errors: [forbidden("pl-[3px]"), forbidden("pr-(--pane-inset)")] },
    { name: "a negative margin", code: 'const a = <i className="-ml-1" />;', errors: [forbidden("-ml-1")] },
    { name: "an offset", code: 'const a = <i className="absolute left-0 right-2" />;', errors: [forbidden("left-0"), forbidden("right-2")] },
    { name: "alignment", code: 'const a = <i className="text-left text-right" />;', errors: [forbidden("text-left"), forbidden("text-right")] },
    { name: "rounding", code: 'const a = <i className="rounded-l rounded-r-md rounded-tl-lg rounded-br" />;', errors: [forbidden("rounded-l"), forbidden("rounded-r-md"), forbidden("rounded-tl-lg"), forbidden("rounded-br")] },
    { name: "scroll margin and padding", code: 'const a = <i className="scroll-ml-2 scroll-mr-2 scroll-pl-2 scroll-pr-2" />;', errors: [forbidden("scroll-ml-2"), forbidden("scroll-mr-2"), forbidden("scroll-pl-2"), forbidden("scroll-pr-2")] },
    { name: "float and clear", code: 'const a = <i className="float-left float-right clear-left clear-right" />;', errors: [forbidden("float-left"), forbidden("float-right"), forbidden("clear-left"), forbidden("clear-right")] },
    { name: "a border", code: 'const a = <i className="border-l border-r-2" />;', errors: [forbidden("border-l"), forbidden("border-r-2")] },
    { name: "the other rounded corners", code: 'const a = <i className="rounded-tr-md rounded-bl-lg" />;', errors: [forbidden("rounded-tr-md"), forbidden("rounded-bl-lg")] },
    { name: "a negative offset", code: 'const a = <i className="-left-1 -right-px" />;', errors: [forbidden("-left-1"), forbidden("-right-px")] },
    { name: "a physical utility next to its rtl twin", code: 'const a = <i className="left-0 rtl:right-0" />;', errors: [forbidden("left-0")] },
    { name: "behind a variant", code: 'const a = <i className="md:hover:pl-2" />;', errors: [forbidden("md:hover:pl-2")] },
    { name: "an arbitrary variant", code: 'const a = <i className="[&>svg]:pr-2" />;', errors: [forbidden("[&>svg]:pr-2")] },
    { name: "in a template literal", code: "const a = <i className={`flex ${x} ml-2`} />;", errors: [forbidden("ml-2")] },
  ],
});

tester.run("no-border-divider", noBorderDivider, {
  valid: [
    { name: "a groove", code: 'const a = <i className="groove-b" />;' },
    { name: "a hairline", code: 'const a = <i className="hairline-b" />;' },
    { name: "a frame round the whole box", code: 'const a = <i className="rounded-xl border border-separator p-4" />;' },
    { name: "a side's border in another colour", code: 'const a = <i className="border-b border-transparent" />;' },
    { name: "the colour with a width only", code: 'const a = <i className="border-2 border-separator" />;' },
    { name: "a table's hairline", code: 'const a = <i className="hairline-b border-separator" />;' },
    { name: "a zero-width side", code: 'const a = <i className="border-b-0 border-separator" />;' },
    { name: "divide width zero", code: 'const a = <i className="divide-y-0 divide-x-0" />;' },
    // Deliberate: a menu's filter-row rule (MenuFilter) is a menu rule, not a pane divider, and is drawn with the variable.
    { name: "the separator variable on a side's border", code: 'const a = <i className="border-b border-(--separator)" />;' },
    { name: "the colour in another string", code: 'const a = <i className="border-b" data-x="border-separator" />;' },
  ],
  invalid: [
    { name: "a bottom border", code: 'const a = <i className="border-b border-separator" />;', errors: [forbidden("border-separator")] },
    { name: "a top border with a width", code: 'const a = <i className="border-t-2 border-separator" />;', errors: [forbidden("border-separator")] },
    { name: "a logical side", code: 'const a = <i className="border-s border-separator" />;', errors: [forbidden("border-separator")] },
    { name: "a variant of the side", code: 'const a = <i className="md:border-y hover:border-separator" />;', errors: [forbidden("hover:border-separator")] },
    { name: "the horizontal pair", code: 'const a = <i className="border-x border-separator" />;', errors: [forbidden("border-separator")] },
    { name: "a physical side", code: 'const a = <i className="border-l border-separator" />;', errors: [forbidden("border-separator")] },
    { name: "in a template literal", code: "const a = <i className={`border-b border-separator ${x}`} />;", errors: [forbidden("border-separator")] },
    { name: "divide behind a variant", code: 'const a = <i className="md:divide-y-2" />;', errors: [forbidden("md:divide-y-2")] },
    { name: "the opacity form", code: 'const a = <i className="border-b border-separator/50" />;', errors: [forbidden("border-separator/50")] },
    { name: "divide-y", code: 'const a = <i className="divide-y" />;', errors: [forbidden("divide-y")] },
    { name: "divide colour", code: 'const a = <i className="divide-x divide-separator" />;', errors: [forbidden("divide-x"), forbidden("divide-separator")] },
  ],
});
