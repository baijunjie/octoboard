import React from "react";

import { StatusBadge } from "../components/StatusBadge";
import { STATUS_MARKS, type StatusKey } from "./statusMarks";

/** A soft badge in the colour a change's status is marked with, for the list's letter and the
 * viewer's word alike. Neither has a name right before it to be told apart from, so it leaves out
 * the badge's comma; `separated` brings it back. */
export function ChangeStatusBadge({ status, separated = false, ...rest }: { status: StatusKey } & Omit<React.ComponentProps<typeof StatusBadge>, "color" | "variant">): React.ReactElement {
  return <StatusBadge variant="soft" color={STATUS_MARKS[status].color} separated={separated} {...rest} />;
}
