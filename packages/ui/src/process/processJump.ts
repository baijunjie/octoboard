/** Where the global process list sends the user: the Processes panel of a project, with every session's processes, or
 * that of a console session. */
export type ProcessJump = { project: string } | { consoleSession: string };
