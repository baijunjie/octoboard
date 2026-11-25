// The server's event bus cannot see navigation in the attached terminal. Publish the TUI's
// current route into its launch scratch directory so delivery follows the conversation it shows.
import { renameSync, writeFileSync } from "node:fs";

export default {
  id: "octoboard-attachment",
  tui: async (api) => {
    const route = new URL("./attached-session", import.meta.url);
    const pending = new URL("./attached-session.next", import.meta.url);
    let previous;
    const update = () => {
      const current = api.route.current;
      const id = current.name === "session" ? current.params.sessionID : "";
      if (id === previous) return;
      try {
        writeFileSync(pending, id, { mode: 0o600 });
        renameSync(pending, route);
        previous = id;
      } catch {}
    };
    update();
    const timer = setInterval(update, 50);
    api.lifecycle.onDispose(() => {
      clearInterval(timer);
      try {
        writeFileSync(pending, "", { mode: 0o600 });
        renameSync(pending, route);
      } catch {}
    });
  },
};
