import React, { createContext, useContext } from "react";

/**
 * Where the document on screen lives, and what a link or an image in it may do. The path is the
 * document's own wire path. The rest is present only where the document view is offered a project
 * to read from and a way to open another file; a document shown without them draws the same text
 * and leaves relative addresses as text.
 */
export interface MarkdownLocation {
  path: string;
  project?: string;
  anchor?: string;
  onOpenFile?: (path: string, fragment?: string) => void;
  isDirectory?: (path: string) => boolean;
}

const MarkdownLocationContext = createContext<MarkdownLocation | undefined>(undefined);

export function MarkdownLocationProvider({
  value,
  children,
}: {
  value: MarkdownLocation;
  children: React.ReactNode;
}): React.ReactElement {
  return <MarkdownLocationContext.Provider value={value}>{children}</MarkdownLocationContext.Provider>;
}

export function useMarkdownLocation(): MarkdownLocation | undefined {
  return useContext(MarkdownLocationContext);
}
