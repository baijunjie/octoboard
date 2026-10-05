/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_DAEMON_PORT?: string;
}

/** Whether the dev server proxies `/ws` to a daemon (see `vite.config.ts`); false in a build. */
declare const __OCTOBOARD_DEV_PROXY__: boolean;

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
