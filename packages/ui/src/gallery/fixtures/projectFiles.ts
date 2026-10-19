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

/** One path of a fixture project: a directory (cut short when `complete` is false), a file's text,
 * an image's bytes or a binary of no known type and the size it reports; with `error`, what reading
 * it (listing it, for a directory) is answered with. */
export type FixtureFile =
  | { dir: true; complete?: boolean; error?: FixtureError }
  | { text: string; error?: FixtureError }
  | { image: { mediaType: string; data: string }; error?: FixtureError }
  | { binary: { size: number }; error?: FixtureError };

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
  // What the daemon lists but neither the tree nor the file name search shows.
  ".git": { dir: true },
  ".git/config": { text: "[core]\n" },
  ".DS_Store": { text: "" },
  "src/._server.ts": { text: "" },
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
const fileTypeFiles: FixtureFiles = {
  "": { dir: true },
  ".env.example": { text: "PORT=8080\n" },
  ".gitignore": SAMPLE_FILES[".gitignore"]!,
  "archive.zip": { text: "" },
  "certificate.pem": { text: "" },
  "data.csv": { text: "id,title\n1,Search\n" },
  Dockerfile: { text: "FROM node:24\n" },
  "font.woff2": { text: "" },
  LICENSE: { text: "MIT License\n" },
  "logo.png": SAMPLE_FILES["docs/logo.png"]!,
  "package.json": SAMPLE_FILES["package.json"]!,
  "README.md": SAMPLE_FILES["README.md"]!,
  "recording.mp3": { text: "" },
  "schema.sql": { text: "CREATE TABLE documents (id INTEGER PRIMARY KEY);\n" },
  src: { dir: true },
  "src/server.ts": SAMPLE_FILES["src/server.ts"]!,
  "unknown.custom": { text: "A file with an unrecognized extension.\n" },
  "walkthrough.mp4": { text: "" },
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
    description: "The Files panel docked in a wide window is one rounded card with a hairline edge and the shadow of the sidebar's floating card, 16px from the terminal (the width handle's gap) and 8px from the window's end, the bottom and the top bar, the gaps showing the terminal's own background rather than a sidebar-coloured column. Its 40px header runs the project name, the Files and Changes tabs, the filter and refresh buttons, then Close panel, with no icon before the name; there is no maximize while Files is the only panel. The top bar has only the Files toggle, pressed, and no toggle for the pane itself. Folders come first in natural order, with .git and the system files .DS_Store and ._server.ts left out. src is expanded and main.ts is selected, its rounded highlight keeping space on all four sides, as Git rows do. Expand or collapse a folder to watch its children slide and fade in or out with the same timing as the session menu, while following rows slide into place. Press a file for the viewer, then Left/Right to visit visible files. Switch between the Website and Search API sessions to inspect the short file-pane entrance and Search API's own tree. Switching between the two Website sessions keeps the tree steady. A project's actions offer Browse files when its pane is hidden or belongs to another project.",
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
    id: "files-pane-handle",
    group: GROUP,
    category: "normal",
    title: "Files pane's width handle in focus and under the pointer",
    description: "The handle in the gap on the pane's inner edge shows its grip, a short rounded pill centred in the gap, and a Resize tooltip opening beside it, on its end side and centred on it, while the pointer is on it. The grip is drawn here through keyboard focus (with its focus ring), as the gallery cannot hold a CSS hover; with the pointer alone it shows on hover and during a drag, and is hidden otherwise. Arrow keys resize the pane from the focused handle, and double-clicking resets its width.",
    state,
    steps: [...openFiles, (ui) => ui.focusControl(ui.t("pane.resizeAside")), (ui) => ui.hoverAnchored(ui.t("pane.resizeAside"), ui.t("pane.resize"))],
  },
  {
    id: "files-filter",
    group: GROUP,
    category: "normal",
    title: "Matching files throughout the project",
    description: "The filter sits above the matching files, which stay in the folder tree: both server.ts files match the trimmed, case-insensitive filename query without opening any folders first, each under its folders, which start open and can be folded. The header reads the project title, then the Files and Changes tabs, then the filter and Refresh buttons and Close panel at the end; Files has no layout choice. Each tab keeps its own query and folded folders.",
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
    id: "files-file-icons",
    group: GROUP,
    category: "normal",
    title: "Icons for each kind of file",
    description: "Muted icons distinguish source code, configuration, JSON, documents, images, audio, video, archives, spreadsheets, databases, certificates, fonts, Git files and licenses; an unknown extension keeps the plain file icon.",
    state,
    files: fileTypeFiles,
    steps: [...openFiles, (ui) => ui.wait(400)],
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
    description: "Website's files are already visible, so its actions menu omits Browse files. Open in groups File manager and Terminal; no repository destination appears without a remote.",
    state,
    steps: [...openFiles, (ui) => ui.press(ui.t("sidebar.project.actions", { name: web.name })), (ui) => ui.press(ui.t("common.openIn"))],
  },
  {
    id: "files-row-menu",
    group: GROUP,
    category: "normal",
    title: "A file’s context menu",
    description: "Right-clicking main.ts opens its menu at the pointer without opening the viewer. Copy path stays in the parent menu. Open in groups File manager, which reveals and selects the file, and Default application. Folder rows offer the same actions.",
    state,
    steps: [...openFiles, (ui) => ui.press("src"), (ui) => ui.contextMenu("main.ts"), (ui) => ui.press(ui.t("common.openIn"))],
  },
  {
    id: "files-viewer",
    group: "File viewer",
    category: "normal",
    title: "A file open from the tree",
    description:
      "The viewer opened over the project's files, on a file pressed in the tree. Its path uses the same code icon as the file tree. Previous file and Next file sit to the left of its title, leaving the bottom edge for content. File manager and Default application end the header row, with the same icons as the row menu's.",
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
    description: "A required error alert opens over the failed folder. After OK, the failure keeps a heading and a separate Try again control. Narrow shows the same failure in the docked Files card.",
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
