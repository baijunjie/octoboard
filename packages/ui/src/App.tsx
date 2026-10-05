import { Button, Chip } from "@heroui/react";
import React from "react";

import { useAppExit } from "./lifecycle/useAppExit";
import { useWaitingNotifications } from "./lifecycle/useWaitingNotifications";
import { useDaemon, useDaemonStore } from "./store";
import { STATUS_LABEL } from "./sessionLabel";

// TODO: this screen is a placeholder that shows the daemon's state is arriving. It is replaced by
// the HeroUI UI rewrite plan's milestone 02 (core screens).
export function App(): React.ReactElement {
  const { request, toastError, dismissToast, reconnect } = useDaemon();
  const connectionState = useDaemonStore((s) => s.connectionState);
  const hosts = useDaemonStore((s) => s.hosts);
  const toasts = useDaemonStore((s) => s.toasts);
  const consoles = useDaemonStore((s) => s.consoles);
  const projects = useDaemonStore((s) => s.projects);
  const sessions = useDaemonStore((s) => s.sessions);

  useWaitingNotifications(Array.from(sessions.values()), consoles, projects, hosts !== undefined);

  // The exit flow's frontend owner: it registers with the shell on mount, which is what makes the
  // window quittable; the confirmation dialog it can ask for comes with the real screens.
  useAppExit({
    getSessions: () => Array.from(sessions.values()),
    requestShutdown: () => request({ type: "shutdown" }),
    toastError,
  });

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex items-center gap-3">
        <h1 className="text-xl font-semibold">Octoboard</h1>
        <Chip color={connectionState === "open" ? "success" : connectionState === "closed" ? "danger" : "warning"}>
          {connectionState}
        </Chip>
        {connectionState === "closed" && <Button onPress={reconnect}>Retry</Button>}
      </header>

      {toasts.map((toast) => (
        <div key={toast.id} className="flex items-center gap-2">
          <Chip color={toast.kind === "error" ? "danger" : "default"}>{toast.kind}</Chip>
          <span>{toast.message}</span>
          <Button size="sm" onPress={() => dismissToast(toast.id)}>
            Dismiss
          </Button>
        </div>
      ))}

      {!hosts ? (
        <p>{connectionState === "closed" ? "The daemon is not answering." : "Connecting to the daemon…"}</p>
      ) : (
        <ul className="flex flex-col gap-3">
          {Array.from(consoles.values()).map((console_) => (
            <li key={console_.id}>
              <div className="font-medium">{console_.name}</div>
              <ul className="ml-4 flex flex-col gap-1">
                {Array.from(projects.values())
                  .filter((p) => p.console_id === console_.id)
                  .map((project) => (
                    <li key={project.id}>
                      <div>{project.name}</div>
                      <ul className="ml-4">
                        {Array.from(sessions.values())
                          .filter((s) => s.project_id === project.id)
                          .map((session) => (
                            <li key={session.id} className="flex items-center gap-2">
                              <span>{session.title}</span>
                              <Chip size="sm">{STATUS_LABEL[session.status]}</Chip>
                            </li>
                          ))}
                      </ul>
                    </li>
                  ))}
                {Array.from(sessions.values())
                  .filter((s) => s.console_id === console_.id && !s.project_id)
                  .map((session) => (
                    <li key={session.id} className="flex items-center gap-2">
                      <span>{session.title} (hub)</span>
                      <Chip size="sm">{STATUS_LABEL[session.status]}</Chip>
                    </li>
                  ))}
              </ul>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
