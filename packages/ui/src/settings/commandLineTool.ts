/** The name of the command-line tool, the link the application makes to its own executable. */
export const COMMAND_LINE_TOOL = "octoboardd";

/** Where installing the tool makes its link, as the shell does (`command_line/macos.rs` in the desktop shell): the
 * system's directory, and the user's own where the system does not allow a change to the first. */
export const COMMAND_LINE_DIRECTORIES = { system: "/usr/local/bin", user: "~/.local/bin" } as const;
