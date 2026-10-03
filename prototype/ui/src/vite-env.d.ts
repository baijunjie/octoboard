/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_DAEMON_PORT?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
