// The files the fixture daemon serves for a project's browser (`fixtureFiles.ts`): the sample project every
// project shows unless its fixture gives other files, and the errors a path can be answered with.
import type { MessageParams } from "../errors/codedError";
import type { SourceUnavailableCause } from "../errors/causes";
import { IMAGES } from "./viewerImages";

/** The daemon error a request for a fixture path is answered with. */
export interface FixtureError {
  code: string;
  params: MessageParams;
  message: string;
}

/** A `source_unavailable` as the daemon sends it, with the `cause` its remedy is worded by. `detail` is the daemon's own
 * text for a scope that is not in another worktree or is reached through a link there, and for a folder that is not a
 * folder ("not a directory"); for any other failure (including the project's own folder gone) it is the system's text. */
export function sourceUnavailable(path: string, cause: SourceUnavailableCause, detail: string): FixtureError {
  return { code: "source_unavailable", params: { path, cause, detail }, message: `${path} is unavailable: ${detail}` };
}

/** A `permission_denied` as the daemon sends it when the system denied reading a directory a request is scoped to: no
 * `cause`, the path and the system's own text. */
export function permissionDenied(path: string, detail: string): FixtureError {
  return { code: "permission_denied", params: { path, detail }, message: `${path} could not be read: ${detail}` };
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
