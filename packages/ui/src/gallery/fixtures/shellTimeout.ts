import type { FixtureError } from "./projectFiles";

/** The daemon preserves each literal separately so the UI can present the command as code. */
export const SHELL_TIMEOUT: FixtureError = {
  code: "shell_environment_timeout",
  params: {
    shell: "/bin/zsh",
    command: "env -0 && printf '%s' 'OCTOBOARD-ENV-SNAPSHOT-END-12345678-1234-1234-1234-123456789abc'",
    timeout: "10s",
  },
  message: "`/bin/zsh -l -i -c 'env -0 && printf '%s' 'OCTOBOARD-ENV-SNAPSHOT-END-12345678-1234-1234-1234-123456789abc''` did not finish within 10s; a shell startup file is probably blocked on something other than stdin, or it left a background process holding the shell's output open. Octoboard refuses to launch with an unknown environment rather than guess at one — fix or skip the slow step in the shell's rc files.",
};
