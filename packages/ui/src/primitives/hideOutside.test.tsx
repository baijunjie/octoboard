// @vitest-environment jsdom
import { expect, it } from "vitest";

import { clearTopLayer, closestTopLayer, liveAnnouncerProps, topLayerProps } from "./hideOutside";
import { mount } from "./testSupport";

it("marks, finds and unmarks a top layer", () => {
  const { container } = mount(
    <div {...topLayerProps(true)} data-testid="layer"><span data-testid="inner" /></div>,
  );
  const layer = container.querySelector("[data-testid=layer]")!;
  expect(layer.getAttribute("data-react-aria-top-layer")).toBe("true");
  expect(closestTopLayer(container.querySelector("[data-testid=inner]"))).toBe(layer);
  clearTopLayer(layer);
  expect(closestTopLayer(layer)).toBeNull();
  expect(topLayerProps(false)).toEqual({});
  expect(topLayerProps(undefined)).toEqual({});
});

it("marks a live announcer", () => {
  const { container } = mount(<div {...liveAnnouncerProps()} />);
  expect(container.firstElementChild!.getAttribute("data-live-announcer")).toBe("true");
});
