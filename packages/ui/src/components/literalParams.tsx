import React from "react";

import { InlineReference } from "./InlineReference";

/** The `pathVar` param of the messages that name the `PATH` the agents are looked for on: set as code. */
export const PATH_VAR_PARAMS: { pathVar: React.ReactElement } = { pathVar: <InlineReference monospace wrap>PATH</InlineReference> };

/** The `tilde` param of the messages that name the home directory's shorthand `~/`: set as code. */
export const TILDE_PARAMS: { tilde: React.ReactElement } = { tilde: <InlineReference monospace wrap>~/</InlineReference> };
