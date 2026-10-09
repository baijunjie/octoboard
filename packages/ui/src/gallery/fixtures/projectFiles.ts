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

const { console: console_, web, api, consoleSession, sessions } = SAMPLE;
const state = snapshotState({ consoles: [console_], projects: [web, api], sessions });
const openFiles = [(ui: Ui) => ui.press(ui.session("Fix the summary layout")), (ui: Ui) => ui.wait(400)];

export const projectFilesScenarios: Scenario[] = [
  {
    id: "files-tree",
    group: GROUP,
    title: "A project session's files",
    description:
      "Selecting a project session puts its project's files in the aside: folders first, natural order, .git left out. src is expanded; press a file to open the viewer, then Left and Right to move through the files on screen.",
    width: 1440,
    state,
    steps: [...openFiles, (ui) => ui.press("src"), (ui) => ui.wait(300)],
  },
  {
    id: "files-viewer",
    group: GROUP,
    title: "A file open from the tree",
    width: 1440,
    state,
    steps: [...openFiles, (ui) => ui.press("src"), (ui) => ui.press("main.ts"), (ui) => ui.wait(600)],
  },
  {
    id: "files-viewer-toast",
    group: GROUP,
    title: "A toast while the viewer is open",
    description: "The toast stack rides above the viewer's footer, clear of Previous file and Next file.",
    width: 1148,
    state,
    toasts: [{ kind: "error", code: "path_not_found", params: { path: "/Users/dev/code/missing" }, message: "" }],
    steps: [...openFiles, (ui) => ui.press("src"), (ui) => ui.press("main.ts"), (ui) => ui.wait(600)],
  },
  {
    id: "files-no-session",
    group: GROUP,
    title: "Browsing a project with no session",
    description: "Browse files from a project's menu: the console session stays selected, and the aside shows the project's files instead of its report.",
    width: 1440,
    state,
    steps: [
      (ui) => ui.press(ui.session(consoleSession.title)),
      (ui) => ui.press(ui.t("sidebar.project.actions", { name: api.name })),
      (ui) => ui.press(ui.t("sidebar.project.browse")),
      (ui) => ui.wait(400),
    ],
  },
  {
    id: "files-states",
    group: GROUP,
    title: "Folders that are empty, unreadable or cut short",
    width: 1440,
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
    id: "files-root-error",
    group: GROUP,
    title: "A project folder that cannot be read",
    width: 1440,
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
  {
    id: "files-narrow",
    group: GROUP,
    title: "Narrow window, files drawer open",
    width: 800,
    state,
    steps: [
      (ui) => ui.press(ui.t("titleBar.sidebar.show")),
      (ui) => ui.press(ui.session("Fix the summary layout")),
      (ui) => ui.press(ui.t("rail.projectPane.show")),
      (ui) => ui.wait(500),
    ],
  },
];
