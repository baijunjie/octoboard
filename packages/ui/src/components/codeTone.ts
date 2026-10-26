import { createContext, useContext } from "react";

export type CodeColor = "default" | "accent" | "help" | "success" | "warning" | "danger";

/** Alert tone takes precedence over a literal's own preference. Only a status surface sets it (an Alert's frame, a
 * toast, a field's error); never a page or section, where it would tint every reference and code block under it. */
export const CodeToneContext = createContext<CodeColor | undefined>(undefined);

export function useCodeColor(color: CodeColor = "default"): CodeColor {
  return useContext(CodeToneContext) ?? color;
}
