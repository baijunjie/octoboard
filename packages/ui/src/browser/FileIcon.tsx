import {
  Database, File, FileArchive, FileAudio, FileCode, FileCog, FileImage,
  FileJson, FileKey, FileQuestion, FileSpreadsheet, FileSymlink, FileTerminal,
  FileText, FileType, FileVideo, GitBranch, Scale, type LucideIcon,
} from "lucide-react";
import type { ReactElement } from "react";

import type { BrowseEntry } from "../protocol";
import { wireBaseName } from "../wirePath";

const FILE_TYPES: readonly [RegExp, LucideIcon][] = [
  [/^(licen[cs]e|copying|notice)(\..*)?$/i, Scale],
  [/^(\.gitignore|\.gitattributes|\.gitmodules|\.gitkeep)$/i, GitBranch],
  [/^(dockerfile|containerfile|makefile|gnumakefile|justfile)(\..*)?$/i, FileTerminal],
  [/^(\.env(\..*)?|\.editorconfig|\.npmrc|\.yarnrc|\.prettierrc|\.eslintrc)$/i, FileCog],
  [/\.(png|jpe?g|gif|webp|bmp|ico|avif|svg|tiff?|heic)$/i, FileImage],
  [/\.(mp3|wav|ogg|flac|aac|m4a|opus|aiff)$/i, FileAudio],
  [/\.(mp4|webm|mov|mkv|avi|m4v|mpeg|mpg)$/i, FileVideo],
  [/\.(zip|gz|bz2|xz|zst|tar|tgz|7z|rar|jar|war)$/i, FileArchive],
  [/\.(csv|tsv|xlsx?|ods)$/i, FileSpreadsheet],
  [/\.(sql|sqlite3?|db)$/i, Database],
  [/\.(pem|key|crt|cer|p12|pfx|pub)$/i, FileKey],
  [/\.(woff2?|ttf|otf|eot)$/i, FileType],
  [/\.(jsonc?|json5|ipynb)$/i, FileJson],
  [/\.(ya?ml|toml|ini|conf|cfg|config|properties|lock)$/i, FileCog],
  [/\.(sh|bash|zsh|fish|ps1|bat|cmd)$/i, FileTerminal],
  [/\.(mdx?|markdown|txt|text|rst|rtf|pdf|docx?|odt|log)$/i, FileText],
  [/\.(tsx?|jsx?|mjs|cjs|mts|cts|html?|css|scss|sass|less|vue|svelte|astro|rs|py|pyi|go|java|kt|kts|swift|c|h|cc|cpp|cxx|hpp|cs|fs|fsx|rb|php|lua|r|ex|exs|erl|hrl|hs|scala|dart|zig|xml|graphql|gql|proto|wasm)$/i, FileCode],
];

export function FileIcon({ entry, path }: { entry: BrowseEntry | undefined; path: string }): ReactElement {
  const name = wireBaseName(path);
  const Icon = !entry || entry.kind === "other"
    ? FileQuestion
    : entry.kind === "symlink"
      ? FileSymlink
      : FILE_TYPES.find(([pattern]) => pattern.test(name))?.[1] ?? File;
  return <Icon aria-hidden="true" className="size-4 shrink-0 text-app-muted" />;
}
