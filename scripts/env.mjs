// A minimal dotenv reader shared by the scripts that take configuration from the repository's env files:
// `KEY=VALUE` per line (an `export ` prefix and spaces around `=` are tolerated, so lines lifted from a shell
// profile work), `#` lines and blank lines ignored, one layer of matching quotes stripped, no `$VAR`
// expansion. Files are read from the repository root, in the order given, later ones overriding earlier ones.
//
// An empty value counts as unset, in a file and in the real environment alike: a template's `KEY=` must not
// turn into an empty credential. A real environment variable always wins over a file value, so CI, which sets
// the variables directly, needs no file.
//
// Which files a caller reads is the safety mechanism: `.env.secret` holds credentials, so a caller names it
// only if it needs them. A release-path caller likewise names no file with a `VITE_` entry, since vite inlines
// those into the bundle it builds from the environment it inherits.
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

export function loadEnv(...names) {
  const fromFiles = {};
  for (const name of names) {
    const file = path.join(repoRoot, name);
    if (existsSync(file)) Object.assign(fromFiles, parse(readFileSync(file, "utf8")));
  }
  for (const [key, value] of Object.entries(fromFiles)) {
    if (!process.env[key]) process.env[key] = value;
  }
}

function parse(text) {
  const values = {};
  for (const line of text.split(/\r?\n/)) {
    const match = /^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=(.*)$/.exec(line);
    if (!match) continue;
    const value = unquote(match[2].trim());
    if (value) values[match[1]] = value;
  }
  return values;
}

function unquote(value) {
  const quote = value[0];
  return (quote === '"' || quote === "'") && value.length > 1 && value.endsWith(quote)
    ? value.slice(1, -1)
    : value;
}
