// @vitest-environment jsdom
import { Description, Input, Label, TextField } from "@heroui/react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { CodeBlock } from "./CodeBlock";
import { InlineChip } from "./InlineChip";
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
      <InlineChip monospace wrap size="xs">neutral</InlineChip>
      <InlineChip monospace wrap size="xs" color="secondary">purple</InlineChip>
      <TintedAlert status="danger">
        <InlineChip monospace wrap size="xs" color="success">outer</InlineChip>
        <TintedAlert status="warning"><InlineChip monospace wrap size="xs" color="danger">inner</InlineChip></TintedAlert>
        <CodeBlock color="primary">outer block</CodeBlock>
      </TintedAlert>
      <InlineChip monospace wrap size="xs">after</InlineChip>
    </>,
  );
  expect(Array.from(container.querySelectorAll("code")).map((el) => el.dataset.color)).toEqual([
    "default", "secondary", "danger", "warning", "danger", "default",
  ]);
  expect(container.querySelector('[role="alert"]')).toBeNull();
});

it("maps accent to primary and lets a neutral alert override explicit color", () => {
  const container = mount(
    <>
      <TintedAlert status="accent"><InlineChip monospace wrap size="xs">accent</InlineChip></TintedAlert>
      <TintedAlert><InlineChip monospace wrap size="xs" color="danger">neutral</InlineChip></TintedAlert>
    </>,
  );
  expect(Array.from(container.querySelectorAll("code")).map((el) => el.dataset.color)).toEqual(["primary", "default"]);
});

it("isolates code typography without consuming the field's description slot", () => {
  const container = mount(
    <TextField>
      <Label>Directory <InlineChip monospace wrap size="xs">~/Projects</InlineChip></Label>
      <Input />
      <Description>Read <InlineChip monospace wrap size="xs">AGENTS.md</InlineChip> first.</Description>
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
  const container = mount(<InlineChip monospace wrap size="xs">{literal}</InlineChip>);
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
