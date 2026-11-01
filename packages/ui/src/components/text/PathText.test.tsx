// @vitest-environment jsdom
import { renderToStaticMarkup } from "react-dom/server";
import { expect, it, vi } from "vitest";

import { PathText } from "./PathText";

// What the path label asks of `FadeOverflow`, whose clipping, tooltip and fade are its own tests' business.
const labels = vi.hoisted(() => ({ props: [] as Record<string, unknown>[] }));
vi.mock("./FadeOverflow", () => ({
  FadeOverflow: (props: Record<string, unknown>) => {
    labels.props.push(props);
    return null;
  },
}));

const asked = (element: React.ReactElement) => {
  labels.props.length = 0;
  renderToStaticMarkup(element);
  return labels.props[0];
};

it("sets a path left to right and clipped from its start, with the path as the tooltip while clipped", () => {
  expect(asked(<PathText path="/home/me/src" className="block" />)).toMatchObject({
    as: "span",
    className: "block",
    dir: "ltr",
    clip: "start",
    titleWhenClipped: "/home/me/src",
    children: "/home/me/src",
  });
});

it("shows the wording it is given, and then keeps the full path as the tooltip even while it fits", () => {
  expect(asked(<PathText path="/home/me/src" shown="~/src" className="c" />)).toMatchObject({
    as: "span",
    titleWhenClipped: "/home/me/src",
    title: "/home/me/src",
    children: "~/src",
  });
  expect(asked(<PathText path="/home/me/src" className="c" />)).toMatchObject({ title: undefined, children: "/home/me/src" });
});
