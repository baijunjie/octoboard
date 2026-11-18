import { renderToStaticMarkup } from "react-dom/server";
import { expect, it } from "vitest";

import { StatusBadge } from "./StatusBadge";

const html = (element: React.ReactElement) => renderToStaticMarkup(element);

it("keeps the neutral bordered badge as the default, with the comma for assistive technology", () => {
  const markup = html(<StatusBadge>Not added</StatusBadge>);
  expect(markup).toContain('<span class="sr-only">, </span>');
  expect(markup).toContain('data-color="default"');
  expect(markup).toContain('data-variant="bordered"');
  expect(markup).toContain('data-slot="chip"');
  expect(markup).toContain('data-size="sm"');
  expect(markup).toContain("bg-[var(--tint-default-bg)] text-[var(--default-foreground)]");
  expect(markup).toContain("ring-1 ring-inset");
  expect(markup).toContain("ring-[var(--tint-default-edge)]");
  expect(markup).not.toContain("--default-soft");
});

it("draws a soft badge from the shared tint's fill and text and gives it no edge", () => {
  for (const color of ["accent", "help", "success", "warning", "danger"] as const) {
    const markup = html(<StatusBadge color={color} variant="soft">x</StatusBadge>);
    expect(markup).toContain(`bg-[var(--tint-${color}-bg)]`);
    expect(markup).toContain(`text-[var(--tint-${color}-fg)]`);
    expect(markup).not.toContain("ring-");
  }
  const neutral = html(<StatusBadge variant="soft">x</StatusBadge>);
  expect(neutral).toContain("bg-[var(--tint-default-bg)] text-[var(--default-foreground)]");
  expect(neutral).not.toContain("--default-soft");
  expect(neutral).not.toContain("ring-");
});

it("gives a bordered coloured badge the tint's fill and its edge", () => {
  for (const color of ["accent", "help", "success", "warning", "danger"] as const) {
    const markup = html(<StatusBadge color={color}>x</StatusBadge>);
    expect(markup).toContain(`bg-[var(--tint-${color}-bg)]`);
    expect(markup).toContain(`ring-[var(--tint-${color}-edge)]`);
    expect(markup).toContain("ring-1 ring-inset");
    expect(markup).toContain(`data-color="${color}"`);
  }
});

it("leaves out the comma without a name, wraps by overflow, and passes the rest to its span", () => {
  expect(html(<StatusBadge separated={false}>x</StatusBadge>)).not.toContain("sr-only");
  expect(html(<StatusBadge overflow="wrap">x</StatusBadge>)).toContain("[overflow-wrap:anywhere]");
  const markup = html(<StatusBadge dir="auto" aria-hidden="true" className="ms-auto">x</StatusBadge>);
  expect(markup).toContain('dir="auto"');
  expect(markup).toContain('aria-hidden="true"');
  expect(markup).toContain("ms-auto");
});

it("seats a single character in a fixed width with no side padding, so it is centred in either direction", () => {
  const markup = html(<StatusBadge single separated={false}>M</StatusBadge>);
  expect(markup).toContain("inline-block w-5 px-0 text-center");
  expect(markup).not.toContain('data-slot="chip-label"');
});

it("keeps the circle for an explicit single on content that is not detected as one character, such as an element", () => {
  expect(html(<StatusBadge single separated={false}><b>M</b></StatusBadge>)).toContain("inline-block w-5 px-0 text-center");
});

it.each([
  ["sm", "w-5"],
  ["md", "w-6"],
  ["lg", "w-7"],
] as const)("makes text of one character a circle without being asked, at size %s", (size, width) => {
  for (const content of ["M", 0, "\u{1D4B3}"]) {
    const markup = html(<StatusBadge size={size} separated={false}>{content}</StatusBadge>);
    expect(markup).toContain(`inline-block ${width} px-0 text-center`);
  }
});

it.each([
  ["sm", "px-2"],
  ["md", "px-2"],
  ["lg", "px-3"],
] as const)("keeps the pill padding for longer text, a longer number, an element or nothing, at size %s", (size, padding) => {
  for (const content of ["10", "", <b key="b">M</b>]) {
    const markup = html(<StatusBadge size={size} separated={false}>{content}</StatusBadge>);
    expect(markup).toContain(padding);
    expect(markup).not.toContain("px-0");
    expect(markup).not.toContain("inline-block");
    expect(markup).not.toContain("text-center");
  }
});

it("counts one character among empty children as a circle, but not an element or a second string beside it", () => {
  const Ring = () => <i />;
  const circle = "inline-block w-5 px-0 text-center";
  expect(html(<StatusBadge separated={false}>{false}{"8"}</StatusBadge>)).toContain(circle);
  expect(html(<StatusBadge separated={false}>{null}{"8"}{undefined}</StatusBadge>)).toContain(circle);
  for (const markup of [
    html(<StatusBadge separated={false}><Ring />{"8"}</StatusBadge>),
    html(<StatusBadge separated={false}>{"1"}{"2"}</StatusBadge>),
  ]) {
    expect(markup).toContain("px-2");
    expect(markup).not.toContain("px-0");
    expect(markup).not.toContain("inline-block");
  }
});

it("offers all three Chip sizes without adding label padding to compact text", () => {
  for (const size of ["sm", "md", "lg"] as const) {
    const markup = html(<StatusBadge size={size} separated={false}>15.7万</StatusBadge>);
    expect(markup).toContain(`chip--${size}`);
    expect(markup).toContain(`data-size="${size}"`);
    expect(markup).not.toContain('data-slot="chip-label"');
    expect(markup).toContain("15.7万");
  }
});
