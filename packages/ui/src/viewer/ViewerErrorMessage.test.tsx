// @vitest-environment jsdom
import { act, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished } from "vitest";

import { DaemonErrorMessage } from "../components/DaemonErrorMessage";
import { DaemonRequestError } from "../daemon-client";
import type { ErrorPresentation } from "../errorPresentation";
import { format, globalParam, type Translate } from "../i18n/catalog";
import { setLanguage } from "../i18n/language";
import type { Language } from "../i18n/languages";
import { ViewerErrorMessage } from "./ViewerErrorMessage";
import { rawFailureText } from "../errorText";
import { viewerErrorText } from "./viewerError";
import { ViewerHost } from "./viewerTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => act(() => setLanguage("en")));

function renderContent(content: ReactNode): HTMLDivElement {
  const container = document.createElement("div");
  const root = createRoot(container);
  onTestFinished(() => act(() => root.unmount()));
  act(() => root.render(<ViewerHost>{content}</ViewerHost>));
  return container;
}

function render(error: ErrorPresentation, path = "src/gone.ts"): HTMLDivElement {
  return renderContent(<ViewerErrorMessage error={error} path={path} />);
}

it.each<Language>(["en", "zh-Hans"])("omits only the current file's path in %s", (language) => {
  setLanguage(language);
  for (const [code, key] of [
    ["invalid_path", "viewer.error.invalidPath"],
    ["file_not_found", "viewer.error.fileNotFound"],
    ["outside_scope", "viewer.error.outsideScope"],
    ["unsupported_file_type", "viewer.error.unsupportedFileType"],
    ["source_changed", "viewer.error.sourceChanged"],
  ] as const) {
    const error = new DaemonRequestError("fallback", code, { path: "src/gone.ts" });
    expect(render(error).textContent).toBe(format(language, key));
    expect(render(error).querySelector("code")).toBeNull();
    expect(error.params.path).toBe("src/gone.ts");
  }
});

it("keeps the special explanation for an unmerged file", () => {
  const error = new DaemonRequestError("fallback", "unsupported_file_type", { path: "src/gone.ts", file_type: "unmerged" });
  expect(render(error).textContent).toBe(format("en", "viewer.error.unmerged"));
});

it.each<Language>(["en", "zh-Hans"])("retains permission diagnostics and the product name treatment in %s", (language) => {
  setLanguage(language);
  const permission = render(new DaemonRequestError("fallback", "permission_denied", { path: "src/gone.ts", detail: "Access denied" }));
  // The system's text is the failure's Details, not part of the summary line.
  expect(permission.textContent).not.toContain("Access denied");
  expect(permission.textContent).not.toContain("src/gone.ts");
  expect(permission.querySelector("[dir=auto]")?.textContent).toBe(globalParam("appName"));
});

it("compares the displayed wire path without decoding a displayed name twice", () => {
  expect(render(new DaemonRequestError("fallback", "file_not_found", { path: "src/a b.ts" }), "src/a%20b.ts").textContent)
    .toBe(format("en", "viewer.error.fileNotFound"));
  expect(render(new DaemonRequestError("fallback", "file_not_found", { path: "src/a%20b.ts" }), "src/a%2520b.ts").textContent)
    .toBe(format("en", "viewer.error.fileNotFound"));
  expect(render(new DaemonRequestError("fallback", "file_not_found", { path: "src/a b.ts" }), "src/a%2520b.ts").textContent)
    .toContain("src/a b.ts");
});

it("retains paths that name another file or an unavailable parent folder", () => {
  const other = render(new DaemonRequestError("fallback", "file_not_found", { path: "src/other.ts" }));
  expect(other.querySelector("code")?.textContent).toBe("src/other.ts");
  const parent = render(new DaemonRequestError("fallback", "source_unavailable", { path: "src", detail: "Permission denied" }));
  expect(parent.textContent).toContain("src");
  expect(parent.textContent).not.toContain("Permission denied");
});

it("leaves plain, unknown and incomplete failures intact", () => {
  expect(render("Read failed: src/gone.ts").textContent).toBe("Read failed: src/gone.ts");
  for (const [code, params] of [
    ["new_failure", { path: "src/gone.ts" }],
    ["file_not_found", {}],
    ["permission_denied", { path: "src/gone.ts" }],
    ["clean_filtered", { path: "src/gone.ts" }],
  ] as const) {
    expect(render(new DaemonRequestError("Read failed", code, params)).textContent).toBe(
      renderContent(<DaemonErrorMessage error={new DaemonRequestError("Read failed", code, params)} part="summary" />).textContent,
    );
  }
});

it("does not shorten shared errors outside the viewer", () => {
  const error = new DaemonRequestError("fallback", "file_not_found", { path: "src/gone.ts" });
  const container = renderContent(<DaemonErrorMessage error={error} />);
  expect(container.querySelector("code")?.textContent).toBe("src/gone.ts");
});

const NO_RECORDS = { consoles: new Map(), projects: new Map(), sessions: new Map() };

it.each<Language>(["en", "zh-Hans"])("uses the same compact reasons in plain-text body failures in %s", (language) => {
  const t: Translate = (key, ...args) => format(language, key, args[0]);
  const text = (error: ErrorPresentation) => viewerErrorText(t, language, error, "src/gone.ts", NO_RECORDS);
  expect(text(new DaemonRequestError("gone", "file_not_found", { path: "src/gone.ts" })))
    .toBe(format(language, "viewer.error.fileNotFound"));
  const denied = text(new DaemonRequestError("denied", "permission_denied", { path: "src/gone.ts", detail: "Access denied" }));
  // The system's own reason is raw text and stays out of the visible line.
  expect(denied).toBe(format(language, "viewer.error.permissionDenied.summary"));
  // Another file's failure is worded in the language, with its path.
  expect(text(new DaemonRequestError("other.ts was not found", "file_not_found", { path: "other.ts" }))).toContain("other.ts");
  expect(text("Read failed")).toBe("Read failed");
});

it.each([
  new DaemonRequestError("database is locked", "internal_error", { detail: "database is locked" }),
  new DaemonRequestError("a newer daemon's text", "a_newer_code", {}),
])("says a body failure the application has no words for as its generic sentence, its text kept as the raw detail: $code", (error) => {
  const t: Translate = (key, ...args) => format("en", key, args[0]);
  expect(viewerErrorText(t, "en", error, "src/gone.ts", NO_RECORDS)).toBe("Something unexpected went wrong.");
  expect(rawFailureText(error)).toBe(error.message);
});

it.each<Language>(["en", "zh-Hans"])("words a failure of another path as its summary sentence, without the raw reason, in %s", (language) => {
  const t: Translate = (key, ...args) => format(language, key, args[0]);
  const text = (error: ErrorPresentation) => viewerErrorText(t, language, error, "src/gone.ts", NO_RECORDS);
  const parent = text(new DaemonRequestError("raw", "source_unavailable", { path: "src", detail: "Permission denied" }));
  expect(parent).toContain("src");
  expect(parent).not.toContain("Permission denied");
  expect(parent).not.toContain("{");
  const other = text(new DaemonRequestError("raw", "permission_denied", { path: "other.ts", detail: "Access denied" }));
  expect(other).toContain("other.ts");
  expect(other).not.toContain("Access denied");
  // With no reason to leave out, the full sentence stands.
  expect(text(new DaemonRequestError("raw", "file_not_found", { path: "other.ts" }))).toContain("other.ts");
});
