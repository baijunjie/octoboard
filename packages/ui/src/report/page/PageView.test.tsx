// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import type { PageContent } from "../../protocol";
import { BROWSER_HOST } from "../../test/hosts";
import { click, type } from "../../test/dom";
import { LAZY_IMPORT_TIMEOUT_MS, settleLazyImport } from "../../test/lazyImport";
import { renderPage } from "./pageTestSupport";

vi.mock("../../preferences/theme", () => ({ useOctoboardTheme: () => ({ resolved: "light" }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const render = renderPage;

const button = (host: ParentNode, name: string) => [...host.querySelectorAll("button")].find((candidate) => candidate.textContent === name)!;
const field = (host: ParentNode, label: string) => [...host.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>("input, textarea")].find((el) => el.labels?.[0]?.textContent === label)!;
const checkbox = (host: ParentNode, label: string) => [...host.querySelectorAll<HTMLInputElement>("input[type=checkbox]")].find((el) => el.closest("label")?.textContent === label)!;
/** Waits for the Markdown renderer, which loads on first use, to have drawn what `selector` names. */
const markdownDrawn = (host: ParentNode, selector: string) => vi.waitFor(async () => {
  await settleLazyImport();
  expect(host.querySelector(selector)).not.toBeNull();
}, { timeout: LAZY_IMPORT_TIMEOUT_MS });
const field_error = (host: ParentNode, name: string) => host.querySelector(`[data-page-field="${name}"] [role=alert]`)?.textContent;

const EVERY_KIND: PageContent = {
  title: "Release survey",
  body: [
    { type: "markdown", text: "Some **bold** words" },
    {
      type: "section", heading: "Details", children: [
        { type: "text", name: "name", label: "Name", placeholder: "Ada", description: "Your full name" },
        { type: "textarea", name: "story", label: "Story", rows: 3 },
        { type: "number", name: "count", label: "Count" },
        { type: "date", name: "day", label: "Day" },
        { type: "time", name: "at", label: "At" },
        { type: "datetime", name: "when", label: "When" },
        { type: "radio", name: "size", label: "Size", options: [{ value: "s", label: "Small" }, { value: "l", label: "Large" }], default: "s" },
        { type: "select", name: "color", label: "Color", options: [{ value: "r", label: "Red" }, { value: "g" }] },
        { type: "checkboxes", name: "tags", label: "Tags", options: [{ value: "a", label: "Alpha" }, { value: "b", label: "Beta" }], default: ["b"] },
        { type: "toggle", name: "urgent", label: "Urgent", description: "Jump the queue" },
        { type: "slider", name: "level", label: "Level", min: 0, max: 10, step: 2, default: 4 },
      ],
    },
    { type: "choices", name: "verdict", label: "Verdict", options: [{ value: "yes", label: "Yes" }, { value: "no" }] },
    { type: "submit" },
  ],
};

it("draws every kind of component with the application's own controls and no light surface of its own", async () => {
  const { host } = await render(EVERY_KIND);
  await markdownDrawn(host, "strong");
  expect(host.querySelector("h3")?.textContent).toBe("Release survey");
  expect(host.querySelector("h4")?.textContent).toBe("Details");
  expect(host.querySelector("strong")?.textContent).toBe("bold");
  expect(field(host, "Name").placeholder).toBe("Ada");
  expect(host.textContent).toContain("Your full name");
  expect(field(host, "Story").tagName).toBe("TEXTAREA");
  expect((field(host, "Story") as HTMLTextAreaElement).rows).toBe(3);
  expect(field(host, "Count").getAttribute("inputmode")).toBe("decimal");
  expect(field(host, "Day").type).toBe("date");
  expect(field(host, "At").type).toBe("time");
  expect(field(host, "When").type).toBe("datetime-local");
  expect(host.querySelector('[role="radiogroup"]')?.textContent).toContain("Small");
  expect((host.querySelector('input[type=radio][value="s"]') as HTMLInputElement).checked).toBe(true);
  expect(host.querySelector('[data-page-field="color"] button')?.textContent).toContain("Choose an option");
  expect(host.querySelector('[role="group"]')?.textContent).toContain("Alpha");
  expect(checkbox(host, "Beta").checked).toBe(true);
  expect(checkbox(host, "Alpha").checked).toBe(false);
  expect(host.querySelector('[role="switch"]')?.getAttribute("aria-label")).toBe("Urgent");
  expect(host.querySelector('[role="slider"], input[type=range]')).not.toBeNull();
  expect(button(host, "Submit")).toBeDefined();
  expect(button(host, "Yes")).toBeDefined();
  expect(button(host, "no")).toBeDefined();
  // The surface is the panel's, in both appearances: nothing here paints its own.
  expect(host.querySelector(".bg-white, [style*=background]")).toBeNull();
});

it("shows a component only while its visible_when holds, and sends nothing of a hidden field", async () => {
  const { host, onSubmit } = await render({
    body: [
      { type: "radio", name: "kind", label: "Kind", options: [{ value: "bug" }, { value: "idea" }] },
      { type: "text", name: "steps", label: "Steps", visible_when: { field: "kind", equals: "bug" } },
      { type: "submit" },
    ],
  });
  expect(field(host, "Steps")).toBeUndefined();
  await click(host.querySelector('input[type=radio][value="bug"]'));
  type(field(host, "Steps"), "open the app");
  await click(host.querySelector('input[type=radio][value="idea"]'));
  expect(field(host, "Steps")).toBeUndefined();
  await click(button(host, "Submit"));
  expect(onSubmit).toHaveBeenCalledExactlyOnceWith({ kind: "idea" });
});

it("refuses a submit with a wrong field, says what is wrong under it, focuses it and sends nothing; fixing it clears the message", async () => {
  const { host, onSubmit } = await render({
    body: [
      { type: "text", name: "email", label: "Email", required: true, pattern: ".+@.+" },
      { type: "number", name: "age", label: "Age", min: 18 },
      { type: "checkboxes", name: "tags", label: "Tags", options: [{ value: "a" }, { value: "b" }], required: true },
      { type: "select", name: "pick", label: "Pick", options: [{ value: "a" }], required: true },
      { type: "radio", name: "r", label: "R", options: [{ value: "a" }], required: true },
      { type: "submit" },
    ],
  });
  await click(button(host, "Submit"));
  expect(onSubmit).not.toHaveBeenCalled();
  expect(field_error(host, "email")).toBe("Enter a value.");
  expect(field_error(host, "tags")).toBe("Choose at least one option.");
  expect(field_error(host, "pick")).toBe("Choose an option.");
  expect(field_error(host, "r")).toBe("Choose an option.");
  expect(document.activeElement).toBe(field(host, "Email"));

  // The pattern is tested when the field is left, not as it is typed into.
  type(field(host, "Email"), "nope");
  expect(field_error(host, "email")).toBeUndefined();
  await act(async () => field(host, "Email").dispatchEvent(new FocusEvent("focusout", { bubbles: true })));
  expect(field_error(host, "email")).toBe("The value does not match the format this field asks for.");
  type(field(host, "Email"), "a@b.c");
  expect(field_error(host, "email")).toBeUndefined();
  type(field(host, "Age"), "17");
  expect(field_error(host, "age")).toBe("Enter \u206818\u2069 or more.");
});

it("sends the values as strings, numbers, a boolean and a selection in option order, with the pressed button's name and value", async () => {
  const { host, onSubmit } = await render({
    body: [
      { type: "text", name: "name", label: "Name" },
      { type: "number", name: "count", label: "Count" },
      { type: "toggle", name: "urgent", label: "Urgent" },
      { type: "checkboxes", name: "tags", label: "Tags", options: [{ value: "a", label: "Alpha" }, { value: "b", label: "Beta" }] },
      { type: "text", name: "skipped", label: "Skipped" },
      { type: "choices", name: "verdict", options: [{ value: "yes", label: "Yes" }, { value: "no", label: "No" }] },
    ],
  });
  type(field(host, "Name"), "Ada");
  type(field(host, "Count"), "3");
  await click(host.querySelector('[role="switch"] input, input[role="switch"]'));
  await click(checkbox(host, "Beta"));
  await click(checkbox(host, "Alpha"));
  await click(button(host, "No"));
  expect(onSubmit).toHaveBeenCalledExactlyOnceWith({ name: "Ada", count: 3, urgent: true, tags: ["a", "b"], verdict: "no" });
});

it("submits on Enter in a single-line field through the first submit button, with its name and value", async () => {
  const { host, onSubmit } = await render({
    body: [
      { type: "text", name: "name", label: "Name" },
      { type: "submit", label: "Approve", name: "action", value: "approve" },
      { type: "submit", label: "Reject", name: "action", value: "reject" },
    ],
  });
  type(field(host, "Name"), "Ada");
  await act(async () => void field(host, "Name").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })));
  expect(onSubmit).toHaveBeenCalledExactlyOnceWith({ name: "Ada", action: "approve" });
});

it("shows what a stored submission holds, read-only, whatever the user could have entered, with no way to submit again", async () => {
  const { host, onSubmit } = await render(
    {
      body: [
        { type: "text", name: "name", label: "Name", default: "Default" },
        { type: "checkboxes", name: "tags", label: "Tags", options: [{ value: "a", label: "Alpha" }, { value: "b", label: "Beta" }], default: ["a"] },
        { type: "choices", name: "verdict", options: [{ value: "yes", label: "Yes" }, { value: "no", label: "No" }] },
        { type: "submit" },
      ],
    },
    { submission: { name: "Ada", tags: ["b"], verdict: "no" } },
  );
  expect(field(host, "Name").value).toBe("Ada");
  expect(field(host, "Name").readOnly).toBe(true);
  expect(checkbox(host, "Beta").checked).toBe(true);
  expect(checkbox(host, "Alpha").checked).toBe(false);
  expect(button(host, "No").getAttribute("aria-current")).toBe("true");
  for (const name of ["Yes", "No", "Submit"]) expect(button(host, name).disabled).toBe(true);
  await click(button(host, "No"));
  expect(onSubmit).not.toHaveBeenCalled();
});

it("keeps a history page read-only: its fields hold their defaults and its buttons do nothing", async () => {
  const { host, onSubmit } = await render(
    { body: [{ type: "text", name: "name", label: "Name", default: "Ada" }, { type: "submit" }] },
    { history: true },
  );
  expect(field(host, "Name").readOnly).toBe(true);
  expect(button(host, "Submit").disabled).toBe(true);
  await click(button(host, "Submit"));
  expect(onSubmit).not.toHaveBeenCalled();
});

it("locks the form while a submission is out, keeping what was entered, with the pressed button pending and a second press doing nothing", async () => {
  const { host, onSubmit, rerender } = await render({
    body: [{ type: "text", name: "name", label: "Name" }, { type: "submit" }, { type: "choices", name: "verdict", options: [{ value: "yes", label: "Yes" }] }],
  });
  type(field(host, "Name"), "Ada");
  await click(button(host, "Submit"));
  expect(onSubmit).toHaveBeenCalledTimes(1);
  await rerender({ pending: true });
  expect(field(host, "Name").value).toBe("Ada");
  expect(field(host, "Name").readOnly).toBe(true);
  expect(button(host, "Submit").getAttribute("aria-busy")).toBe("true");
  expect(button(host, "Yes").disabled).toBe(true);
  await click(button(host, "Submit"));
  await click(button(host, "Yes"));
  expect(onSubmit).toHaveBeenCalledTimes(1);
  // A failure that certainly did not deliver unlocks it, with the value kept.
  await rerender({ pending: false });
  expect(field(host, "Name").readOnly).toBe(false);
  expect(field(host, "Name").value).toBe("Ada");
  await click(button(host, "Yes"));
  expect(onSubmit).toHaveBeenLastCalledWith({ name: "Ada", verdict: "yes" });
});

it("marks only the pressed one of several submit buttons with no name as pending", async () => {
  const { host, rerender } = await render({ body: [{ type: "submit", label: "First" }, { type: "submit", label: "Second" }] });
  await click(button(host, "Second"));
  await rerender({ pending: true });
  expect(button(host, "Second").getAttribute("aria-busy")).toBe("true");
  expect(button(host, "First").getAttribute("aria-busy")).not.toBe("true");
});

it("draws a locked text field, textarea and select as a value to read: a plain border and no fill", async () => {
  const { host } = await render({
    body: [
      { type: "text", name: "t", label: "T" },
      { type: "textarea", name: "a", label: "A" },
      { type: "select", name: "s", label: "S", options: [{ value: "x" }] },
    ],
  }, { history: true });
  for (const control of [field(host, "T"), field(host, "A"), host.querySelector("[data-page-field=s] button")!]) {
    expect(control.className).toContain("border-separator");
    expect(control.className).toContain("bg-transparent");
  }
});

const MARKDOWN_PAGE = (text: string): PageContent => ({ body: [{ type: "markdown", text }] });

it("does not render raw HTML in a Markdown block: it is shown as the text it was written as", async () => {
  const { host } = await render(MARKDOWN_PAGE('**Before** <img src="https://tracker.example/p.png" onerror="alert(1)"> <script>alert(2)</script> <b>bold</b> after\n\n<div>block</div>'));
  await markdownDrawn(host, "strong");
  expect(host.querySelector("img, script, b, [data-markdown-document] div, [onerror]")).toBeNull();
  expect(host.textContent).toContain('Before <img src="https://tracker.example/p.png" onerror="alert(1)"> <script>alert(2)</script> <b>bold</b> after');
  expect(host.textContent).toContain("<div>block</div>");
});

it("draws a Markdown image only from a data: address, so nothing else is ever requested", async () => {
  const dot = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
  const { host } = await render(MARKDOWN_PAGE(`![inline dot](${dot})\n\n![remote pic](https://tracker.example/p.png)\n\n![relative pic](docs/p.png)\n\n![scheme-less](//tracker.example/p.png)\n\n![js](javascript:alert(1))`));
  await markdownDrawn(host, "img");
  const images = [...host.querySelectorAll("img")];
  expect(images.map((image) => image.getAttribute("src"))).toEqual([dot]);
  expect(images[0].alt).toBe("inline dot");
  for (const alt of ["remote pic", "relative pic", "scheme-less", "js"]) expect(host.textContent).toContain(alt);
  expect(host.innerHTML).not.toContain("tracker.example");
});

it("opens an absolute web link in the system browser and draws a relative, project or heading link as plain text", async () => {
  const open = vi.fn().mockResolvedValue(undefined);
  const { host } = await render(MARKDOWN_PAGE("[docs](https://example.com/docs) and [local](src/main.rs) and [up](../secret) and [mail](mailto:a@example.com) and [jump](#top)"), {
    platform: { kind: "browser", host: BROWSER_HOST, links: { open } },
  });
  await markdownDrawn(host, "a");
  const anchors = [...host.querySelectorAll("a")];
  expect(anchors.map((anchor) => anchor.textContent)).toEqual(["docs", "mail"]);
  expect(host.textContent).toContain("local");
  expect(host.textContent).toContain("up");
  expect(host.textContent).toContain("jump");
  await click(anchors[0]);
  expect(open).toHaveBeenCalledExactlyOnceWith("https://example.com/docs");
});

it("draws its fields in HeroUI's default field variant, as controls on the pane's own background", async () => {
  const { host } = await render({
    body: [
      { type: "text", name: "a", label: "A" },
      { type: "textarea", name: "b", label: "B" },
      { type: "number", name: "c", label: "C" },
      { type: "select", name: "d", label: "D", options: [{ value: "x" }] },
    ],
  });
  for (const selector of [".input--primary", ".textarea--primary", ".select--primary"]) expect(host.querySelector(selector)).not.toBeNull();
  expect(host.querySelector('[class*="--secondary"]')).toBeNull();
});

it("marks a required text field, textarea and radio group as required for assistive technology, with no mark drawn", async () => {
  const { host } = await render({
    body: [
      { type: "text", name: "a", label: "A", required: true },
      { type: "textarea", name: "b", label: "B", required: true },
      { type: "number", name: "c", label: "C" },
      { type: "radio", name: "d", label: "D", required: true, options: [{ value: "x" }] },
    ],
  });
  expect(field(host, "A").getAttribute("aria-required")).toBe("true");
  expect(field(host, "B").getAttribute("aria-required")).toBe("true");
  expect(field(host, "C").hasAttribute("aria-required")).toBe(false);
  expect(host.querySelector('[role="radiogroup"]')?.getAttribute("aria-required")).toBe("true");
  expect(host.textContent).not.toContain("*");
  // HeroUI draws an asterisk after the label of a field marked `data-required`, which an `aria-required` alone avoids.
  expect(host.querySelector('[data-slot="textfield"][data-required="true"]')).toBeNull();
});

it("gives what is drawn for a person's text the direction of that text", async () => {
  const { host } = await render({
    title: "T",
    body: [
      { type: "text", name: "a", label: "A", description: "D" },
      { type: "textarea", name: "b", label: "B" },
      { type: "radio", name: "r", label: "R", options: [{ value: "x", label: "X" }] },
      { type: "toggle", name: "t", label: "T2" },
      { type: "choices", name: "c", options: [{ value: "y" }] },
    ],
  });
  expect(field(host, "A").getAttribute("dir")).toBe("auto");
  expect(field(host, "B").getAttribute("dir")).toBe("auto");
  for (const label of ["A", "B", "R", "X", "T2"]) {
    const element = [...host.querySelectorAll("label, span")].reverse().find((candidate) => candidate.textContent === label)!;
    expect(element.getAttribute("dir") ?? element.closest("[dir]")?.getAttribute("dir")).toBe("auto");
  }
  expect(host.querySelector("h3")?.getAttribute("dir")).toBe("auto");
  expect(button(host, "y").querySelector("[dir=auto]")).not.toBeNull();
});

it("keeps the outline in order: the title is an h3, a section's heading an h4, a callout's title one below its context", async () => {
  const { host } = await render({
    title: "Page",
    body: [
      { type: "callout", title: "Top", text: "# From Markdown" },
      { type: "section", heading: "Part", children: [{ type: "callout", title: "Inner", text: "x" }, { type: "markdown", text: "# In section" }] },
      { type: "section", children: [{ type: "markdown", text: "# Without heading" }] },
    ],
  });
  await markdownDrawn(host, "h5, h6");
  const headings = [...host.querySelectorAll("h1, h2, h3, h4, h5, h6")].map((heading) => `${heading.tagName.toLowerCase()} ${heading.textContent}`);
  expect(headings).toEqual(["h3 Page", "h4 Top", "h5 From Markdown", "h4 Part", "h5 Inner", "h5 In section", "h4 Without heading"]);
});

it("leaves no wrapper for a column whose content is hidden, and the columns left share the width", async () => {
  const { host } = await render({
    body: [
      { type: "toggle", name: "more", label: "More" },
      { type: "columns", children: [{ type: "text", name: "a", label: "A" }, { type: "text", name: "b", label: "B", visible_when: { field: "more", equals: true } }] },
    ],
  });
  const grid = host.querySelector("[data-page-field=a]")!.parentElement!.parentElement!;
  expect(grid.children).toHaveLength(1);
  expect(grid.className).not.toContain("grid-cols-2");
  await click(host.querySelector('input[role="switch"]'));
  expect(grid.children).toHaveLength(2);
});

it("does not run a pattern on every keystroke: it is tested when the field is left, and the error goes as the value is edited", async () => {
  const { host } = await render({ body: [{ type: "text", name: "e", label: "E", pattern: "\\d+" }] });
  type(field(host, "E"), "abc");
  expect(field_error(host, "e")).toBeUndefined();
  await act(async () => field(host, "E").dispatchEvent(new FocusEvent("focusout", { bubbles: true })));
  expect(field_error(host, "e")).toBe("The value does not match the format this field asks for.");
  type(field(host, "E"), "abcd");
  expect(field_error(host, "e")).toBeUndefined();
});

it("shows block and inline raw HTML of a Markdown block as text, around the Markdown it has between the tags, and leaves code as it was", async () => {
  const { host } = await render(MARKDOWN_PAGE("Before <details><summary>Sum</summary>inner **bold**</details> after `<b>code</b>`\n\n```html\n<p>fence</p>\n```"));
  await markdownDrawn(host, "strong");
  expect(host.querySelector("details, summary, p > b, [data-markdown-document] p > p")).toBeNull();
  expect(host.textContent).toContain("<details><summary>Sum</summary>inner ");
  expect(host.querySelector("strong")?.textContent).toBe("bold");
  expect(host.textContent).toContain("</details> after ");
  expect(host.querySelector("code")?.textContent).toBe("<b>code</b>");
  expect(host.querySelector("pre")?.textContent).toContain("<p>fence</p>");
});

it("draws a link to a heading of a Markdown block as plain text that is not a link", async () => {
  const { host } = await render(MARKDOWN_PAGE("# Top\n\nBack to [the top](#top) or [elsewhere](#nowhere)."));
  await markdownDrawn(host, "h3");
  expect(host.querySelector("a")).toBeNull();
  expect(host.textContent).toContain("Back to the top or elsewhere.");
});

it("starts an untitled page's headings under the panel's own: a section's heading, a callout's title and a Markdown heading are an h3, one below the panel's h2", async () => {
  const { host } = await render({
    body: [
      { type: "callout", title: "Top", text: "# From Markdown" },
      { type: "section", heading: "Part", children: [{ type: "callout", title: "Inner", text: "x" }, { type: "markdown", text: "# In section" }] },
      { type: "section", children: [{ type: "markdown", text: "# Without heading" }] },
    ],
  });
  await markdownDrawn(host, "h4");
  const headings = [...host.querySelectorAll("h1, h2, h3, h4, h5, h6")].map((heading) => `${heading.tagName.toLowerCase()} ${heading.textContent}`);
  expect(headings).toEqual(["h3 Top", "h4 From Markdown", "h3 Part", "h4 Inner", "h4 In section", "h3 Without heading"]);
});
