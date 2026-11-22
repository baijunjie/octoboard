// Hold every write of the application's own Rust code to a file its crate's registration names (P9 in
// docs/conventions/principles.md): a call of the vocabulary below, in production code, outside a registered file
// fails. What this cannot see is in README.md, "Registered writes".
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { productionSource, testOnly } from "./provider-neutral.mjs";
import { matchingRules } from "./scope.mjs";

// Each crate's production source, its registration and the list its clean-up before uninstalling reads: in the
// source text between `open` and the first `close` after it, `item` matches each exception with something to undo.
export const CRATES = [
  {
    name: "daemon",
    source: "apps/daemon/src",
    registration: "apps/daemon/src/registered_writes.json",
    cleanup: { file: "apps/daemon/src/coordinator/system_changes.rs", open: "pub const EXCEPTIONS: &[Exception] = &[", close: "];", item: /[\w:]+::EXCEPTION\b/g },
  },
  {
    name: "desktop",
    source: "apps/desktop/src-tauri/src",
    registration: "apps/desktop/src-tauri/src/registered_writes.json",
    cleanup: { file: "apps/desktop/src-tauri/src/system_changes.rs", open: "pub static REGISTERED: &[&dyn Exception] = &[", close: "];", item: /(?<=&)[\w:]+/g },
  },
];

// Why each kind of entry exists. A file's kind says whose word its writes go by, which is what a reviewer of a
// new entry has to judge; only `exception` is a write into the user's own system that Octoboard decided on.
export const KINDS = {
  primitive: "The helper layer: it writes at the path its caller names. Every public function of the file is a word of the vocabulary, so its callers are registered themselves, but for the ones the entry lists as `readers`.",
  own: "Writes only inside Octoboard's own directories or its own system resources (its delegated cgroup subtree), or a temporary file removed again. Declared and reviewed, not proven.",
  installation: "Octoboard's own installation and presence outside its directories (its executable and what an update puts beside it, the application its updater replaces, the taskbar's jump list), which an uninstall removes. Not an exception.",
  exception: "A write into the user's own system that the user approved as an exception to P9; names its exceptions.",
  user_path: "Writes at a path the user named for that write, at the user's own action (a save dialog, a project's folder), or an account's own directory, the default account's included, created empty where the agent's client would create it anyway.",
};

// The comment that says a line the vocabulary matches writes no file (a device, a pipe, a terminal), with why. It
// stands at the end of the line or in the comment lines right above it.
export const ACKNOWLEDGED = /\/\/.*?\bno-file-write:[ \t]*(\S[^\n]*)?/;

const calls = (names) => new RegExp(`\\b(?:${names.join("|")})\\s*\\(`, "g");
const words = (names) => new RegExp(`\\b(?:${names.join("|")})\\b`, "g");
const FS_FUNCTIONS = ["write", "create_dir", "create_dir_all", "remove_file", "remove_dir", "remove_dir_all", "rename", "copy", "hard_link", "soft_link", "set_permissions", "symlink", "symlink_file", "symlink_dir", "chown", "lchown", "fchown"];

// What counts as a raw write. Each entry has `examples`, one per form it takes, which the test holds it to. Strings
// and comments are blanked before matching, so a name inside one is not a hit. Opening a file counts only with a
// flag that writes, so that reading one, or a device, needs no entry.
export const VOCABULARY = [
  { word: "fs function", pattern: new RegExp(`\\bfs\\s*::\\s*(?:${FS_FUNCTIONS.join("|")})\\b`, "g"), examples: FS_FUNCTIONS.map((name) => `std::fs::${name}(a, b)?;`).concat("tokio::fs::remove_dir_all(&dir).await?;"), why: "the standard library's and tokio's writing, creating, renaming, removing, linking, owner and permission functions" },
  { word: "fs import", pattern: new RegExp(`\\buse\\s+(?:std|tokio)::(?:os::\\w+::)?fs::(?:\\{[^}]*\\b)?(?:${FS_FUNCTIONS.join("|")})\\b`, "g"), examples: ["use std::fs::{read, rename};", "use tokio::fs::write;", "use std::os::unix::fs::symlink;"], why: "the same functions imported by name, which a call would then not show" },
  { word: "alias", pattern: /\b(?:std|tokio)::fs\s+as\s+\w+|\b(?:std|tokio)::fs::\{\s*self\s+as\s+\w+|\b(?:File|OpenOptions|DirBuilder|Connection)\s+as\s+\w+|\bfs_err\b/g, examples: ["use std::fs as files;", "use std::fs::{self as files, read};", "use std::fs::{self, File as F};", "use rusqlite::Connection as Db;", "fs_err::write(a, b)?;"], why: "a writing module or type under another name, and the `fs_err` crate's copies of them, which the other words would not show" },
  { word: "file creation", pattern: /\bFile\s*::\s*(?:create|create_new)\b|\bDirBuilder\b/g, examples: ["let file = File::create(path)?;", "let file = std::fs::File::create_new(path)?;", "std::fs::DirBuilder::new().mode(0o700).create(&dir)?;"], why: "creating a file or a directory" },
  { word: "open to write", pattern: /\.\s*(?:write|create|append|truncate|create_new)\s*\(\s*true\s*\)/g, examples: ["OpenOptions::new().write(true).open(path)?;", "options.create(true);", "File::options().append(true).open(path)?;", "options.truncate(true);", "options.create_new(true);"], why: "an open with a flag that writes, set to `true`; one that only reads is not a hit" },
  { word: "file handle change", pattern: /\.\s*(?:set_permissions|set_len|set_modified|set_times)\s*\(/g, examples: ["file.set_permissions(permissions)?;", "file.set_len(0)?;", "file.set_modified(time)?;", "file.set_times(times)?;"], why: "changing a file through its handle" },
  { word: "link or owner call", pattern: calls(["symlink", "symlink_file", "symlink_dir", "hard_link", "chown", "lchown", "fchown"]), examples: ["symlink(target, &temporary)?;", "symlink_dir(target, at)?;", "hard_link(from, to)?;", "lchown(path, uid, gid)?;"], why: "the platform link and owner functions called by their bare name" },
  { word: "libc file call", pattern: /\blibc\s*::\s*(?:creat|unlink|unlinkat|rename|renameat|renameat2|mkdir|mkdirat|rmdir|chmod|fchmod|fchmodat|chown|fchown|lchown|fchownat|symlink|symlinkat|link|linkat|mkfifo|mknod|mkstemp|mkostemp|mkdtemp|truncate|ftruncate|utimensat|futimens|utimes|futimes|lutimes|setxattr|fsetxattr|removexattr|fremovexattr|clonefile|fclonefileat|copyfile|fcopyfile)\s*\(/g, examples: ["unsafe { libc::unlink(path.as_ptr()) };", "libc::setxattr(path, name, value, size, 0, 0)", "libc::mkstemp(template)", "libc::renameat2(a, b, c, d, 0)"], why: "the C library's file calls that change something" },
  { word: "open flag that writes", pattern: /\bO_(?:WRONLY|RDWR|CREAT|TRUNC|APPEND)\b|\b(?:GENERIC_WRITE|GENERIC_ALL|FILE_GENERIC_WRITE|FILE_WRITE_DATA|FILE_APPEND_DATA)\b/g, examples: ["libc::open(path, libc::O_WRONLY | libc::O_CLOEXEC)", "let flags = libc::O_RDWR;", "CreateFileW(name, GENERIC_WRITE, 0, null(), OPEN_EXISTING, 0, null_mut())", "let access = FILE_APPEND_DATA;"], why: "the flags with which `open` and `CreateFileW` write, wherever they are named: the call itself may take them from a variable" },
  { word: "Windows file call", pattern: calls(["CreateDirectoryW", "DeleteFileW", "RemoveDirectoryW", "MoveFileExW", "MoveFileW", "ReplaceFileW", "CopyFileW", "CopyFileExW", "CreateHardLinkW", "CreateSymbolicLinkW", "SetFileAttributesW", "SetFileInformationByHandle", "SetNamedSecurityInfoW", "SetSecurityInfo", "SetFileSecurityW", "SHFileOperationW"]), examples: ["unsafe { DeleteFileW(path.as_ptr()) };", "SetNamedSecurityInfoW(name, kind, flags, owner, null(), acl, null())"], why: "the Windows file, link and access-control calls" },
  { word: "database", pattern: /\bConnection\s*::\s*open\w*/g, examples: ["let db = Connection::open(path)?;", "Connection::open_with_flags(path, flags)?", "Connection::open_with_flags_and_vfs(path, flags, vfs)?"], why: "opening an SQLite database, which creates it and its journal" },
  { word: "archive unpacking", pattern: /\.\s*unpack(?:_in)?\s*\(/g, examples: ["archive.unpack(dir)?;", "entry.unpack_in(dir)?;"], why: "the `tar` crate writing an archive's entries to disk" },
  { word: "Trash", pattern: /\btrash\s*::\s*(?:delete\w*|TrashContext|macos|os_limited)\b|\bmove_to_trash\b|\btrash_context\s*\(|\b(?:\w*tools|get\s*\(\s*\))\s*\.\s*trash\b/g, examples: ["trash::delete(path)?;", "let mut context = trash::TrashContext::default();", "trash: move_to_trash,", "crate::platform::fs::trash_context()", "let trash = state.host_tools.get().trash;", "carry_out(&git, removal, &forced, tools.trash)", "let trash = host_tools.trash;"], why: "the `trash` crate, the daemon's Trash tool, and taking that tool from the host tools: it is passed on as a function value, whose calls the check cannot see" },
  { word: "registry", pattern: words(["RegSetValueExW", "RegSetKeyValueW", "RegCreateKeyExW", "RegCreateKeyW", "RegDeleteValueW", "RegDeleteKeyValueW", "RegDeleteKeyW", "RegDeleteTreeW"]), examples: ["RegSetKeyValueW(HKEY_CURRENT_USER, key, name, REG_SZ, data, size);", "RegDeleteKeyValueW(HKEY_CURRENT_USER, key, name)"], why: "writing or deleting in the Windows registry" },
  { word: "credential store", pattern: words(["set_generic_password", "delete_generic_password", "CredWriteW", "CredDeleteW", "SecItemAdd", "SecItemUpdate", "SecItemDelete"]), examples: ["set_generic_password(service, account, secret)", "delete_generic_password(service, account)", "CredWriteW(&credential, 0)", "CredDeleteW(target, CRED_TYPE_GENERIC, 0)"], why: "writing or deleting in the macOS keychain or the Windows Credential Manager" },
  { word: "login item", pattern: /\bSMAppService\b|\bregisterAndReturnError\b|\bunregisterAndReturnError\b/g, examples: ["msg_send![&*service, registerAndReturnError: _]", "msg_send![&*service, unregisterAndReturnError: _]"], why: "the macOS login item" },
  { word: "jump list", pattern: /\bICustomDestinationList\b|\.\s*(?:CommitList|DeleteList)\s*\(/g, examples: ["list.CommitList()", "list.DeleteList(PCWSTR::null())", "let list: ICustomDestinationList = create()?;"], why: "the Windows taskbar's jump list" },
  { word: "application update", pattern: /\.\s*download_and_install\s*\(|\.\s*install\s*\(\s*&?\s*bytes\b/g, examples: ["update.download_and_install(on_chunk, on_done).await?;", "update.install(bytes)"], why: "the updater library replacing the application itself" },
];

// The word for a function the registration makes one: a public function of a `primitive` file, or an entry point
// another entry names.
export const REGISTERED_NAME = "registered function";

// Comments, strings and what `cfg(test)` proves absent from production are blanked, positions kept.
export const blanked = (source) => productionSource(source, true, true);

// The names of the functions `source` declares with any `pub`, methods included.
export const publicFunctions = (source) => [...new Set([...blanked(source).matchAll(/\bpub(?:\s*\([^)]*\))?\s+(?:(?:const|async|unsafe)\s+)*fn\s+(\w+)/g)].map((match) => match[1]))];

// The bodies of the functions called `name` that `source` declares (one per platform, say), blanked like it.
export function bodiesOf(source, name) {
  const production = blanked(source);
  const bodies = [];
  for (const match of production.matchAll(new RegExp(`\\bfn\\s+${name}\\b`, "g"))) {
    const open = production.indexOf("{", match.index);
    const end = production.indexOf(";", match.index);
    if (open < 0 || (end >= 0 && end < open)) continue;
    let depth = 0;
    let at = open;
    for (; at < production.length; at++) {
      if (production[at] === "{") depth++;
      if (production[at] === "}" && --depth === 0) break;
    }
    bodies.push(production.slice(open, at + 1));
  }
  return bodies;
}

// The writes of `source` by the vocabulary and by `names`, the functions a registration makes words: the line,
// the word and the source of each, with `reason` on the ones a comment acknowledges as no write to a file.
export function hitsIn(source, names = []) {
  const production = blanked(source);
  const lines = source.split("\n");
  const seen = new Map();
  // A call or a path to the function, not its declaration: a stub of the same name elsewhere declares, and writes nothing.
  const vocabulary = names.length ? [...VOCABULARY, { word: REGISTERED_NAME, pattern: new RegExp(`(?<!\\bfn\\s+)\\b(?:${names.join("|")})\\s*\\(`, "g") }] : VOCABULARY;
  for (const { word, pattern } of vocabulary) {
    for (const match of production.matchAll(pattern)) {
      const line = production.slice(0, match.index).split("\n").length;
      if (!seen.has(line)) seen.set(line, { line, word, text: lines[line - 1].trim() });
    }
  }
  for (const hit of seen.values()) {
    const marker = acknowledgementOf(lines, hit.line);
    if (marker) Object.assign(hit, { acknowledgedAt: marker.line, reason: marker.reason });
  }
  return [...seen.values()].sort((a, b) => a.line - b.line);
}

// The acknowledgement of the hit on line `at`: on the line itself, or in the run of comment lines right above it.
function acknowledgementOf(lines, at) {
  for (let index = at - 1; index >= 0; index--) {
    const marker = (index === at - 1 || /^\s*\/\//.test(lines[index])) && lines[index].match(ACKNOWLEDGED);
    if (marker) return { line: index + 1, reason: marker[1]?.trim() ?? "" };
    if (index < at - 1 && !/^\s*\/\//.test(lines[index])) return null;
  }
  return null;
}

// The lines of `source` that carry an acknowledgement, in a comment.
const acknowledgements = (source) => source.split("\n").flatMap((text, index) => (ACKNOWLEDGED.test(text) ? [index + 1] : []));

// The crate's Rust files that reach production: every `.rs` file but the external modules a `cfg(test)` proves
// absent from it. A file is not left out for its name, so a test helper compiled into production is checked.
export function productionFiles(root, source) {
  const files = [];
  const testModules = new Set();
  const declaration = /#\[cfg\((test|all\([^\]]*\))\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;/g;
  const walk = (directory) => {
    for (const entry of readdirSync(path.join(root, directory), { withFileTypes: true })) {
      const file = `${directory}/${entry.name}`;
      if (entry.isDirectory()) walk(file);
      else if (entry.isFile() && file.endsWith(".rs")) {
        files.push(file);
        const structure = productionSource(readFileSync(path.join(root, file), "utf8"), false, true);
        for (const match of structure.matchAll(declaration)) {
          if (!testOnly(match[1])) continue;
          const parent = /\/(?:mod|main|lib)\.rs$/.test(file) ? path.posix.dirname(file) : file.slice(0, -3);
          testModules.add(`${parent}/${match[2]}.rs`);
          testModules.add(`${parent}/${match[2]}/`);
        }
      }
    }
  };
  walk(source);
  return files.filter((file) => ![...testModules].some((module) => file === module || (module.endsWith("/") && file.startsWith(module)))).sort();
}

const heading = (text, section) => text.split("\n").some((line) => /^#{1,6}\s/.test(line) && line.replace(/^#+\s+/, "").trim() === section);

// The failures of one crate: unregistered writes, and a registration that no longer says what the code does.
export function checkCrate(root, crate) {
  const failures = [];
  const fail = (where, text) => failures.push({ file: where, text });
  const registrationPath = path.join(root, crate.registration);
  if (!existsSync(registrationPath)) return [{ file: crate.registration, text: "the registration is missing" }];
  let registration;
  try {
    registration = JSON.parse(readFileSync(registrationPath, "utf8"));
  } catch (err) {
    return [{ file: crate.registration, text: `the registration is not JSON: ${err.message}` }];
  }
  const entries = registration.entries ?? [];
  const exceptions = registration.exceptions ?? {};
  const sourceOf = (file) => readFileSync(path.join(root, file), "utf8");
  const registered = new Map();
  // The functions the registration makes words of the vocabulary: every public function of a primitive file but
  // its readers, and the entry points the other entries name.
  const names = new Set();
  const primitives = [];
  for (const entry of entries) {
    const where = `${crate.registration}: ${entry.path}`;
    const file = `${crate.source}/${entry.path}`;
    if (registered.has(file)) fail(where, "is registered twice");
    if (!Object.hasOwn(KINDS, entry.kind)) fail(where, `has no known kind (${Object.keys(KINDS).join(", ")})`);
    if (!entry.writes) fail(where, "does not say what it writes");
    const listed = entry.exceptions ?? [];
    if ((entry.kind === "exception") !== (listed.length > 0)) fail(where, "only an entry of the kind `exception` names exceptions, and it has to name one at least");
    for (const name of listed) if (!Object.hasOwn(exceptions, name)) fail(where, `names the exception \`${name}\`, which the registration does not define`);
    // A file, never a directory: a new file under a registered directory would be registered unseen.
    if (!existsSync(path.join(root, file)) || !statSync(path.join(root, file)).isFile()) {
      fail(where, "names no file that exists (a directory is not registered: each file that writes is)");
      continue;
    }
    registered.set(file, entry);
    const declared = publicFunctions(sourceOf(file));
    if ((entry.readers !== undefined || entry.entry_points !== undefined) && (entry.kind === "primitive") !== (entry.entry_points === undefined)) fail(where, "a `primitive` entry lists its `readers`, any other its `entry_points`");
    for (const reader of entry.readers ?? []) if (!declared.includes(reader)) fail(where, `lists the reader \`${reader}\`, which is no public function of the file`);
    for (const name of entry.entry_points ?? []) if (!new RegExp(`\\bfn\\s+${name}\\b`).test(blanked(sourceOf(file)))) fail(where, `names the entry point \`${name}\`, which the file does not declare`);
    if (entry.kind === "primitive") primitives.push([where, file, entry]);
    for (const name of entry.kind === "primitive" ? declared.filter((name) => !entry.readers?.includes(name)) : entry.entry_points ?? []) names.add(name);
  }
  // A reader is excused from the vocabulary because it writes nothing: one whose body holds a write is no reader.
  for (const [where, file, entry] of primitives) {
    for (const reader of entry.readers ?? []) {
      const written = bodiesOf(sourceOf(file), reader).flatMap((body) => hitsIn(body, [...names])).find((hit) => hit.reason === undefined);
      if (written) fail(where, `lists \`${reader}\` as a reader, and it writes (${written.word}): ${written.text}`);
    }
  }
  const used = new Set();
  for (const file of productionFiles(root, crate.source)) {
    const source = sourceOf(file);
    const hits = hitsIn(source, [...names]);
    const acknowledged = new Set(hits.map((hit) => hit.acknowledgedAt));
    for (const line of acknowledgements(source)) if (!acknowledged.has(line)) failures.push({ file, line, text: "acknowledges a write the vocabulary does not find on the line or right below: remove the comment" });
    for (const hit of hits.filter((hit) => hit.reason === "")) failures.push({ file, line: hit.acknowledgedAt, text: "an acknowledgement says why the line writes no file" });
    const writes = hits.filter((hit) => hit.reason === undefined);
    if (writes.length === 0) continue;
    if (registered.has(file)) used.add(file);
    else for (const hit of writes) failures.push({ file, line: hit.line, text: `unregistered write (${hit.word}): ${hit.text}` });
  }
  // An entry point is its file's write where the vocabulary sees none in it: a script sent to a remote host, say.
  for (const [file, entry] of registered) if (!used.has(file) && !entry.entry_points?.length) fail(`${crate.registration}: ${entry.path}`, "holds no write any more: remove the entry");

  const cleanupSource = existsSync(path.join(root, crate.cleanup.file)) ? blanked(sourceOf(crate.cleanup.file)) : "";
  const from = cleanupSource.indexOf(crate.cleanup.open);
  const items = from < 0 ? null : cleanupSource.slice(from + crate.cleanup.open.length, cleanupSource.indexOf(crate.cleanup.close, from + crate.cleanup.open.length)).match(crate.cleanup.item) ?? [];
  if (items === null) fail(crate.cleanup.file, `the list of exceptions with something to undo was not found (\`${crate.cleanup.open}\`)`);
  const named = new Set();
  for (const [id, exception] of Object.entries(exceptions)) {
    const where = `${crate.registration}: exception ${id}`;
    if (!entries.some((entry) => entry.exceptions?.includes(id))) fail(where, "is the exception of no entry: remove it");
    if (!exception.approved) fail(where, "does not say what the user approved");
    const doc = exception.doc ?? {};
    if (!doc.file || !existsSync(path.join(root, doc.file))) fail(where, "names no product doc that exists");
    else {
      if (!heading(sourceOf(doc.file), doc.section)) fail(where, `names a section \`${doc.section}\` that ${doc.file} does not have`);
      // A change to the document has to run this check, or renaming the section passes its own push.
      if (!matchingRules(doc.file).some((rule) => rule.consumers.length > 0)) fail(where, `names ${doc.file}, which no rule of scripts/verify/scope.mjs gives a consumer: a change to it would run no check`);
    }
    if ((exception.cleanup === undefined) === (exception.nothing_to_undo === undefined)) fail(where, "has to name either its `cleanup` item or why there is `nothing_to_undo`");
    else if (exception.cleanup !== undefined) {
      named.add(exception.cleanup);
      if (items !== null && !items.includes(exception.cleanup)) fail(where, `has something to undo, and \`${exception.cleanup}\` is not in the clean-up's list in ${crate.cleanup.file}`);
    }
  }
  // An item the clean-up lists is an exception: one the registration does not name was added without it.
  for (const item of items ?? []) {
    if (!named.has(item)) fail(crate.cleanup.file, `clears \`${item}\`, which no exception of ${crate.registration} names as its \`cleanup\``);
  }
  return failures;
}

export const checkRegisteredWrites = (root, crates = CRATES) => crates.flatMap((crate) => checkCrate(root, crate));

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const failures = checkRegisteredWrites(process.cwd());
  for (const failure of failures) console.error(`${failure.file}${failure.line ? `:${failure.line}` : ""}: ${failure.text}`);
  if (failures.length) {
    console.error("\nA write outside a registered file: move it behind a registered entry point, or register the file in its crate's registered_writes.json. A new `exception` needs the user's approval (docs/conventions/principles.md, P9).");
    process.exitCode = 1;
  } else console.log("Registered writes check passed.");
}
