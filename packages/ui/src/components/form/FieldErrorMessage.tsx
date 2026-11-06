import React from "react";

import { CodeToneContext } from "../../i18n/codeTone";
import { FieldError } from "../../primitives";

/** The message under a field that says what is wrong with it, for a field to hold: `children` is the message
 * while the field is invalid and nothing otherwise, and `FieldError` renders nothing while the field is valid. The alert
 * is on an element of its own because react-aria drops a `role` given to the error itself: the message appears on a
 * submit that gives the user no other feedback, so it has to be announced as a dialog-wide failure is. The error is
 * danger-toned text, so a literal inside it is too, as in a danger alert. */
export function FieldErrorMessage({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <FieldError>
      <CodeToneContext.Provider value="danger">
        <span role="alert">{children}</span>
      </CodeToneContext.Provider>
    </FieldError>
  );
}
