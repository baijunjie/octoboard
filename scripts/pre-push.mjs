// Forwarding stub: hook.sh copies installed before the hook scripts moved to scripts/hooks/ look for this path.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const result = spawnSync(process.execPath, [fileURLToPath(new URL("./hooks/pre-push.mjs", import.meta.url)), ...process.argv.slice(2)], { stdio: "inherit" });
if (result.error) throw result.error;
process.exit(result.status ?? 1);
