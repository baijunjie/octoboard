/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_DAEMON_PORT?: string;
  readonly VITE_DAEMON_KEY?: string;
}

/** Whether the dev server proxies `/ws` to a daemon (see `vite.config.ts`); false in a build. */
declare const __OCTOBOARD_DEV_PROXY__: boolean;

/** The release version this build of the UI is of, the daemon's own for the page a daemon serves (see `vite.config.ts`). */
declare const __OCTOBOARD_VERSION__: string;

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
