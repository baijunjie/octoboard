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

export const projectFilesScenarios: Scenario[] = [
  {
    id: "files-tree",
    group: GROUP,
    title: "A project session's files",
    description:
      "Folders first, natural order, .git left out. src is expanded and main.ts is selected: its rounded highlight has space on all four sides, matching Git rows. Compare its actions button with the selected sidebar session and the console header: resting glyphs stay muted, while hovering or opening each menu gives it the shared subtle tint. Press a file for the viewer and Left/Right to visit visible files. Hover or focus file and folder rows for their Copy path actions. Select Hub, then Search API actions → Browse files to inspect a project without changing the selected console session. Narrow the window and open the project pane drawer to see the same tree over the terminal.",
    state,
    steps: [...openFiles, (ui) => ui.press("src"), (ui) => ui.press("main.ts"), (ui) => ui.wait(600), (ui) => ui.key("Escape")],
  },
  {
    id: "files-row-no-clipboard",
    group: GROUP,
    title: "File and folder rows where nothing can be copied",
    description: "The current row, main.ts, on a platform with no clipboard: no actions button is drawn on files or folders, whether expanded or collapsed.",
    state,
    clipboard: "unavailable",
    steps: [...openFiles, (ui) => ui.press("src"), (ui) => ui.press("main.ts"), (ui) => ui.wait(600), (ui) => ui.key("Escape"), (ui) => ui.wait(300)],
  },
  {
    id: "files-viewer",
    group: GROUP,
    title: "A file open from the tree, with a toast",
    description:
      "The viewer opened over the project's files, on a file pressed in the tree. A toast is raised while it is open: the toast stack rides above the viewer's footer, clear of Previous file and Next file.",
    state,
    toasts: [{ kind: "error", code: "path_not_found", params: { path: "/Users/dev/code/missing" }, message: "" }],
    steps: [...openFiles, (ui) => ui.press("src"), (ui) => ui.press("main.ts"), (ui) => ui.wait(600)],
  },
  {
    id: "files-states",
    group: GROUP,
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
    title: "A project folder with nothing in it",
    state,
    files: { "": { dir: true } },
    steps: openFiles,
  },
  {
    id: "files-root-error",
    group: GROUP,
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
