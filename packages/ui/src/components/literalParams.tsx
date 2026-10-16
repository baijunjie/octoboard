import React from "react";

import { InlineChip } from "./InlineChip";

/** The `pathVar` param of the messages that name the `PATH` the agents are looked for on: set as code. */
export const PATH_VAR_PARAMS: { pathVar: React.ReactElement } = { pathVar: <InlineChip monospace wrap size="xs">PATH</InlineChip> };

/** The `tilde` param of the messages that name the home directory's shorthand `~/`: set as code. */
export const TILDE_PARAMS: { tilde: React.ReactElement } = { tilde: <InlineChip monospace wrap size="xs">~/</InlineChip> };
