// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";

import type { PageContent } from "../../protocol";
import { click, type } from "../../test/dom";
import { buttonNamed, renderPage } from "./pageTestSupport";

vi.mock("../../preferences/theme", () => ({ useOctoboardTheme: () => ({ resolved: "light" }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const field = (host: ParentNode, label: string) => [...host.querySelectorAll<HTMLInputElement>("input")].find((el) => el.labels?.[0]?.textContent === label);
const tab = (host: ParentNode, label: string) => [...host.querySelectorAll<HTMLElement>('[role="tab"]')].find((el) => el.textContent === label)!;

describe("columns", () => {
  it("draws what each column holds, in order, and its fields are part of the form", async () => {
    const { host, onSubmit } = await renderPage({
      body: [
        {
          type: "columns", children: [
            { type: "text", name: "first", label: "First", default: "Ada" },
            { type: "text", name: "second", label: "Second", required: true },
          ],
        },
        { type: "submit" },
      ],
    });
    expect([...host.querySelectorAll("input")].map((el) => el.labels?.[0]?.textContent)).toEqual(["First", "Second"]);
    expect(field(host, "First")!.value).toBe("Ada");
    await click(buttonNamed(host, "Submit"));
    expect(onSubmit).not.toHaveBeenCalled();
    type(field(host, "Second")!, "Lovelace");
    await click(buttonNamed(host, "Submit"));
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith({ first: "Ada", second: "Lovelace" });
  });

  it("hides what is in the columns while the columns' own visible_when is unmet, and sends none of their fields", async () => {
    const page: PageContent = {
      body: [
        { type: "toggle", name: "more", label: "More" },
        { type: "columns", visible_when: { field: "more", equals: true }, children: [{ type: "text", name: "extra", label: "Extra", default: "x" }, { type: "text", name: "other", label: "Other" }] },
        { type: "submit" },
      ],
    };
    const { host, onSubmit } = await renderPage(page);
    expect(field(host, "Extra")).toBeUndefined();
    await click(buttonNamed(host, "Submit"));
    expect(onSubmit).toHaveBeenLastCalledWith({ more: false });
    await click(host.querySelector('input[role="switch"]'));
    expect(field(host, "Extra")!.value).toBe("x");
    await click(buttonNamed(host, "Submit"));
    expect(onSubmit).toHaveBeenLastCalledWith({ more: true, extra: "x" });
  });
});

describe("columns that come and go", () => {
  const CONTROLLED: PageContent = {
    body: [
      {
        type: "columns", children: [
          { type: "text", name: "a", label: "A", visible_when: { field: "show", equals: true } },
          { type: "toggle", name: "show", label: "Show A" },
          { type: "text", name: "c", label: "C" },
        ],
      },
    ],
  };

  it("keeps the control of another column, with its focus and value, when a column before it appears and goes", async () => {
    const { host } = await renderPage(CONTROLLED);
    const toggle = host.querySelector<HTMLElement>('input[role="switch"]')!;
    const last = field(host, "C")!;
    type(last, "kept");
    toggle.focus();
    await click(toggle);
    expect(field(host, "A")).toBeDefined();
    // The columns after the one that appeared are the same elements, not new ones put where they were.
    expect(host.querySelector('input[role="switch"]')).toBe(toggle);
    expect(field(host, "C")).toBe(last);
    expect(document.activeElement).toBe(toggle);
    await click(toggle);
    expect(field(host, "A")).toBeUndefined();
    expect(host.querySelector('input[role="switch"]')).toBe(toggle);
    expect(field(host, "C")).toBe(last);
    expect(last.value).toBe("kept");
    expect(document.activeElement).toBe(toggle);
  });

  it("keeps a typed value in a column across the hiding and showing of another", async () => {
    const { host } = await renderPage(CONTROLLED);
    await click(host.querySelector('input[role="switch"]'));
    type(field(host, "A")!, "typed");
    await click(host.querySelector('input[role="switch"]'));
    await click(host.querySelector('input[role="switch"]'));
    expect(field(host, "A")!.value).toBe("typed");
  });
});

describe("tabs", () => {
  const TABS: PageContent = {
    body: [{
      type: "tabs", tabs: [
        { label: "One", children: [{ type: "text", name: "a", label: "Alpha", default: "from one" }] },
        { label: "Two", children: [{ type: "text", name: "b", label: "Beta", default: "from two" }] },
      ],
    }, { type: "submit" }],
  };

  it("shows the first tab at the start and the children of a tab once it is selected", async () => {
    const { host } = await renderPage(TABS);
    expect([...host.querySelectorAll('[role="tab"]')].map((el) => el.textContent)).toEqual(["One", "Two"]);
    expect(tab(host, "One").getAttribute("aria-selected")).toBe("true");
    expect(field(host, "Alpha")).toBeDefined();
    expect(field(host, "Beta")).toBeUndefined();
    await click(tab(host, "Two"));
    expect(tab(host, "Two").getAttribute("aria-selected")).toBe("true");
    expect(field(host, "Beta")!.value).toBe("from two");
    expect(field(host, "Alpha")).toBeUndefined();
  });

  it("submits the fields of every tab, the ones in a tab never shown included", async () => {
    const { host, onSubmit } = await renderPage(TABS);
    await click(buttonNamed(host, "Submit"));
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith({ a: "from one", b: "from two" });
  });

  it("keeps what was typed in a tab while another is shown", async () => {
    const { host, onSubmit } = await renderPage(TABS);
    type(field(host, "Alpha")!, "edited");
    await click(tab(host, "Two"));
    await click(tab(host, "One"));
    expect(field(host, "Alpha")!.value).toBe("edited");
    await click(buttonNamed(host, "Submit"));
    expect(onSubmit).toHaveBeenCalledExactlyOnceWith({ a: "edited", b: "from two" });
  });

  it("refuses a submit while a required field of a tab not shown is empty", async () => {
    const { host, onSubmit } = await renderPage({
      body: [{
        type: "tabs", tabs: [
          { label: "One", children: [{ type: "key_values", items: [{ key: "Nothing", value: "to fill in" }] }] },
          { label: "Two", children: [{ type: "text", name: "needed", label: "Needed", required: true }] },
        ],
      }, { type: "submit" }],
    });
    await click(buttonNamed(host, "Submit"));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("hides the whole tab set, and sends none of its fields, while the tabs' own visible_when is unmet", async () => {
    const { host, onSubmit } = await renderPage({
      body: [
        { type: "toggle", name: "show", label: "Show" },
        { type: "tabs", visible_when: { field: "show", equals: true }, tabs: [{ label: "One", children: [{ type: "text", name: "a", label: "Alpha", default: "x" }] }] },
        { type: "submit" },
      ],
    });
    expect(host.querySelector('[role="tab"]')).toBeNull();
    await click(buttonNamed(host, "Submit"));
    expect(onSubmit).toHaveBeenLastCalledWith({ show: false });
    await click(host.querySelector('input[role="switch"]'));
    expect(host.querySelector('[role="tab"]')).not.toBeNull();
    await click(buttonNamed(host, "Submit"));
    expect(onSubmit).toHaveBeenLastCalledWith({ show: true, a: "x" });
  });
});

describe("divider", () => {
  it("is a separator, with no name when it has no label", async () => {
    const { host } = await renderPage({ body: [{ type: "markdown", text: "above" }, { type: "divider" }, { type: "markdown", text: "below" }] });
    const separators = host.querySelectorAll('hr, [role="separator"]');
    expect(separators).toHaveLength(1);
    expect(separators[0]!.tagName).toBe("HR");
    expect(separators[0]!.hasAttribute("aria-label")).toBe(false);
  });

  it("shows its label on the rule and names the separator by it", async () => {
    const { host } = await renderPage({ body: [{ type: "divider", label: "Details" }] });
    const separator = host.querySelector<HTMLElement>('[role="separator"]')!;
    expect(separator.getAttribute("aria-label")).toBe("Details");
    expect(separator.textContent).toBe("Details");
  });

  it("is hidden while its visible_when is unmet", async () => {
    const { host } = await renderPage({
      body: [
        { type: "toggle", name: "more", label: "More" },
        { type: "divider", label: "Extra", visible_when: { field: "more", equals: true } },
      ],
    });
    expect(host.querySelector('[role="separator"]')).toBeNull();
    await click(host.querySelector('input[role="switch"]'));
    expect(host.querySelector('[role="separator"]')).not.toBeNull();
  });
});
