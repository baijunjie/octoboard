// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";

import { ProjectFilterButton } from "./ProjectFilter";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("clears keyword and tags through the active heading button without opening a popover", () => {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  function Filter() {
    const [filter, setFilter] = useState({ keyword: "site", tags: ["frontend"] });
    return <ProjectFilterButton filter={filter} vocabulary={["frontend"]} onChange={setFilter} />;
  }
  try {
    act(() => root.render(<Filter />));
    const clear = host.querySelector<HTMLButtonElement>("button")!;
    expect(clear.getAttribute("aria-label")).toBe("Clear filter");
    expect(clear.getAttribute("aria-haspopup")).toBe("false");
    clear.focus();
    act(() => clear.click());
    expect(host.querySelector("button")?.getAttribute("aria-label")).toBe("Filter projects");
    expect(host.querySelector("button")).toBe(clear);
    expect(document.activeElement).toBe(clear);
    expect(document.querySelector("[role=dialog]")).toBeNull();
  } finally {
    act(() => root.unmount());
    host.remove();
  }
});
