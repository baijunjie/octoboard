// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";

import { ProjectFilterButton, ProjectFilterSummary } from "./ProjectFilter";

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


it("shows the keyword and categories as static tags without individual clipping or controls", () => {
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  try {
    act(() => root.render(<ProjectFilterSummary filter={{ keyword: "site", tags: ["frontend", "مراجعة"] }} />));
    const keyword = host.querySelector('[aria-label="Filter keyword"]')!;
    const categories = host.querySelector('[aria-label="Selected tags"]')!;
    const tags = [...host.querySelectorAll<HTMLElement>('[data-slot="tag"]')];
    expect(tags.map((tag) => tag.textContent)).toEqual(["site", "frontend", "مراجعة"]);
    expect(keyword.contains(tags[0])).toBe(true);
    expect(categories.contains(tags[1])).toBe(true);
    expect(tags[0].classList.contains("bg-default")).toBe(true);
    expect(tags.slice(1).every((tag) => tag.classList.contains("bg-accent-soft"))).toBe(true);
    expect(tags.every((tag) => tag.classList.contains("tag--sm") && tag.getAttribute("dir") === "auto")).toBe(true);
    expect(tags.every((tag) => !tag.querySelector(".overflow-hidden") && tag.tabIndex === -1 && !tag.hasAttribute("aria-disabled"))).toBe(true);
    expect(host.querySelector('button, [role="grid"], [role="row"], [aria-selected], [tabindex], [data-marquee-scope]')).toBeNull();
    expect(host.querySelectorAll(".overflow-hidden")).toHaveLength(1);
    expect(document.querySelector('[role="tooltip"]')).toBeNull();
  } finally {
    act(() => root.unmount());
    host.remove();
  }
});
