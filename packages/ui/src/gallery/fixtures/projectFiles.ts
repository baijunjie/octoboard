// The files the fixture daemon serves for a project's browser (`fixtureDaemon.ts`), and the
// gallery's "Project files" scenarios over them: the tree, the viewer opened from it, and the states
// a listing or a read can end in.
import type { MessageParams } from "../../protocol";
import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import { SAMPLE, snapshotState } from "./builders";
import { IMAGES } from "./viewerImages";

const GROUP = "Project pane: files";

/** The daemon error a request for a fixture path is answered with. */
export interface FixtureError {
  code: string;
  params: MessageParams;
  message: string;
}

/** One path of a fixture project: a directory (cut short when `complete` is false), a file's text or
 * an image's bytes; with `error`, what reading it (listing it, for a directory) is answered with. */
export type FixtureFile =
  | { dir: true; complete?: boolean; error?: FixtureError }
  | { text: string; error?: FixtureError }
  | { image: { mediaType: string; data: string }; error?: FixtureError };

/** A project's files by wire path; a directory lists the paths directly under it. */
export type FixtureFiles = Record<string, FixtureFile>;

const README = `# Search API

The service behind the search box: it indexes documents and answers queries.

## Running it

\`\`\`sh
pnpm install
pnpm dev
\`\`\`
`;

const MAIN = `import { createServer } from "./server";
import { loadConfig } from "./config";

const config = loadConfig(process.env);
const server = createServer(config);

server.listen(config.port, () => {
  console.log(\`listening on \${config.port}\`);
});
`;

/** The project every gallery project shows unless its scenario gives other files. */
export const SAMPLE_FILES: FixtureFiles = {
  "": { dir: true },
  docs: { dir: true },
  "docs/architecture.md": { text: "# Architecture\n\nRequests go through the router to the index.\n" },
  "docs/logo.png": { image: { mediaType: "image/png", data: IMAGES.before_png.data } },
  src: { dir: true },
  "src/components": { dir: true },
  "src/components/SearchBox.tsx": { text: 'export function SearchBox() {\n  return <input type="search" />;\n}\n' },
  "src/config.ts": { text: "export function loadConfig(env: NodeJS.ProcessEnv) {\n  return { port: Number(env.PORT ?? 8080) };\n}\n" },
  "src/main.ts": { text: MAIN },
  "src/server.ts": { text: 'import http from "node:http";\n\nexport const createServer = () => http.createServer();\n' },
  "item 2.txt": { text: "Natural order puts this before item 10.\n" },
  "item 10.txt": { text: "Natural order puts this after item 2.\n" },
  ".gitignore": { text: "node_modules\ndist\n" },
  "package.json": { text: '{\n  "name": "search-api",\n  "private": true\n}\n' },
  "README.md": { text: README },
};

const unreadableDir = (path: string): FixtureFile => ({
  dir: true,
  error: {
    code: "permission_denied",
    params: { path, detail: "Permission denied (os error 13)" },
    message: `Permission was denied reading ${path}: Permission denied (os error 13)`,
  },
});

const { console: console_, web, api, sessions } = SAMPLE;
const state = snapshotState({ consoles: [console_], projects: [web, api], sessions });
const openFiles = [(ui: Ui) => ui.press(ui.session("Fix the summary layout")), (ui: Ui) => ui.wait(400)];
const filterFiles: FixtureFiles = {
  ...SAMPLE_FILES,
  "src/services": { dir: true },
  "src/services/search": { dir: true },
  "src/services/search/server.ts": { text: "export const searchPort = 8081;\n" },
};

export const projectFilesScenarios: Scenario[] = [
  {
    id: "files-viewer-error-toast", group: "File viewer", category: "exceptional",
    title: "File viewer with an error toast",
    description: "An unrelated path failure is deliberately raised over the viewer, its path set as code, to confirm that toast placement leaves Previous / Next accessible.",
    state,
    toasts: [{ kind: "error", code: "path_not_found", params: { path: "/Users/dev/code/missing" }, message: "" }],
    steps: [...openFiles, (ui) => ui.press("src"), (ui) => ui.press("main.ts"), (ui) => ui.wait(600)],
  },
  {
    id: "files-tree",
    group: GROUP,
    category: "normal",
    title: "A project session's files",
    description:
      "Folders first, natural order, .git left out. Expand or collapse a folder to watch the following rows slide into place; scrolling keeps rows steady. src is expanded and main.ts is selected: its rounded highlight has space on all four sides, matching Git rows. Compare its actions button with the selected sidebar session and the console header: resting glyphs stay muted, while hovering or opening each menu gives it the shared subtle tint. Press a file for the viewer and Left/Right to visit visible files. Right-click file and folder rows, or use their actions button, for Open, Show in file manager and Copy path. Switch between Website and Search API sessions to inspect the short file-pane entrance; Search API has its own routes and schema tree. Switching between the two Website sessions keeps the current tree steady. A project's actions offer Browse files when its pane is hidden or belongs to another project. Open in file manager is available independently. Narrow the window and open the project pane drawer to see the same tree over the terminal.",
    state,
    filesByProject: {
      [api.id]: {
        "": { dir: true },
        routes: { dir: true },
        "routes/search.ts": { text: "export const search = (query: string) => ({ query, results: [] });\n" },
        schema: { dir: true },
        "schema/documents.sql": { text: "CREATE TABLE documents (id INTEGER PRIMARY KEY, title TEXT);\n" },
        "api.config.json": { text: '{ "port": 8080 }\n' },
      },
    },
    steps: [...openFiles, (ui) => ui.press("src"), (ui) => ui.press("main.ts"), (ui) => ui.wait(600), (ui) => ui.key("Escape")],
  },
  {
    id: "files-filter",
    group: GROUP,
    category: "normal",
    title: "Matching files throughout the project",
    description: "The filter sits above matching files grouped by folder. Both server.ts files match the trimmed, case-insensitive filename query without opening any folders first. The project title, compact pill tabs and right-aligned filter, layout and Refresh controls share one header row. Each tab keeps its own query, layout and folded folders.",
    state,
    files: filterFiles,
    steps: [
      ...openFiles,
      (ui) => ui.press(ui.t("browser.filter.show")),
      (ui) => ui.type(" SERVER "),
      (ui) => ui.wait(400),
    ],
  },
  {
    id: "files-flat",
    group: GROUP,
    category: "normal",
    title: "All project files as a flat list",
    description: "The flat layout lists files throughout the project without opening folders or entering a query. Relative paths distinguish files in different folders. The layout toggle occupies the same header position as in Uncommitted and Compare.",
    state,
    files: filterFiles,
    steps: [
      ...openFiles,
      (ui) => ui.press(ui.t("git.layout.flat")),
      (ui) => ui.wait(400),
    ],
  },
  {
    id: "files-filter-empty",
    group: GROUP,
    category: "normal",
    title: "No matching files in the project",
    description: "The whole project has no files matching the query, so the active filter shows a no-match empty state. Hiding the filter clears the query and restores the folder tree.",
    state,
    files: filterFiles,
    steps: [
      ...openFiles,
      (ui) => ui.press(ui.t("browser.filter.show")),
      (ui) => ui.type("no-such-file"),
      (ui) => ui.wait(400),
    ],
  },
  {
    id: "files-project-menu",
    group: GROUP,
    category: "normal",
    title: "Project actions with its files already visible",
    description: "Website's files are already visible, so its open actions menu omits Browse files. Open in file manager remains available; in the gallery it acknowledges the request without opening a host application.",
    state,
    steps: [...openFiles, (ui) => ui.press(ui.t("sidebar.project.actions", { name: web.name }))],
  },
  {
    id: "files-row-menu",
    group: GROUP,
    category: "normal",
    title: "A file’s context menu",
    description: "Right-clicking main.ts opens its menu at the pointer without opening the viewer. Open uses the system’s default application; Show in file manager reveals and selects the file; Copy path copies its project-relative path. Native actions are acknowledged without opening a host application in the gallery. Folder rows offer the same actions.",
    state,
    steps: [...openFiles, (ui) => ui.press("src"), (ui) => ui.contextMenu("main.ts")],
  },
  {
    id: "files-row-no-clipboard",
    group: GROUP,
    category: "normal",
    title: "File actions without a clipboard",
    description: "The file’s menu keeps Open and Show in file manager on a platform without clipboard access. Copy path is absent; folder rows retain the same two native actions.",
    state,
    clipboard: "unavailable",
    steps: [...openFiles, (ui) => ui.press("src"), (ui) => ui.contextMenu("main.ts")],
  },
  {
    id: "files-viewer",
    group: "File viewer",
    category: "normal",
    title: "A file open from the tree",
    description:
      "The viewer opened over the project's files, on a file pressed in the tree. Previous file and Next file sit to the left of its title, leaving the bottom edge for content.",
    state,
    steps: [...openFiles, (ui) => ui.press("src"), (ui) => ui.press("main.ts"), (ui) => ui.wait(600)],
  },
  {
    id: "files-states",
    group: GROUP,
    category: "exceptional",
    title: "Folders that are empty, unreadable or cut short",
    state,
    files: {
      "": { dir: true, complete: false },
      empty: { dir: true },
      locked: unreadableDir("locked"),
      many: { dir: true, complete: false },
      "many/a.txt": { text: "a\n" },
      "notes.md": { text: "Notes\n" },
    },
    steps: [...openFiles, (ui) => ui.press("empty"), (ui) => ui.press("locked"), (ui) => ui.press("many"), (ui) => ui.wait(300)],
  },
  {
    id: "files-empty",
    group: GROUP,
    category: "normal",
    title: "A project folder with nothing in it",
    state,
    files: { "": { dir: true } },
    steps: openFiles,
  },
  {
    id: "files-root-error",
    group: GROUP,
    category: "exceptional",
    title: "A project folder that cannot be read",
    description: "The failure has a danger alert with a heading and a separate Try again control. Narrow shows the same failure after opening the project pane drawer.",
    state,
    files: {
      "": {
        dir: true,
        error: {
          code: "source_unavailable",
          params: { path: "/Users/dev/code/website", detail: "No such file or directory (os error 2)" },
          message: "The directory /Users/dev/code/website is not available: No such file or directory (os error 2)",
        },
      },
    },
    steps: openFiles,
  },
];
