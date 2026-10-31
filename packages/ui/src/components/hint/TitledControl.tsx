import React from "react";

import { Hint, type ControlHintProps } from "./Hint";

/** `Hint` of kind `control` under a shorter name: a tooltip on a single react-aria control (the rules are in
 * `hintTooltips.tsx`). `title` is `Hint`'s `tooltip`, the one prop the alias renames: the ~33 call sites and the
 * gallery's own tests spell it `title`, as HTML does for the same text. */
export function TitledControl({ title, ...rest }: Omit<ControlHintProps, "tooltip"> & { title: React.ReactNode }): React.ReactElement {
  return <Hint kind="control" tooltip={title} {...rest} />;
}
