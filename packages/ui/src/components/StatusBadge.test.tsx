import { renderToStaticMarkup } from "react-dom/server";
import { expect, it } from "vitest";

import { StatusBadge } from "./StatusBadge";

const html = (element: React.ReactElement) => renderToStaticMarkup(element);

it("keeps the neutral bordered badge as the default, with the comma for assistive technology", () => {
  const markup = html(<StatusBadge>Not added</StatusBadge>);
  expect(markup).toContain('<span class="sr-only">, </span>');
  expect(markup).toContain('data-color="default"');
  expect(markup).toContain('data-variant="bordered"');
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
  expect(markup).not.toContain("px-2");
  expect(html(<StatusBadge separated={false}>M</StatusBadge>)).toContain("px-2");
});
