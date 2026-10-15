// Generates daemon broadcasts the way a busy board does: renames the extra-project sessions of one
// console in turn (each a `session_upserted`) every <intervalMs> for <seconds>, then restores the titles.
// SIGTERM or SIGINT ends the renaming early; the titles are restored all the same.
//
//   node event-pump.mjs <port> <consoleName> <intervalMs> <seconds>
const [port, consoleName, intervalArg, secondsArg] = process.argv.slice(2);
if (!port || !consoleName || !intervalArg || !secondsArg) {
  console.error("usage: node event-pump.mjs <port> <consoleName> <intervalMs> <seconds>");
  process.exit(2);
}
const replies = new Map();
const ws = new WebSocket(`ws://127.0.0.1:${port}/ws/control`);
let snapshot;
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.type === "snapshot") snapshot = m;
  if (m.id) replies.get(m.id)?.(m);
};
await new Promise((resolve) => (ws.onopen = resolve));
while (!snapshot) await new Promise((resolve) => setTimeout(resolve, 50));
const target = snapshot.consoles.find((c) => c.name === consoleName);
// The main projects' sessions ("p1-s1") are the ones the scenarios drive; only the others are renamed.
const sessions = snapshot.sessions.filter((s) => s.console_id === target?.id && !/^p\d-s\d$/.test(s.title));
if (sessions.length === 0) {
  console.error(`console "${consoleName}" has no extra-project session to rename`);
  process.exit(1);
}
const titles = new Map(sessions.map((s) => [s.id, s.title]));
let sent = 0;
let stopping = false;
for (const signal of ["SIGTERM", "SIGINT"]) process.on(signal, () => (stopping = true));
const end = Date.now() + Number(secondsArg) * 1000;
while (!stopping && Date.now() < end) {
  const s = sessions[sent % sessions.length];
  ws.send(JSON.stringify({ type: "rename_session", session: s.id, title: `${titles.get(s.id)} · ${sent}`, id: `pump${sent}` }));
  sent++;
  await new Promise((resolve) => setTimeout(resolve, Number(intervalArg)));
}
// Put back exactly the titles found, and wait for the daemon to accept each.
const restored = sessions.map((s) => new Promise((resolve) => {
  const id = `restore-${s.id}`;
  const timer = setTimeout(() => resolve(`no reply for ${s.id}`), 5000);
  replies.set(id, (m) => { clearTimeout(timer); resolve(m.type === "error" ? `refused for ${s.id}: ${JSON.stringify(m).slice(0, 200)}` : null); });
  ws.send(JSON.stringify({ type: "rename_session", session: s.id, title: titles.get(s.id), id }));
}));
const failures = (await Promise.all(restored)).filter(Boolean);
ws.close();
console.log(`events sent: ${sent}`);
if (failures.length) {
  console.error(`could not restore ${failures.length} title(s): ${failures.join("; ")}`);
  process.exit(1);
}
