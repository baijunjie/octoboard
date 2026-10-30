// Creates the fixtures the driver expects on a throwaway daemon, over its control WebSocket: three
// consoles ("Console 1".."Console 3"), a project in each (p1, p2, p3) with two stand-in Claude sessions
// (p<N>-s1, p<N>-s2), and optionally more projects with idle sessions to give the sidebar a realistic
// size. The stand-in's behaviour per project comes from `<home>/code/<project>/.standin.json`.
//
//   node fixtures.mjs <port> <key> <home> [extraProjects=5] [extraSessions=3]
//
// Only ever point it at a throwaway daemon: it creates consoles and sessions and trusts folders.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const MAIN_PROJECTS = [
  { name: "p1", standin: { lines: 30000, spinner_secs: 8 } },
  { name: "p2", standin: { lines: 30000, spinner_secs: 8 } },
  { name: "p3", standin: { lines: 30000, spinner_secs: 8 } },
];
// The extra projects only fill the sidebar; a short transcript and a short spinner leave them idle.
const EXTRA_STANDIN = { lines: 2000, spinner_secs: 3 };
const TIMEOUT_MS = 60_000;

function makeProject(home, name, standin) {
  const dir = path.join(home, "code", name);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, ".standin.json"), JSON.stringify(standin));
  return dir;
}

export async function seedFixtures({ port, key, home, extraProjects = 5, extraSessions = 3, log = console.log }) {
  if (!key) throw new Error("seedFixtures needs the daemon's key");
  const ws = new WebSocket(`ws://127.0.0.1:${port}/ws/control?key=${key}`);
  const replies = new Map();
  const waiters = new Set();
  let nextId = 0;
  const send = (body) => {
    const id = `f${nextId++}`;
    ws.send(JSON.stringify({ ...body, id }));
    return id;
  };
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.type === "trust_prompt") send({ type: "confirm_trust", session: msg.session, remember: true });
    if (msg.id) replies.get(msg.id)?.(msg);
    for (const w of [...waiters]) if (w.match(msg)) { waiters.delete(w); w.resolve(msg); }
  };
  const waitFor = (match, ms = TIMEOUT_MS) => new Promise((resolve, reject) => {
    const w = { match, resolve };
    waiters.add(w);
    setTimeout(() => { if (waiters.delete(w)) reject(new Error("timed out waiting for a daemon event")); }, ms).unref();
  });
  const request = (body) => new Promise((resolve, reject) => {
    const id = send(body);
    replies.set(id, resolve);
    setTimeout(() => reject(new Error(`no reply to ${body.type}`)), TIMEOUT_MS).unref();
  }).then((reply) => {
    if (reply.type === "error") throw new Error(`${body.type} failed: ${JSON.stringify(reply).slice(0, 200)}`);
    return reply;
  });
  const addProject = async (consoleId, name, standin) => {
    const upserted = waitFor((m) => m.type === "project_upserted" && m.project.console_id === consoleId && m.project.name === name);
    await request({ type: "add_project", console_id: consoleId, source: "local", path: makeProject(home, name, standin), name });
    return (await upserted).project;
  };

  try {
    // The listener is up before the daemon's snapshot arrives; wait for it, and for the agent probe that
    // makes `claude` selectable (it may already be inside the snapshot).
    const snapshot = waitFor((m) => m.type === "snapshot");
    await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = () => reject(new Error(`cannot connect to the daemon on port ${port}`)); });
    const probed = (await snapshot).agent_availability?.some((a) => a.agent === "claude" && a.availability !== "not_determined");
    if (!probed) await waitFor((m) => m.type === "agent_availability_updated", 15_000).catch(() => {});

    for (const [i, { name, standin }] of MAIN_PROJECTS.entries()) {
      const created = waitFor((m) => m.type === "console_upserted" && m.console.name === `Console ${i + 1}`);
      await request({ type: "create_console", name: `Console ${i + 1}`, default_agent: "claude" });
      const console_ = (await created).console;
      const project = await addProject(console_.id, name, standin);
      for (let k = 1; k <= 2; k++) {
        await request({ type: "open_session", console_id: console_.id, project_id: project.id, agent: "claude", title: `${name}-s${k}` });
      }
      for (let j = 1; j <= extraProjects; j++) {
        const extra = await addProject(console_.id, `c${i + 1}x${j}`, EXTRA_STANDIN);
        for (let k = 1; k <= extraSessions; k++) {
          await request({ type: "open_session", console_id: console_.id, project_id: extra.id, agent: "claude", title: `c${i + 1}x${j}-s${k}` });
        }
      }
      log(`console ${i + 1}: ${name} with 2 sessions, ${extraProjects} extra projects`);
    }
    // Trust prompts arrive as the agents start, and the replays are only final once the spinners have
    // stopped; give the last ones time for both.
    const spinnerSecs = Math.max(...MAIN_PROJECTS.map((p) => p.standin.spinner_secs));
    await new Promise((resolve) => setTimeout(resolve, (spinnerSecs + 2) * 1000));
  } finally {
    ws.close();
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [port, key, home, extraProjects, extraSessions] = process.argv.slice(2);
  if (!port || !key || !home) {
    console.error("usage: node fixtures.mjs <port> <key> <home> [extraProjects=5] [extraSessions=3]");
    process.exit(2);
  }
  await seedFixtures({ port, key, home, extraProjects: extraProjects === undefined ? 5 : Number(extraProjects), extraSessions: extraSessions === undefined ? 3 : Number(extraSessions) });
}
