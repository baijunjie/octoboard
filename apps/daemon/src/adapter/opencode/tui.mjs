// The server's event bus cannot see navigation in the attached terminal. Publish the TUI's
// current route into its launch scratch directory so delivery follows the conversation it shows.
import { renameSync, writeFileSync } from "node:fs";

export default {
  id: "octoboard-attachment",
  // Total: a TUI plugin interface that changed turns this one off, and delivery falls back to
  // the terminal's input, as it does with no selection known.
  tui: async (api) => {
    try {
      const route = new URL("./attached-session", import.meta.url);
      const pending = new URL("./attached-session.next", import.meta.url);
      let previous;
      const update = () => {
        try {
          const current = api.route.current;
          const id = current.name === "session" ? current.params.sessionID : "";
          if (id === previous) return;
          writeFileSync(pending, id, { mode: 0o600 });
          renameSync(pending, route);
          previous = id;
        } catch {}
      };
      // A route that cannot be read turns the plugin off before its timer starts.
      const current = api.route.current;
      if (typeof current?.name !== "string") return;
      update();
      // The dispose handler is registered before the timer starts, so a lifecycle interface that
      // throws leaves no timer running behind the plugin turned off.
      let timer;
      api.lifecycle.onDispose(() => {
        clearInterval(timer);
        try {
          writeFileSync(pending, "", { mode: 0o600 });
          renameSync(pending, route);
        } catch {}
      });
      timer = setInterval(update, 50);
    } catch {}
  },
};
