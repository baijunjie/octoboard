import { expect, it } from "vitest";

import { appConfig } from "../appConfig";
import { DAEMON_INSTALL_COMMAND } from "./daemonInstallCommand";

it("is the install script of the website, piped to the shell", () => {
  expect(DAEMON_INSTALL_COMMAND).toBe(`curl -fsSL ${appConfig.websiteUrl}/install.sh | sh`);
  expect(DAEMON_INSTALL_COMMAND).toMatch(/^curl -fsSL https:\/\/\S+\/install\.sh \| sh$/);
});
