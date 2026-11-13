import { resolveDaemonAddress } from "../daemon";
import { useOptionalPlatform } from "../platform/react";

/** Whether this page is the production browser build the daemon serves itself. The daemon embeds only its own build's
 * files, so after it updated, a page that did not reload asks for lazy chunks it no longer has: loading again can
 * never help, and reloading the page is the way out. False without a platform above, in the desktop window and on a dev
 * server. */
export function useServedByDaemon(): boolean {
  const platform = useOptionalPlatform();
  return platform !== undefined && resolveDaemonAddress(platform)?.servedByDaemon === true;
}
