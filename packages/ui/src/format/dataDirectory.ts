/** Where the daemon keeps its own data on its host (`paths::data_dir` in the daemon), spelled the way that host writes
 * a path: `%USERPROFILE%\.octoboard` on Windows, `~/.octoboard` elsewhere. For telling the user where it is, never for
 * building a path a request carries. */
export function dataDirectory(pathStyle: string | undefined): string {
  return pathStyle === "windows" ? "%USERPROFILE%\\.octoboard" : "~/.octoboard";
}

/** The daemon's logs under that directory, with the separator its host ends a directory with. */
export function logsDirectory(pathStyle: string | undefined): string {
  const separator = pathStyle === "windows" ? "\\" : "/";
  return `${dataDirectory(pathStyle)}${separator}logs${separator}`;
}
