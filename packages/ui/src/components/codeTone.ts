import { createContext, useContext } from "react";

export type CodeColor = "default" | "accent" | "help" | "success" | "warning" | "danger";

/** Alert tone takes precedence over a literal's own preference. */
export const CodeToneContext = createContext<CodeColor | undefined>(undefined);

export function useCodeColor(color: CodeColor = "default"): CodeColor {
  return useContext(CodeToneContext) ?? color;
}
