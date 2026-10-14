import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

// Relative asset URLs keep this build usable beneath the website's configured deployment path.
const result = spawnSync(
  "pnpm",
  ["--filter", "@octoboard/ui", "build:preview"],
  {
    cwd: fileURLToPath(new URL("../../../", import.meta.url)),
    stdio: "inherit",
  },
);
if (result.error) throw result.error;
process.exit(result.status ?? 1);
