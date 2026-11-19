// Shared by the tests of the modules that read the deployment's settings from the environment when they are loaded
// (`site.config.ts`, and `src/commands.ts` through it): a module is loaded once per process, so each environment gets
// a process of its own.
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

/** The values of `names`, exports of the module at `file`, as that module reads them when loaded with `env` added to
 * the environment, where a variable set to `undefined` is left out of it. The deployment variables are cleared first,
 * so what the machine running the test has set does not leak in. */
export function loadWithEnv(file: string, names: readonly string[], env: Record<string, string | undefined> = {}): Record<string, unknown> {
  const environment: NodeJS.ProcessEnv = { ...process.env };
  delete environment.NUXT_APP_BASE_URL;
  delete environment.NUXT_PUBLIC_SITE_ORIGIN;
  for (const [name, value] of Object.entries(env)) {
    if (value === undefined) delete environment[name];
    else environment[name] = value;
  }
  const script = `import(${JSON.stringify(pathToFileURL(file).href)}).then((module) => {
    console.log(JSON.stringify(Object.fromEntries(${JSON.stringify(names)}.map((name) => [name, name === "default" ? module.default : module[name]]))));
  });`;
  const run = spawnSync(process.execPath, ["--experimental-strip-types", "--no-warnings", "--input-type=module", "-e", script], {
    env: environment,
    encoding: "utf8",
  });
  if (run.status !== 0) throw new Error(`${file} did not load:\n${run.stderr}`);
  return JSON.parse(run.stdout) as Record<string, unknown>;
}
