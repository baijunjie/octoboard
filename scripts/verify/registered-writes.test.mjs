import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { blanked, bodiesOf, checkRegisteredWrites, CRATES, hitsIn, KINDS, productionFiles, publicFunctions, REGISTERED_NAME, VOCABULARY } from "./registered-writes.mjs";

const [DAEMON, DESKTOP] = CRATES;
const DOC = { file: "docs/product/launching-agents.md", section: "What Octoboard never modifies" };

// A repository with both crates, each with one registered helper file, one exception with something to undo and
// its clean-up item. `change` edits the tree before it is checked.
function tree(t, change = () => {}) {
  const root = mkdtempSync(path.join(os.tmpdir(), "registered-writes-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const put = (file, text) => {
    mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    writeFileSync(path.join(root, file), typeof text === "string" ? text : JSON.stringify(text));
  };
  const registration = (cleanup) => ({
    exceptions: { sharing: { approved: "Sharing.", doc: DOC, cleanup } },
    entries: [
      { path: "helpers/mod.rs", kind: "primitive", writes: "Where its caller says.", readers: ["size_of"] },
      { path: "sharing.rs", kind: "exception", exceptions: ["sharing"], writes: "Links." },
    ],
  });
  const registrations = { daemon: registration("sharing::EXCEPTION"), desktop: registration("sharing::Links") };
  put(DOC.file, `# Launching agents\n\n## ${DOC.section}\n`);
  for (const crate of CRATES) {
    put(`${crate.source}/main.rs`, "mod helpers;\nmod sharing;\n#[cfg(test)]\nmod tests;\nfn main() { sharing::link(); }\n");
    put(`${crate.source}/helpers/mod.rs`, "pub fn replace_whole(a: &Path, b: &Path) { std::fs::rename(a, b).unwrap(); }\npub fn size_of(a: &Path) -> u64 { a.metadata().unwrap().len() }\n");
    put(`${crate.source}/sharing.rs`, "pub fn link() { std::os::unix::fs::symlink(a, b).unwrap(); helpers::replace_whole(a, b); }\n");
    put(`${crate.source}/tests.rs`, "fn scratch() { std::fs::write(a, b).unwrap(); helpers::replace_whole(a, b); }\n");
  }
  put(DAEMON.cleanup.file, `${DAEMON.cleanup.open}sharing::EXCEPTION${DAEMON.cleanup.close}\n`);
  put(DESKTOP.cleanup.file, `${DESKTOP.cleanup.open}\n    &sharing::Links,\n${DESKTOP.cleanup.close}\n`);
  change({ put, registrations, root });
  for (const crate of CRATES) if (registrations[crate.name] !== null) put(crate.registration, registrations[crate.name]);
  return root;
}

const texts = (t, change) => checkRegisteredWrites(tree(t, change)).map((failure) => `${failure.file}${failure.line ? `:${failure.line}` : ""}: ${failure.text}`);
const writes = (source, names) => hitsIn(source, names).filter((hit) => hit.reason === undefined);

test("every word of the vocabulary matches each form it takes, and nothing that only reads", () => {
  for (const { word, pattern, examples, why } of VOCABULARY) {
    assert.ok(why, word);
    assert.ok(examples.length > 0, word);
    for (const example of examples) {
      // The word's own pattern finds it, whichever word a line of several is listed under.
      assert.ok(new RegExp(pattern.source).test(blanked(example)), `${word}: ${example}`);
      assert.equal(writes(example).length, 1, `${word}: ${example}`);
    }
  }
  for (const reading of [
    "let text = std::fs::read_to_string(path)?;",
    "let file = File::open(path)?;",
    "for entry in std::fs::read_dir(dir)? {}",
    "let metadata = fs::symlink_metadata(path)?;",
    "let null = OpenOptions::new().read(true).open(name)?;",
    "let mut options = OpenOptions::new();",
    "let file = File::options().read(true).write(false).open(path)?;",
    "let dir = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) };",
    "let handle = CreateFileW(name, GENERIC_READ, FILE_SHARE_READ, null(), OPEN_EXISTING, 0, null_mut());",
    "entries.truncate(len);",
    "output.append(&mut held);",
    "stream.write(buffer)?;",
    "use crate::platform::fs::{self as platform, Links};",
    "let wide = value as u32;",
    "let list = CommitList { commits };",
    "return Err(crate::accounts::trash::being_removed(&removing));",
    "let name = facts.trash;",
  ]) assert.deepEqual(hitsIn(reading), [], reading);
});

test("a hit names its line; comments, strings and test-only items are not hits", () => {
  const source = `// std::fs::write(a, b)
let text = "std::fs::remove_file(path)";
#[cfg(test)]
mod tests { fn scratch() { std::fs::create_dir_all(dir).unwrap(); } }
#[cfg(all(test, unix))]
fn fixture() { std::fs::write(a, b).unwrap(); }
#[cfg(any(test, unix))]
fn production() { std::fs::write(a, b).unwrap(); }`;
  assert.deepEqual(hitsIn(source).map((hit) => [hit.line, hit.word]), [[8, "fs function"]]);
  assert.deepEqual(hitsIn("fs\n    ::rename(a, b)").map((hit) => hit.line), [1]);
});

test("a function the registration names is a word where it is called, not where one of its name is declared", () => {
  assert.deepEqual(writes("let script = hooks::write_hook_script(&home, exe)?;", ["write_hook_script"]).map((hit) => hit.word), [REGISTERED_NAME]);
  assert.deepEqual(writes("command.run_to_file (&log)", ["run_to_file"]).length, 1);
  assert.deepEqual(writes("pub(super) fn bind_replacing_stale(_path: &Path) -> io::Result<Listener> { unsupported() }", ["bind_replacing_stale"]), []);
  assert.deepEqual(writes("let script = hooks::write_hook_script(&home, exe)?;"), []);
});

test("a line that opens a device or a pipe is acknowledged with its reason, on the line or right above it", () => {
  const acknowledged = (source) => hitsIn(source).map((hit) => [hit.line, hit.reason]);
  assert.deepEqual(acknowledged('// no-file-write: the null device.\nlet null = File::options().write(true).open("NUL")?;'), [[2, "the null device."]]);
  assert.deepEqual(acknowledged("let flags = libc::O_RDWR; // no-file-write: a pseudoterminal."), [[1, "a pseudoterminal."]]);
  assert.deepEqual(acknowledged("OpenOptions::new()\n    .read(true)\n    // A named pipe:\n    // no-file-write: it is only connected to.\n    // Both ways.\n    .write(true)"), [[6, "it is only connected to."]]);
  // Not from further up than the comment lines right above, and not from a string.
  assert.deepEqual(acknowledged("// no-file-write: the null device.\nlet a = 1;\nstd::fs::write(a, b)?;"), [[3, undefined]]);
  assert.deepEqual(acknowledged('let text = "no-file-write: x"; std::fs::write(a, b)?;'), [[1, undefined]]);
});

test("an acknowledgement without a reason, or of a line that writes nothing, fails", (t) => {
  const planted = (source) => texts(t, ({ put }) => put(`${DAEMON.source}/platform/term.rs`, source));
  assert.deepEqual(planted("fn open() {\n    // no-file-write: a pseudoterminal's slave end.\n    let flags = libc::O_RDWR;\n}\n"), []);
  assert.match(planted("fn open() {\n    // no-file-write:\n    let flags = libc::O_RDWR;\n}\n")[0], /term\.rs:2: an acknowledgement says why the line writes no file/);
  assert.match(planted("fn open() {\n    // no-file-write: a pseudoterminal.\n    let flags = libc::O_RDONLY;\n}\n")[0], /term\.rs:2: acknowledges a write the vocabulary does not find/);
  // One acknowledgement is one line's: the write after it is still unregistered.
  assert.match(planted("fn open() {\n    // no-file-write: a pseudoterminal.\n    let flags = libc::O_RDWR;\n    std::fs::write(a, b).unwrap();\n}\n")[0], /term\.rs:4: unregistered write \(fs function\)/);
});

test("a registered write passes and a planted one fails with its file and line", (t) => {
  assert.deepEqual(texts(t), []);
  assert.deepEqual(
    texts(t, ({ put }) => put(`${DAEMON.source}/session/launch.rs`, "fn launch() {\n    std::fs::write(home.join(\".profile\"), text).unwrap();\n}\n")),
    [`${DAEMON.source}/session/launch.rs:2: unregistered write (fs function): std::fs::write(home.join(".profile"), text).unwrap();`],
  );
  assert.match(texts(t, ({ put }) => put(`${DESKTOP.source}/menu.rs`, "fn build() { RegSetKeyValueW(key); }\n"))[0], /src\/menu\.rs:1: unregistered write \(registry\)/);
});

test("every public function of a primitive file is a word, so a call of one from an unregistered file fails", (t) => {
  assert.deepEqual(publicFunctions("pub fn a() {}\npub(crate) async fn b() {}\npub(in crate::x) const unsafe fn c() {}\nfn d() {}\nimpl T { pub fn e(&self) {} }\n#[cfg(test)]\npub fn f() {}\n"), ["a", "b", "c", "e"]);
  assert.match(texts(t, ({ put }) => put(`${DAEMON.source}/adapter/hooks.rs`, "fn script(home: &Path) { helpers::replace_whole(home, other); }\n"))[0], /adapter\/hooks\.rs:1: unregistered write \(registered function\)/);
  // A new public function of the file is a word at once, with nothing added to the registration.
  assert.match(
    texts(t, ({ put }) => {
      put(`${DAEMON.source}/helpers/mod.rs`, "pub fn replace_whole(a: &Path, b: &Path) { std::fs::rename(a, b).unwrap(); }\npub fn size_of(a: &Path) -> u64 { 0 }\npub(crate) fn write_script(a: &Path) { replace_whole(a, a) }\n");
      put(`${DAEMON.source}/adapter/hooks.rs`, "fn script(home: &Path) { helpers::write_script(home); }\n");
    })[0],
    /adapter\/hooks\.rs:1: unregistered write \(registered function\): fn script\(home: &Path\) \{ helpers::write_script\(home\); \}/,
  );
  // A reader is not a word.
  assert.deepEqual(texts(t, ({ put }) => put(`${DAEMON.source}/adapter/hooks.rs`, "fn size(home: &Path) -> u64 { helpers::size_of(home) }\n")), []);
});

test("a reader of a primitive file is one the file declares and whose body writes nothing", (t) => {
  assert.deepEqual(bodiesOf("fn a();\n#[cfg(unix)]\nfn a() { one() }\n#[cfg(windows)]\nfn a() { if x { two() } }\nfn b() {}", "a"), ["{ one() }", "{ if x { two() } }"]);
  const readers = (list, source) => texts(t, ({ put, registrations }) => {
    registrations.daemon.entries[0].readers = list;
    if (source) put(`${DAEMON.source}/helpers/mod.rs`, source);
  }).join("\n");
  assert.match(readers(["size_of", "gone"]), /helpers\/mod\.rs: lists the reader `gone`, which is no public function of the file/);
  assert.match(readers(["size_of", "replace_whole"]), /helpers\/mod\.rs: lists `replace_whole` as a reader, and it writes \(fs function\)/);
  assert.match(
    readers(["probe"], "pub fn replace_whole(a: &Path, b: &Path) { std::fs::rename(a, b).unwrap(); }\npub fn probe(a: &Path) -> bool { replace_whole(a, a); true }\n"),
    /lists `probe` as a reader, and it writes \(registered function\)/,
  );
  assert.match(texts(t, ({ registrations }) => { registrations.daemon.entries[1].readers = ["link"]; })[0], /sharing\.rs: a `primitive` entry lists its `readers`, any other its `entry_points`/);
});

test("an entry point another entry names is a word, and is its file's write where no other shows", (t) => {
  const change = ({ put, registrations }) => {
    put(`${DESKTOP.source}/remote/keys.rs`, "pub async fn change_keys(session: &Session) { session.run(SCRIPT).await }\n");
    registrations.desktop.entries.push({ path: "remote/keys.rs", kind: "exception", exceptions: ["sharing"], writes: "A line on the host.", entry_points: ["change_keys"] });
  };
  assert.deepEqual(texts(t, change), []);
  assert.match(
    texts(t, (given) => { change(given); given.put(`${DESKTOP.source}/remote/connect.rs`, "async fn connect() { keys::change_keys(&session).await }\n"); })[0],
    /remote\/connect\.rs:1: unregistered write \(registered function\)/,
  );
  assert.match(texts(t, (given) => { change(given); given.registrations.desktop.entries[2].entry_points = ["remove_keys"]; })[0], /names the entry point `remove_keys`, which the file does not declare/);
  assert.match(texts(t, (given) => { change(given); given.registrations.desktop.entries[0].entry_points = ["replace_whole"]; })[0], /a `primitive` entry lists its `readers`, any other its `entry_points`/);
});

test("a file is test code only where cfg(test) proves it, not for its name", (t) => {
  assert.match(texts(t, ({ put }) => put(`${DAEMON.source}/fixture.rs`, "pub fn scratch() { std::fs::write(a, b).unwrap(); }\n"))[0], /fixture\.rs:1: unregistered write/);
  const root = tree(t, ({ put }) => {
    put(`${DAEMON.source}/session/mod.rs`, "#[cfg(test)]\npub(crate) mod fixture;\n#[cfg(all(test, unix))]\nmod capture;\n");
    put(`${DAEMON.source}/session/fixture.rs`, "pub fn scratch() { std::fs::write(a, b).unwrap(); }\n");
    put(`${DAEMON.source}/session/capture/child.rs`, "pub fn scratch() { std::fs::write(a, b).unwrap(); }\n");
  });
  assert.deepEqual(checkRegisteredWrites(root), []);
  assert.deepEqual(productionFiles(root, DAEMON.source).map((file) => file.slice(DAEMON.source.length + 1)), ["coordinator/system_changes.rs", "helpers/mod.rs", "main.rs", "session/mod.rs", "sharing.rs"]);
});

test("an entry is one file that writes: never a directory, of any kind", (t) => {
  const entry = (extra) => ({ registrations }) => registrations.daemon.entries.push({ kind: "own", writes: "Its own files.", ...extra });
  for (const directory of ["helpers/", "helpers"]) assert.match(texts(t, entry({ path: directory })).join("\n"), /helpers\/?: names no file that exists \(a directory is not registered: each file that writes is\)/);
  assert.match(texts(t, entry({ path: "gone.rs" })).join("\n"), /gone\.rs: names no file that exists/);
  assert.match(texts(t, entry({ path: "main.rs" })).join("\n"), /main\.rs: holds no write any more/);
  // A new file beside a registered one is not registered with it.
  assert.match(texts(t, ({ put }) => put(`${DAEMON.source}/helpers/extra.rs`, "pub fn remove(a: &Path) { std::fs::remove_file(a).unwrap(); }\n"))[0], /helpers\/extra\.rs:1: unregistered write/);
});

test("the registration has to say what the code does", (t) => {
  const entry = (extra) => ({ registrations }) => registrations.daemon.entries.push({ kind: "own", writes: "Its own files.", ...extra });
  assert.match(texts(t, entry({ path: "sharing.rs" })).join("\n"), /sharing\.rs: is registered twice/);
  assert.match(texts(t, ({ registrations }) => { registrations.daemon.entries[0].kind = "harmless"; })[0], /helpers\/mod\.rs: has no known kind/);
  assert.match(texts(t, ({ registrations }) => { delete registrations.daemon.entries[0].writes; })[0], /does not say what it writes/);
  assert.match(texts(t, ({ put }) => put(`${DAEMON.source}/sharing.rs`, "pub fn link() {}\n"))[0], /sharing\.rs: holds no write any more/);
  assert.match(texts(t, ({ registrations }) => { registrations.desktop = "not a registration"; }).join("\n"), /^apps\/desktop\/src-tauri\/src\/registered_writes\.json: the registration is not JSON/);
  assert.deepEqual(texts(t, ({ registrations }) => { registrations.desktop = null; }), [`${DESKTOP.registration}: the registration is missing`]);
  assert.match(texts(t, ({ registrations }) => { registrations.desktop = {}; }).join("\n"), /src\/helpers\/mod\.rs:1: unregistered write[\s\S]*src\/sharing\.rs:1: unregistered write[\s\S]*clears `sharing::Links`/);
});

test("an exception is approved, documented and tied to its clean-up", (t) => {
  const exception = (edit) => ({ registrations }) => edit(registrations.daemon.exceptions.sharing, registrations.daemon);
  assert.match(texts(t, exception((_, daemon) => { delete daemon.entries[1].exceptions; })).join("\n"), /sharing\.rs: only an entry of the kind `exception` names exceptions[\s\S]*exception sharing: is the exception of no entry/);
  assert.match(texts(t, exception((_, daemon) => { daemon.entries[0].exceptions = ["sharing"]; }))[0], /helpers\/mod\.rs: only an entry of the kind `exception` names exceptions/);
  assert.match(texts(t, exception((_, daemon) => { daemon.entries[1].exceptions = ["other"]; })).join("\n"), /names the exception `other`, which the registration does not define/);
  assert.match(texts(t, exception((sharing) => { delete sharing.approved; }))[0], /does not say what the user approved/);
  assert.match(texts(t, exception((sharing) => { sharing.doc = { file: "docs/product/gone.md", section: DOC.section }; }))[0], /names no product doc that exists/);
  assert.match(texts(t, exception((sharing) => { sharing.doc = { ...DOC, section: "Launching" }; }))[0], /names a section `Launching`/);
  assert.match(texts(t, exception((sharing) => { sharing.nothing_to_undo = "Nothing."; }))[0], /either its `cleanup` item or why there is `nothing_to_undo`/);
  assert.match(texts(t, exception((sharing) => { delete sharing.cleanup; }))[0], /either its `cleanup` item or why there is `nothing_to_undo`/);
});

test("the document an exception names is one whose change runs this check", (t) => {
  const failures = texts(t, ({ put, registrations }) => {
    put("docs/product/sharing.md", `# Sharing\n\n## ${DOC.section}\n`);
    registrations.daemon.exceptions.sharing.doc = { file: "docs/product/sharing.md", section: DOC.section };
  });
  assert.equal(failures.length, 1);
  assert.match(failures[0], /exception sharing: names docs\/product\/sharing\.md, which no rule of scripts\/verify\/scope\.mjs gives a consumer/);
});

test("an exception with something to undo has its item in the clean-up's list, and the list no other", (t) => {
  assert.match(texts(t, ({ put }) => put(DAEMON.cleanup.file, `${DAEMON.cleanup.open}${DAEMON.cleanup.close}\n`))[0], /exception sharing: has something to undo, and `sharing::EXCEPTION` is not in the clean-up's list/);
  assert.match(texts(t, ({ put }) => put(DAEMON.cleanup.file, "pub fn list() {}\n")).join("\n"), /system_changes\.rs: the list of exceptions with something to undo was not found/);
  assert.match(
    texts(t, ({ put }) => put(DESKTOP.cleanup.file, `${DESKTOP.cleanup.open}\n    &sharing::Links,\n    &proxy::Setting,\n${DESKTOP.cleanup.close}\n`))[0],
    /system_changes\.rs: clears `proxy::Setting`, which no exception of .*registered_writes\.json names as its `cleanup`/,
  );
  // An exception whose undoing would lose the user's data says so in place of an item.
  assert.deepEqual(texts(t, ({ put, registrations }) => {
    put(DAEMON.cleanup.file, `${DAEMON.cleanup.open}${DAEMON.cleanup.close}\n`);
    delete registrations.daemon.exceptions.sharing.cleanup;
    registrations.daemon.exceptions.sharing.nothing_to_undo = "The history is the user's own.";
  }), []);
});

test("every kind says why it exists", () => {
  for (const [kind, why] of Object.entries(KINDS)) assert.ok(why.length > 20, kind);
});

test("the executable passes a registered tree and fails on a planted write", (t) => {
  const script = fileURLToPath(new URL("./registered-writes.mjs", import.meta.url));
  const passed = spawnSync(process.execPath, [script], { cwd: tree(t), encoding: "utf8" });
  assert.equal(passed.status, 0, passed.stderr);
  assert.match(passed.stdout, /Registered writes check passed/);
  const root = tree(t, ({ put }) => put(`${DAEMON.source}/session/launch.rs`, "fn launch() { tokio::fs::remove_dir_all(home).await.unwrap(); }\n"));
  const failed = spawnSync(process.execPath, [script], { cwd: root, encoding: "utf8" });
  assert.equal(failed.status, 1);
  assert.match(failed.stderr, /session\/launch\.rs:1: unregistered write \(fs function\)/);
});
