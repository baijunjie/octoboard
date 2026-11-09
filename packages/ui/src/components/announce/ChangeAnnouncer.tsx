import React from "react";

import { AttendedStatusAnnouncer } from "./AttendedStatusAnnouncer";

/**
 * Speaks `text` each time `count` grows, for a change made under the user that nothing on the screen marks (a form field
 * replaced because it changed elsewhere, a file read again). The region is remounted per count, so a second change with
 * the same words is spoken again, and it is emptied once read, as the change is on the screen too. It follows the
 * attended timing of `AttendedStatusAnnouncer`: a change made while the window is unattended is spoken once the user is
 * back, and one that arrives with a focus move is spoken after it. `onSaid` runs once the text of the latest change has
 * been written; a change that replaces one still waiting is the only one said.
 */
export function ChangeAnnouncer({ count, text, onSaid }: { count: number; text: string; onSaid?: () => void }): React.ReactElement {
  return <AttendedStatusAnnouncer key={count} text={count > 0 ? text : undefined} clearAfterRead onSaid={onSaid} />;
}
