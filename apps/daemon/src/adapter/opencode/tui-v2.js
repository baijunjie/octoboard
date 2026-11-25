// The TUI can navigate independently of the server. Give delivery the id it actually shows.
import { renameSync, writeFileSync } from "node:fs";

export default {
  id: "octoboard-attachment",
  setup(ctx) {
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
    const update = () => {
      const current = ctx.ui.router.current();
      publish(current.type === "session" ? current.sessionID : "");
    };
    update();
    const timer = setInterval(update, 50);
    return () => {
      clearInterval(timer);
      publish("");
    };
  },
};
