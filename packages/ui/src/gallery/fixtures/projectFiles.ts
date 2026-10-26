// The files the fixture daemon serves for a project's browser (`fixtureDaemon.ts`), and the
// gallery's "Project files" scenarios over them: the tree, the viewer opened from it, and the states
// a listing or a read can end in.
import type { MessageParams } from "../../protocol";
import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import { SAMPLE, failureTitle, snapshotState } from "./builders";
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
 * it (listing it, for a directory) is answered with. With `ignored`, Git ignores it, and so
 * everything beneath it, as the daemon marks it. */
export type FixtureFile = (
  | { dir: true; complete?: boolean; error?: FixtureError }
  | { text: string; error?: FixtureError }
  | { image: { mediaType: string; data: string }; error?: FixtureError }
  | { binary: { size: number }; error?: FixtureError }
) & { ignored?: true };

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
  // What Git ignores: a generated sibling of a tracked file, and two folders holding nothing else.
  "src/server.gen.ts": { text: "export {};\n", ignored: true },
  dist: { dir: true, ignored: true },
  "dist/server.js": { text: "export {};\n" },
  node_modules: { dir: true, ignored: true },
  "node_modules/serverless": { dir: true },
  "node_modules/serverless/server.js": { text: "module.exports = {};\n" },
};
/** `filterFiles` with a folder the search cannot read, which makes every search of it incomplete. */
const incompleteSearchFiles: FixtureFiles = { ...filterFiles, locked: unreadableDir("locked") };
const searchFor = (query: string) => [
  ...openFiles,
  (ui: Ui) => ui.press(ui.t("browser.filter.show")),
  (ui: Ui) => ui.type(query),
  (ui: Ui) => ui.wait(400),
];
/** A project with ignored entries among the tracked ones, folders and a file at its top level. */
const ignoredTreeFiles: FixtureFiles = {
  ...SAMPLE_FILES,
  ".gitignore": { text: "node_modules\ndist\ntarget\n.env\n" },
  ".env": { text: "PORT=8080\n", ignored: true },
  dist: { dir: true, ignored: true },
  "dist/main.js": { text: "export {};\n" },
  node_modules: { dir: true, ignored: true },
  "node_modules/left-pad": { dir: true },
  "node_modules/left-pad/index.js": { text: "module.exports = {};\n" },
  target: { dir: true, ignored: true },
  "target/debug": { dir: true },
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
    toasts: [{ kind: "error", code: "path_not_found", params: { path: "/Users/dev/code/missing" }, message: "", title: failureTitle("common.revealFailedTitle") }],
    steps: [...openFiles, (ui) => ui.press("src"), (ui) => ui.press("main.ts"), (ui) => ui.wait(600)],
  },
  {
    id: "files-tree",
    group: GROUP,
    category: "normal",
    title: "A project session's files",
    description: "The Files panel docked in a wide window is one rounded card with a hairline edge and the shadow of the sidebar's floating card, 16px from the terminal (the width handle's gap) and 8px from the window's end, the bottom and the top bar, the gaps showing the terminal's own background rather than a sidebar-coloured column. Its 40px header runs the project name, the Files and Changes tabs, the filter, Collapse all folders (enabled, with src open) and refresh buttons, then Close panel, with no icon before the name; Files has no maximize, even beside the Terminal panel. The top bar has the Terminal toggle, not pressed, before the Files toggle, pressed, and no toggle for the pane itself. Folders come first in natural order, with .git and the system files .DS_Store and ._server.ts left out. src is expanded and main.ts is selected, its rounded highlight keeping space on all four sides, as Git rows do. Expand or collapse a folder to watch its children slide and fade in or out with the same timing as the session menu, while following rows slide into place. Press a file for the viewer, then Left/Right to visit visible files. Switch between the Website and Search API sessions to see the pane fade in place, without sliding, onto Search API's own tree. Switching between the two Website sessions keeps the tree steady. A project's actions offer Browse files when its pane is hidden or belongs to another project.",
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
    id: "files-tree-ignored",
    group: GROUP,
    category: "normal",
    title: "Files that Git ignores in the tree",
    description: "dist, node_modules, target and .env are Git-ignored: each row is dimmed under a translucent veil of the pane's surface, name and icon together, as the sidebar dims its inactive projects, and so is everything inside the opened ignored folder (node_modules / left-pad), while src and the other tracked entries stay at full strength. .env is the selected row, so its veil is lifted; each ignored row lifts its own veil the same way while the pointer is on it, it holds keyboard focus or its actions menu is open. With src and node_modules open, Collapse all folders in the header, between the filter and Refresh buttons, is enabled; press it to fold both.",
    state,
    files: ignoredTreeFiles,
    steps: [...openFiles, (ui) => ui.press("src"), (ui) => ui.press(ui.t("browser.tree.row.ignored", { name: "node_modules" })), (ui) => ui.press(ui.t("browser.tree.row.ignored", { name: "left-pad" })), (ui) => ui.press(ui.t("browser.tree.row.ignored", { name: ".env" })), (ui) => ui.wait(600), (ui) => ui.key("Escape")],
  },
  {
    id: "files-tree-ignored-collapsed",
    group: GROUP,
    category: "normal",
    title: "Ignored folders with nothing open",
    description: "No folder is open, so Collapse all folders in the header is disabled. The ignored folders (dist, node_modules, target) and .env are all under the dimming veil, none lifted, among the tracked entries at full strength.",
    state,
    files: ignoredTreeFiles,
    steps: [...openFiles, (ui) => ui.wait(400)],
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
    description: "The filter sits above the matching files, which stay in the folder tree: both server.ts files match the trimmed, case-insensitive filename query without opening any folders first, each under its folders, which start open and can be folded. The Include ignored files toggle, not pressed (a file with a cross), ends the field's row: the Git-ignored matches (dist, node_modules and a generated server.gen.ts) are left out, so no row is dimmed. The header reads the project title, then the Files and Changes tabs, then the filter, Collapse all folders and Refresh buttons and Close panel at the end; Files has no layout choice. Each tab keeps its own query and folded folders.",
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
    id: "files-filter-ignored",
    group: GROUP,
    category: "normal",
    title: "Matching files including the ones Git ignores",
    description: "The same query with the Include ignored files toggle pressed (its icon a file with a magnifier, where the unpressed one shows a file with a cross): the ignored matches join the results under the dimming veil — server.gen.ts beside the tracked server.ts files, and the dist and node_modules folders above their ignored matches, veiled as folders because every result beneath them is ignored, while src, which also holds a tracked match, stays at full strength.",
    state,
    files: filterFiles,
    steps: [
      ...openFiles,
      (ui) => ui.press(ui.t("browser.filter.show")),
      (ui) => ui.type(" SERVER "),
      (ui) => ui.wait(400),
      (ui) => ui.press(ui.t("browser.filter.includeIgnored")),
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
    id: "files-project-focus-menu",
    group: GROUP,
    category: "normal",
    title: "Project focus actions with its files already visible",
    description: "In Website's focus mode with its files visible, the header's actions menu has no Browse files, and starts directly with Open in: no separator line above it.",
    state,
    preferences: { sidebarConsole: console_.id, sidebarFocus: `project:${web.id}` },
    steps: [...openFiles, (ui) => ui.press(ui.t("sidebar.project.actions", { name: web.name }))],
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
    description: "Under the opened folders: \"Empty folder\" in muted italics; the unreadable folder stays drawn closed (closed chevron), with a danger-coloured circled \"!\" named Could not list this folder right after its name, nothing beneath it and no sentence of error text in the row; and the cut-short one, open over the entries it did list, with a warning triangle right after its own name and no extra row among its entries. The project root itself is whole here; a cut-short root is marked at the pane's title, in the next scenarios.",
    state,
    files: {
      "": { dir: true },
      empty: { dir: true },
      locked: unreadableDir("locked"),
      many: { dir: true, complete: false },
      "many/a.txt": { text: "a\n" },
      "notes.md": { text: "Notes\n" },
    },
    steps: [...openFiles, (ui) => ui.press("empty"), (ui) => ui.press("locked"), (ui) => ui.press("many"), (ui) => ui.wait(300)],
  },
  {
    id: "files-root-cut-short",
    group: GROUP,
    category: "exceptional",
    title: "A project folder listed only in part",
    description: "The daemon stopped listing the project's own folder early, so the tree holds what it did list and a warning triangle stands right after the pane's title, in the header row, with no sentence under the tree. Its popover is shown in the next scenario.",
    state,
    files: { ...SAMPLE_FILES, "": { dir: true, complete: false } },
    steps: openFiles,
  },
  {
    id: "files-root-cut-short-hover",
    group: GROUP,
    category: "exceptional",
    title: "Why a project folder is listed only in part, from a resting mouse",
    description: "The mouse rests on the warning triangle after the pane's title: its popover opens above it with no click and no focus taken, saying not every entry of this folder is shown. The tree stays as it is.",
    state,
    files: { ...SAMPLE_FILES, "": { dir: true, complete: false } },
    steps: [...openFiles, (ui) => ui.hoverStatusMark(ui.t("browser.partial"))],
  },
  {
    id: "files-folder-cut-short-hover",
    group: GROUP,
    category: "exceptional",
    title: "Why a folder is listed only in part, from a resting mouse",
    description: "The mouse rests on the warning triangle right after the open folder many's name: its popover opens above it with no click and no focus taken, saying not every entry of this folder is shown. The folder's entries stay listed beneath it with no row of their own saying so.",
    state,
    files: {
      "": { dir: true },
      many: { dir: true, complete: false },
      "many/a.txt": { text: "a\n" },
      "many/b.txt": { text: "b\n" },
      "notes.md": { text: "Notes\n" },
    },
    steps: [...openFiles, (ui) => ui.press("many"), (ui) => ui.hoverStatusMark(ui.t("browser.partial"))],
  },
  {
    id: "files-folder-failure-reason",
    group: GROUP,
    category: "exceptional",
    title: "The reason a folder could not be listed",
    description: "The unreadable folder is drawn closed, with nothing beneath it, and the danger-coloured mark right after its name is tapped with a finger: its popover holds why it could not be listed, then the system's own text, then Try again at its trailing edge, with focus inside it.",
    state,
    files: {
      "": { dir: true },
      locked: unreadableDir("locked"),
      "notes.md": { text: "Notes\n" },
    },
    steps: [...openFiles, (ui) => ui.press("locked"), (ui) => ui.touchPressStatusMark(ui.t("browser.tree.errorLabel"))],
  },
  {
    id: "files-folder-failure-hover",
    group: GROUP,
    category: "exceptional",
    title: "The reason a folder could not be listed, from a resting mouse",
    description: "The unreadable folder is drawn closed, with nothing beneath it, and the mouse rests on the danger-coloured mark right after its name: the same popover opens above it with no click and no focus taken, so the terminal keeps focus, and it stays up while the pointer is on the mark or on the popover. A mouse click opens nothing.",
    state,
    files: {
      "": { dir: true },
      locked: unreadableDir("locked"),
      "notes.md": { text: "Notes\n" },
    },
    steps: [...openFiles, (ui) => ui.press("locked"), (ui) => ui.hoverStatusMark(ui.t("browser.tree.errorLabel"))],
  },
  {
    id: "files-filter-incomplete",
    group: GROUP,
    category: "exceptional",
    title: "Matching files from a search that is not whole",
    description: "The matches are listed as usual, and a warning triangle sits inside the filter field, right before its Clear (x) button, which stays in place; the Include ignored files toggle is beside the field, and no sentence is added under the list. Its name is \"The search is incomplete\"; the popover it opens is shown in the next scenario.",
    state,
    files: incompleteSearchFiles,
    steps: searchFor(" SERVER "),
  },
  {
    id: "files-filter-incomplete-hover",
    group: GROUP,
    category: "exceptional",
    title: "Why a search is not whole, from a resting mouse",
    description: "The mouse rests on the warning triangle inside the filter field, before Clear: its popover opens above it with no click and no focus taken, saying the search stopped at its limits or could not read some files or folders, so not every match may be listed. The matches stay listed beneath. A touch or a key press opens the same popover with focus inside.",
    state,
    files: incompleteSearchFiles,
    steps: [...searchFor(" SERVER "), (ui) => ui.hoverStatusMark(ui.t("browser.filter.partial"))],
  },
  {
    id: "files-filter-incomplete-empty",
    group: GROUP,
    category: "exceptional",
    title: "No matching files from a search that is not whole",
    description: "The empty state says there are no matching files, and the warning triangle inside the filter field, before its Clear button, says the search may have missed some, so the emptiness is not final.",
    state,
    files: incompleteSearchFiles,
    steps: searchFor("no-such-file"),
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
    description: "The failure takes the empty-state shape in place of the tree: a danger glyph, the heading and the application's own explanation of why the folder cannot be read, with Try again as the primary button and Details, which holds the system's reason, beside it. Narrow shows the same failure in the docked Files card.",
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
