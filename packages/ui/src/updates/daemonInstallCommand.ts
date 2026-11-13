import { appConfig } from "../appConfig";

/** The command that installs the current release of the daemon over the one on its host, for an update the daemon
 * cannot put in place itself (its executable is a file it cannot replace). */
export const DAEMON_INSTALL_COMMAND = `curl -fsSL ${appConfig.websiteUrl}/install.sh | sh`;
