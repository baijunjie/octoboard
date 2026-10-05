import React from "react";
import ReactDOM from "react-dom/client";

import { App } from "./App";
import { resolveDaemonOrigin, resolveStartupError } from "./daemon";
import { ErrorBoundary } from "./ErrorBoundary";
import { selectPlatform } from "./platform";
import { PlatformProvider } from "./platform/react";
import { StartupScreen } from "./StartupScreen";
import { createDaemon, DaemonProvider } from "./store";
import "./style.css";

const rootEl = document.getElementById("root")!;
const root = ReactDOM.createRoot(rootEl);

const platform = selectPlatform();
const startupError = resolveStartupError();
const daemonOrigin = resolveDaemonOrigin(platform);
// Created here rather than by a component: the connection lives as long as the window, and a
// component that owned it would drop it, for good, on its first unmount (StrictMode's included).
const daemon = !startupError && daemonOrigin ? createDaemon(daemonOrigin) : undefined;

function screen(): React.ReactElement {
  if (startupError) {
    // The daemon failed to start before the window even opened, and the shell passed the message
    // along via `?error=` instead of `?port=` — there is nothing to connect to, same as the
    // missing-address case below, just with a specific reason instead of a generic one.
    return <StartupScreen message={`The daemon failed to start: ${startupError}`} />;
  }
  if (!daemon) {
    // No shell handed us a `?port=`, no `VITE_DAEMON_PORT` was set, and the page's own origin is
    // not one that can be the daemon — there is nothing to connect to, so say so instead of
    // guessing a port.
    return (
      <StartupScreen
        message={
          <>
            No daemon address known. The Tauri shell passes a port via <code>?port=</code>; for{" "}
            <code>vite dev</code> against a daemon started by hand, open the page with{" "}
            <code>?port=</code>, set <code>VITE_DAEMON_PORT</code>, or start the dev server with{" "}
            <code>OCTOBOARD_DAEMON_PORT</code> to proxy to it.
          </>
        }
      />
    );
  }
  return (
    <DaemonProvider value={daemon}>
      <App />
    </DaemonProvider>
  );
}

// One render for every screen, so none of them is outside the boundary.
root.render(
  <React.StrictMode>
    <PlatformProvider value={platform}>
      <ErrorBoundary>{screen()}</ErrorBoundary>
    </PlatformProvider>
  </React.StrictMode>,
);
