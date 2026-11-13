import { expect, it } from "vitest";

import { writeStartingPreferences } from "./preferences";

it("writes the theme, the language and each preference under the keys and in the form their owners read", () => {
  const written = new Map<string, string>();
  writeStartingPreferences(
    { setItem: (key, value) => void written.set(key, value) },
    {
      language: "zh-Hans",
      theme: "dark",
      preferences: { sidebarVisible: false, sidebarWidth: 280, markdownStyle: "github", panelStacks: { project: { open: ["files"], shares: { files: 1 } } } },
    },
  );
  expect(Object.fromEntries(written)).toEqual({
    "heroui-theme": "dark",
    "octoboard.language": "zh-Hans",
    "octoboard.sidebarVisible": "false",
    "octoboard.sidebarWidth": "280",
    "octoboard.markdownStyle": "github",
    "octoboard.panelStacks": JSON.stringify({ project: { open: ["files"], shares: { files: 1 } } }),
  });
});

it("writes only the theme and the language when the page has no preferences", () => {
  const written: string[] = [];
  writeStartingPreferences({ setItem: (key) => void written.push(key) }, { language: "en", theme: "light" });
  expect(written).toEqual(["heroui-theme", "octoboard.language"]);
});

it("skips a preference left undefined", () => {
  const written: string[] = [];
  writeStartingPreferences({ setItem: (key) => void written.push(key) }, { language: "en", theme: "light", preferences: { sidebarWidth: undefined } });
  expect(written).toEqual(["heroui-theme", "octoboard.language"]);
});

it("writes the desktop's automatic update check as the string its owner reads, off only when false", () => {
  const written = new Map<string, string>();
  writeStartingPreferences({ setItem: (key, value) => void written.set(key, value) }, { language: "en", theme: "light", preferences: { updateAutoCheck: false } });
  expect(written.get("octoboard.updateAutoCheck")).toBe("false");
});
