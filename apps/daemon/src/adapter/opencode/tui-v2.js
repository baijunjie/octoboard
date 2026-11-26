// The TUI can navigate independently of the server. Give delivery the id it actually shows.
import { renameSync, writeFileSync } from "node:fs";

export default {
  id: "octoboard-attachment",
  // Total: a TUI plugin interface that changed turns this one off, and delivery falls back to
  // the terminal's input, as it does with no selection known.
  setup(ctx) {
    try {
      const route = new URL("./attached-session", import.meta.url);
      const pending = new URL("./attached-session.next", import.meta.url);
      let previous;
      const publish = (id) => {
        if (id === previous) return;
        try {
          writeFileSync(pending, id, { mode: 0o600 });
          renameSync(pending, route);
          previous = id;
        } catch {}
      };
      const read = () => {
        const current = ctx.ui.router.current();
        return current.type === "session" ? current.sessionID : "";
      };
      // A route that cannot be read turns the plugin off before its timer starts.
      const first = read();
      if (typeof first !== "string") return () => {};
      publish(first);
      const update = () => {
        try {
          publish(read());
        } catch {}
      };
      const timer = setInterval(update, 50);
      return () => {
        clearInterval(timer);
        publish("");
      };
    } catch {
      return () => {};
    }
  },
};
