// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { Description, Input, Label, TextField } from "@heroui/react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { CodeBlock } from "./CodeBlock";
import { InlineReference } from "./InlineReference";
import { TintedAlert } from "./TintedAlert";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(children: React.ReactNode) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(children));
  return container;
}

it("inherits the closest alert tone even when the caller requests a conflicting color", () => {
  const container = mount(
    <>
      <InlineReference monospace wrap size="xs">neutral</InlineReference>
      <InlineReference monospace wrap size="xs" color="help">purple</InlineReference>
      <TintedAlert status="danger">
        <InlineReference monospace wrap size="xs" color="success">outer</InlineReference>
        <TintedAlert status="warning"><InlineReference monospace wrap size="xs" color="danger">inner</InlineReference></TintedAlert>
        <CodeBlock color="accent">outer block</CodeBlock>
      </TintedAlert>
      <InlineReference monospace wrap size="xs">after</InlineReference>
    </>,
  );
  expect(Array.from(container.querySelectorAll("code")).map((el) => el.dataset.color)).toEqual([
    "default", "help", "danger", "warning", "danger", "default",
  ]);
  expect(container.querySelector('[role="alert"]')).toBeNull();
});

it("takes an accent alert's colour and lets a neutral alert override explicit color", () => {
  const container = mount(
    <>
      <TintedAlert status="accent"><InlineReference monospace wrap size="xs">accent</InlineReference></TintedAlert>
      <TintedAlert><InlineReference monospace wrap size="xs" color="danger">neutral</InlineReference></TintedAlert>
    </>,
  );
  expect(Array.from(container.querySelectorAll("code")).map((el) => el.dataset.color)).toEqual(["accent", "default"]);
});

it("isolates code typography without consuming the field's description slot", () => {
  const container = mount(
    <TextField>
      <Label>Directory <InlineReference monospace wrap size="xs">~/Projects</InlineReference></Label>
      <Input />
      <Description>Read <InlineReference monospace wrap size="xs">AGENTS.md</InlineReference> first.</Description>
    </TextField>,
  );
  const description = container.querySelector('[data-slot="description"]')!;
  expect(description.textContent).toBe("Read AGENTS.md first.");
  expect(container.querySelector("input")?.getAttribute("aria-describedby")?.split(" ")).toContain(description.id);
  for (const code of container.querySelectorAll("code")) {
    expect(code.getAttribute("dir")).toBe("ltr");
    expect(code.classList.contains("select-text")).toBe(true);
    expect(code.hasAttribute("slot")).toBe(false);
  }
});

it("preserves URL text and repeated separators while allowing paths to wrap", () => {
  const literal = "https://example.test/projects//日本語/";
  const container = mount(<InlineReference monospace wrap size="xs">{literal}</InlineReference>);
  const code = container.querySelector("code")!;
  expect(code.textContent).toBe(literal);
  expect(code.querySelector("wbr")).not.toBeNull();
});

it("preserves diagnostic lines and supports wrapping or a full-width scrollable trace", () => {
  const text = "first line\n    second line";
  const container = mount(<><CodeBlock>{text}</CodeBlock><CodeBlock dir="ltr" wrap={false} className="text-xs">{text}</CodeBlock></>);
  const [wrapped, trace] = Array.from(container.querySelectorAll("pre"));
  expect(wrapped.getAttribute("dir")).toBe("auto");
  expect(wrapped.classList.contains("whitespace-pre-wrap")).toBe(true);
  expect(wrapped.classList.contains("min-w-0")).toBe(true);
  expect(trace.getAttribute("dir")).toBe("ltr");
  expect(trace.classList.contains("min-w-full")).toBe(true);
  expect(trace.classList.contains("w-max")).toBe(true);
  expect(trace.classList.contains("whitespace-pre")).toBe(true);
  expect(trace.classList.contains("text-xs")).toBe(true);
  for (const pre of [wrapped, trace]) {
    expect(pre.textContent).toBe(text);
    expect(pre.querySelector("code")?.classList.contains("w-full")).toBe(true);
    expect(pre.querySelector("code")?.classList.contains("select-text")).toBe(true);
  }
});

it("defaults to xs and marks an unbroken chip", () => {
  const container = mount(
    <>
      <InlineReference>plain</InlineReference>
      <InlineReference size="sm">padded</InlineReference>
      <InlineReference unbroken>whole</InlineReference>
    </>,
  );
  const chips = Array.from(container.querySelectorAll<HTMLElement>("[data-inline-reference]"));
  expect(chips.map((chip) => chip.dataset.size)).toEqual(["xs", "sm", "xs"]);
  // style.css tightens the line height of an unbroken chip, so it stays clear of the line below.
  expect(chips[2].classList).toContain("inline-reference--unbroken");
  expect(chips[1].classList).not.toContain("inline-reference--unbroken");
});

it("draws each colour's fill and text from the shared tint, the badge's, and never its edge", () => {
  // jsdom resolves no `var()`, so the rules are read from style.css.
  const css = readFileSync(resolve(import.meta.dirname, "../style.css"), "utf8");
  const tints = { default: "default", accent: "accent", help: "help", success: "success", warning: "warning", danger: "danger" };
  for (const [color, tint] of Object.entries(tints)) {
    expect(css).toContain(`.inline-reference.chip--soft[data-color="${color}"] { --chip-bg: var(--tint-${tint}-bg); --chip-fg: var(--tint-${tint}-fg); }`);
  }
  expect(css).not.toMatch(/\.inline-reference[^{]*\{[^}]*--tint-\w+-edge/);
});

it("styles every size as padding around the font of the surrounding text", () => {
  // jsdom resolves neither inheritance nor `var()`, so the rules are read from style.css (by directory: under jsdom
  // `import.meta.url` is not a file URL);
  // scripts/gallery-chip-geometry.mjs measures the result in a browser.
  const css = readFileSync(resolve(import.meta.dirname, "../style.css"), "utf8");
  const rule = (selector: string) => css.match(new RegExp(`^${selector.replace(/[.[\]"]/g, "\\$&")} \\{([^}]*)\\}`, "m"))?.[1] ?? "";
  const padding = (selector: string) => ["block", "inline"].map((axis) => rule(selector).match(new RegExp(`--inline-reference-${axis}: (\\S+);`))?.[1]);
  for (const property of ["font-family", "font-size", "font-weight", "line-height"]) expect(rule(".inline-reference")).toContain(`${property}: inherit;`);
  expect(rule(".inline-reference")).toContain("display: inline;");
  expect(padding(".inline-reference")).toEqual(["0", "4px"]);
  expect(padding('.inline-reference[data-size="sm"]')).toEqual(["1px", "6px"]);
  expect(padding('.inline-reference[data-size="md"]')).toEqual(["2px", "8px"]);
  expect(padding('.inline-reference[data-size="lg"]')).toEqual(["3px", "10px"]);
  const container = mount(<>{(["xs", "sm", "md", "lg"] as const).map((size) => <InlineReference key={size} size={size}>{size}</InlineReference>)}</>);
  expect(Array.from(container.querySelectorAll<HTMLElement>("[data-inline-reference]"), (chip) => chip.dataset.size)).toEqual(["xs", "sm", "md", "lg"]);
});
