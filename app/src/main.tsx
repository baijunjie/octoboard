import React from "react";
import ReactDOM from "react-dom/client";

import { App } from "./App";
import { ErrorBoundary } from "./ErrorBoundary";
import { resolveDaemonPort, resolveStartupError, daemonWsUrl } from "./daemon";
import { StartupScreen } from "./StartupScreen";
import { DaemonProvider } from "./store";
import "./style.css";

const rootEl = document.getElementById("root")!;
const root = ReactDOM.createRoot(rootEl);

const startupError = resolveStartupError();
const port = resolveDaemonPort();

function screen(): React.ReactElement {
  if (startupError) {
    // The daemon failed to start before the window even opened, and the shell passed the message
    // along via `?error=` instead of `?port=` — there is nothing to connect to, same as the
    // missing-port case below, just with a specific reason instead of a generic one.
    return <StartupScreen message={`The daemon failed to start: ${startupError}`} />;
  }
  if (!port) {
    // No Tauri shell handed us a `?port=`, and no `VITE_DAEMON_PORT` was set for a bare `vite dev`
    // session — there is nothing to connect to, so say so instead of guessing a port.
    return (
      <StartupScreen
        message={
          <>
            No daemon port known. The Tauri shell passes one via <code>?port=</code>; for{" "}
            <code>vite dev</code> against a daemon started by hand, set <code>VITE_DAEMON_PORT</code>.
          </>
        }
      />
    );
  }
  return (
    <DaemonProvider url={daemonWsUrl(port, "/ws/control")}>
      <App port={port} />
    </DaemonProvider>
  );
}

// One render for every screen, so none of them is outside the boundary.
root.render(
  <React.StrictMode>
    <ErrorBoundary>{screen()}</ErrorBoundary>
  </React.StrictMode>,
);
