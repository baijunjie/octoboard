import type { MermaidConfig } from "mermaid";

import { CODE_THEMES } from "./codeTheme";

/**
 * Mermaid's configuration for one appearance. `securityLevel: "strict"` is what keeps a diagram
 * from emitting HTML or running a script; it is set here, not filtered out of the drawing
 * afterwards. `htmlLabels` is added to `secure` because a diagram's own directive can turn it back
 * on, and the library's default secure list does not cover it. With it secured and false, labels
 * are text. `startOnLoad` is false so the library does not scan the page for diagrams of its own.
 */
const SECURE = ["secure", "securityLevel", "startOnLoad", "maxTextSize", "suppressErrorRendering", "maxEdges", "htmlLabels"];

/** The system font written into the drawing. A font named inside the SVG does not inherit. */
const DIAGRAM_FONT = "ui-sans-serif, system-ui, sans-serif";

/** The configuration passed to `mermaid.initialize` before a diagram is drawn. */
export function diagramConfig(appearance: "light" | "dark"): MermaidConfig {
  const colours = CODE_THEMES[appearance];
  const ink = colours.foreground;
  const paper = colours.background;
  return {
    startOnLoad: false,
    securityLevel: "strict",
    htmlLabels: false,
    suppressErrorRendering: true,
    secure: [...SECURE],
    theme: "base",
    darkMode: appearance === "dark",
    fontFamily: DIAGRAM_FONT,
    themeVariables: {
      background: paper,
      primaryColor: paper,
      primaryTextColor: ink,
      primaryBorderColor: ink,
      lineColor: ink,
      textColor: ink,
      secondaryColor: paper,
      tertiaryColor: paper,
      mainBkg: paper,
      nodeBorder: ink,
      clusterBkg: paper,
      titleColor: ink,
      edgeLabelBackground: paper,
      actorTextColor: ink,
      signalColor: ink,
      signalTextColor: ink,
      noteTextColor: ink,
      noteBorderColor: ink,
    },
  };
}

/** The accessible name and the text alternative a diagram's own source declares, when it does. */
export function diagramReading(source: string): { title?: string; description?: string } {
  const title = source.match(/^\s*accTitle\s*:?\s*(.+)$/m)?.[1]?.trim();
  const block = source.match(/^\s*accDescr\s*\{([^}]*)\}/m)?.[1];
  const line = source.match(/^\s*accDescr\s*:\s*(.+)$/m)?.[1];
  const description = (block ?? line)?.trim();
  return {
    title: title || undefined,
    description: description || undefined,
  };
}

let chain: Promise<void> = Promise.resolve();

/**
 * Draws `source` and returns its SVG. Initializes and draws one at a time: the library keeps a
 * single configuration, so two diagrams of different appearances would otherwise race.
 */
export function renderDiagram(id: string, source: string, appearance: "light" | "dark"): Promise<string> {
  const job = chain.then(async () => {
    const mermaid = (await import("mermaid")).default;
    mermaid.initialize(diagramConfig(appearance));
    const { svg } = await mermaid.render(id, source);
    return svg;
  });
  chain = job.then(
    () => undefined,
    () => undefined,
  );
  return job;
}
