// The one module that touches the Markdown library (`react-markdown`, `remark-gfm`, `remark-math`).
// It is loaded lazily by `MarkdownSurface.tsx`, the first time a document is shown. Raw HTML in the
// text stays text. A diagram, a formula and a project image are drawn inside the document; the
// libraries load on first use, and a project file is the only thing a path in the text can read.
// An address is never followed by the viewer itself: a project file is handed to `onOpenFile`, and
// an absolute web or mail address to `onOpenLink`.
import React, { memo, useEffect, useId, useMemo, useRef } from "react";
import Markdown, { type Components, type ExtraProps, type Options } from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";

import { ExternalLink } from "../components/ExternalLink";
import { useFocusVisibleProps } from "../components/useFocusVisibleProps";
import { textDirection } from "../i18n/languages";
import { useCurrentLanguage, useT } from "../i18n/react";
import { type Direction, type DirectionNode, firstStrongDirection } from "./blockDirection";
import { DocumentImage } from "./DocumentImage";
import { FormulaView } from "./FormulaView";
import { headingIds, limitFences, prepareMarkdown } from "./markdownPlugins";
import { markdownStyle, type MarkdownStyle } from "./markdownStyle";
import { MermaidBlock } from "./MermaidBlock";
import { resolveProjectAddress } from "./projectAddress";

export { openableAddress } from "./projectAddress";

// The library's syntax tree types, which it does not export by name.
type Element = NonNullable<ExtraProps["node"]>;
type ElementContent = Element["children"][number];

export interface MarkdownDocumentProps {
  text: string;
  /** The document's own wire path, which relative addresses resolve against. */
  path: string;
  /** The project a relative image is read from. Without it, an image stays its description. */
  project?: string;
  /** Draws a fenced block of code; `language` is the fence's info word, when it has one, and
   * `highlight` is false for a block past the document's budget of highlighted ones. */
  renderCode: (code: { language?: string; text: string; highlight: boolean }) => React.ReactNode;
  /** The appearance a diagram is drawn in. */
  theme: "light" | "dark";
  /** Opens an absolute web or mail address; without it such a link is drawn as its text alone. */
  onOpenLink?: (url: string) => void | Promise<void>;
  /** Opens a project file the document links to, and a heading in it when `fragment` is set.
   * Without it a relative link is drawn as its text alone. */
  onOpenFile?: (path: string, fragment?: string) => void;
  /** A path the project's listing shows as a directory is not opened. */
  isDirectory?: (path: string) => boolean;
  /** A heading to move to once the document is shown, from a link that opened this file. */
  anchor?: string;
  /** The document is one of several inside a view with headings of its own: its headings are
   * demoted two levels (`h1` to `h3`, none below `h6`) so none outranks the view's, and it gives
   * no heading an id and its footnotes ids of their own, so documents side by side cannot clash. */
  embedded?: boolean;
}

/** The text a node reads as; an image reads as its alt text. */
function textOf(node: ElementContent): string {
  if (node.type === "text") return node.value;
  if (node.type !== "element") return "";
  if (node.tagName === "img") return String(node.properties.alt ?? "");
  return node.children.map(textOf).join("");
}

/** The fenced block a `<pre>` holds: its text without the final newline, its info word, and whether
 * the budget leaves it highlighted (`limitFences`). */
function fenceOf(node: Element): { language?: string; text: string; highlight: boolean } {
  const code = node.children.find((child): child is Element => child.type === "element" && child.tagName === "code");
  const classes = code?.properties.className;
  const info = Array.isArray(classes) ? classes.find((name) => typeof name === "string" && name.startsWith("language-")) : undefined;
  const text = (code?.children ?? []).map(textOf).join("").replace(/\n$/, "");
  return {
    language: typeof info === "string" ? info.slice("language-".length) : undefined,
    text,
    highlight: code?.properties.dataPlain === undefined,
  };
}

/** Puts the document's content in the frame `<div data-markdown-document>`, which takes the
 * direction of the first strong letter of the whole text, or `fallback` when it has none, and the
 * classes of the document's `style` (`markdown.css`). Done on the syntax tree because the wrapper
 * has to be told a direction that only the tree can give: the library is a component of its own,
 * which draws after the one around it. Called with that direction before it is placed in the plugin
 * list, so this returns the plugin: unified calls each entry to obtain the transformer, and a
 * transformer placed in the list runs once at setup and never on the tree. */
function frameDocument(fallback: Direction, style: MarkdownStyle) {
  return () => (tree: DirectionNode & { children: DirectionNode[] }) => {
    tree.children = [
      {
        type: "element",
        tagName: "div",
        properties: {
          dir: firstStrongDirection(tree) ?? fallback,
          dataMarkdownDocument: "",
          dataMarkdownStyle: style,
          className: style === "tailwind" ? "markdown-prose prose prose-sm max-w-none" : "markdown-prose markdown-styled",
        },
        children: tree.children,
      },
    ];
  };
}

/** Gives the footnotes' heading, whose id the library fixes as `footnote-label`, an id of its own,
 * and points the references that the library describes by it (`aria-describedby`) there, so
 * documents side by side do not share one. */
function scopeFootnoteLabel(scope: string) {
  const scoped = `footnote-label-${scope}`;
  return () => (tree: DirectionNode & { children: DirectionNode[] }) => {
    const visit = (node: DirectionNode & { properties?: Record<string, unknown>; children?: DirectionNode[] }) => {
      const props = node.properties;
      if (props?.id === "footnote-label") props.id = scoped;
      if (Array.isArray(props?.ariaDescribedBy)) {
        props.ariaDescribedBy = props.ariaDescribedBy.map((id) => (id === "footnote-label" ? scoped : id));
      }
      node.children?.forEach(visit);
    };
    visit(tree);
  };
}

/** A link in the document. The anchor does not follow `href` itself. `onActivate` scrolls within
 * the document, opens a project file, or hands an absolute address to the platform. */
function DocumentLink({
  href,
  title,
  onActivate,
  children,
  ...attributes
}: Omit<React.AnchorHTMLAttributes<HTMLAnchorElement>, "onClick" | "onAuxClick" | "className"> & {
  href: string;
  title?: string;
  onActivate: (event: React.MouseEvent<HTMLAnchorElement>) => void;
  children: React.ReactNode;
}): React.ReactElement {
  const focus = useFocusVisibleProps();
  return (
    <a
      {...attributes}
      href={href}
      // Native, not the app's tooltip, which is the rule everywhere else: on a link `title` is also its
      // accessible description, which a tooltip shown on hover alone cannot give a screen reader.
      // It shows where the link goes before it is followed.
      title={title}
      {...focus}
      className="rounded-sm break-words text-(--document-link) underline underline-offset-2 outline-none data-focus-visible:ring-2 data-focus-visible:ring-app-focus"
      onClick={(event) => {
        event.preventDefault();
        onActivate(event);
      }}
      onAuxClick={(event) => event.preventDefault()}
    >
      {children}
    </a>
  );
}

/** Moves to the element `fragment` names, inside `root`. A heading id is the slug text; a footnote
 * id is stored percent-encoded and matches the fragment as written, so both are tried. The
 * browser's own navigation to a fragment would move the viewer's page. Focus goes with it, so a
 * screen reader follows. */
export function revealInDocument(root: ParentNode, fragment: string): void {
  let decoded = fragment;
  try {
    decoded = decodeURIComponent(fragment);
  } catch {
    // A fragment that is not valid encoding is compared as written.
  }
  const ids = [...root.querySelectorAll<HTMLElement>("[id]")];
  const target = ids.find((element) => element.id === fragment || element.id === decoded);
  if (!target) return;
  // Not a no-op: the getter answers -1 for an element with no `tabindex` attribute, a definition's
  // `<li>`, while the setter writes the attribute, which is what lets it take focus from a script. A
  // link back to the reference answers 0, and writing -1 there would take it out of the Tab order.
  if (target.tabIndex < 0) target.tabIndex = -1;
  target.focus({ preventScroll: true });
  target.scrollIntoView({ block: "nearest" });
}

function classNamesOf(node: Element | undefined): string[] {
  const names = node?.properties.className;
  return Array.isArray(names) ? names.filter((name): name is string => typeof name === "string") : [];
}

function codeChild(node: Element): Element | undefined {
  return node.children.find((child): child is Element => child.type === "element" && child.tagName === "code");
}

function overBudget(node: Element | undefined): boolean {
  return node?.properties.dataOverBudget !== undefined;
}

/** An element the library draws, with a class string of ours merged into the classes the library
 * gives it of its own (`contains-task-list`, `sr-only`, `language-ts`): the `className` prop is
 * merged, never replaced, and both sets of utilities are on the element, the stylesheet and not
 * their order deciding which wins. The one exception is the footnote heading, which the library
 * marks `sr-only` and which keeps to that alone: it is for screen readers, and the styling of a
 * heading would show it (the style sheets leave an `sr-only` element out as well). Made once,
 * here, so every element keeps one identity for the library to keep mounted. A `dir` of
 * "first-strong" gives the element the direction of the first strong letter of its whole text,
 * see `blockDirection.ts`; a block with no strong letter of its own is given no `dir` at all, so
 * that it reads the way the document around it does rather than being pinned left to right the
 * way an unresolved `dir="auto"` would be. */
function styled(
  tag:
    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "ul" | "ol" | "li" | "blockquote" | "hr" | "table" | "th" | "td" | "code" | "section",
  base: string,
  dir?: "ltr" | "first-strong",
) {
  return function Styled({ node, className, ...props }: ExtraProps & React.HTMLAttributes<HTMLElement>): React.ReactElement {
    const screenReaderOnly = className?.split(" ").includes("sr-only");
    return React.createElement(tag, {
      dir: dir === "first-strong" ? (node && firstStrongDirection(node as DirectionNode)) : dir,
      ...props,
      className: screenReaderOnly ? className : [base, className].filter(Boolean).join(" "),
    });
  };
}

// Direction follows each block's text; the document's style (`markdown.css`) owns its visual hierarchy.
const BLOCKS = {
  h1: styled("h1", "", "first-strong"),
  h2: styled("h2", "", "first-strong"),
  h3: styled("h3", "", "first-strong"),
  h4: styled("h4", "", "first-strong"),
  h5: styled("h5", "", "first-strong"),
  h6: styled("h6", "", "first-strong"),
  p: styled("p", "break-words", "first-strong"),
  ul: styled("ul", "", "first-strong"),
  ol: styled("ol", "", "first-strong"),
  li: styled("li", "break-words"),
  blockquote: styled("blockquote", "break-words", "first-strong"),
  hr: styled("hr", ""),
  table: styled("table", "", "first-strong"),
  // GFM's explicit physical column alignment wins over this directional default.
  th: styled("th", "text-start"),
  td: styled("td", "text-start"),
  code: styled("code", "rounded bg-current/10 px-1 py-0.5 font-mono text-[0.9em] break-words", "ltr"),
  section: styled("section", "mt-6 border-t border-current/25 pt-3 text-xs"),
} satisfies Components;

// An embedded document's headings sit two levels down, clamped at the last.
const DEMOTED_BLOCKS = {
  h1: styled("h3", "", "first-strong"),
  h2: styled("h4", "", "first-strong"),
  h3: styled("h5", "", "first-strong"),
  h4: styled("h6", "", "first-strong"),
  h5: styled("h6", "", "first-strong"),
  h6: styled("h6", "", "first-strong"),
} satisfies Components;

// A task list's boxes are drawn as the checked or unchecked state they record, not as controls. The
// library already marks them disabled; this keeps them inert whatever it does.
function TaskBox({ node: _, className, ...props }: ExtraProps & React.InputHTMLAttributes<HTMLInputElement>): React.ReactElement {
  return <input {...props} disabled className={className} />;
}

/** A Markdown text as a document, drawn again only when its text or callbacks change. Each block
 * takes the direction of its own first strong letter, as the document does as a whole, so a
 * right-to-left passage in a left-to-right document, or the reverse, reads from its own side, with
 * its quote bar there too. Raw HTML in the text comes out as bare text with no block around it, which
 * takes the direction of the document. A document with no strong letter takes the app's direction.
 * A diagram and a formula are left to right of their own, and do not lend their source to the
 * direction of the paragraph they sit in. */
export const MarkdownDocument = memo(function MarkdownDocument({
  text,
  path,
  project,
  renderCode,
  theme,
  onOpenLink,
  onOpenFile,
  isDirectory,
  anchor,
  embedded = false,
}: MarkdownDocumentProps): React.ReactElement {
  const t = useT();
  const appDirection = textDirection(useCurrentLanguage());
  const style = markdownStyle.useValue();
  const frameRef = useRef<HTMLDivElement>(null);
  const idScope = useId();
  useEffect(() => {
    if (!anchor) return;
    const root = frameRef.current?.querySelector("[data-markdown-document]");
    if (root) revealInDocument(root, anchor);
  }, [anchor, text]);
  // Not memoised, unlike the components below: the library builds its processor on every render
  // whatever this array's identity is, so holding one would buy nothing.
  const remarkPlugins: NonNullable<Options["remarkPlugins"]> = [remarkGfm, remarkMath, prepareMarkdown(text, path), limitFences];
  const rehypePlugins: NonNullable<Options["rehypePlugins"]> = embedded
    ? [scopeFootnoteLabel(idScope), frameDocument(appDirection, style)]
    : [headingIds, frameDocument(appDirection, style)];
  // Memoised: the library takes a new component for an element to be a new kind of element and
  // mounts it afresh, which would take the focus of a link with it and draw every fenced block again
  // each time the caller renders (as it does when the frame gains or loses its focus ring).
  const components = useMemo<Components>(
    () => ({
      ...BLOCKS,
      ...(embedded ? DEMOTED_BLOCKS : {}),
      input: TaskBox,
      img: ({ src, alt, node }) => (
        <DocumentImage src={src} alt={alt} documentPath={path} project={project} overBudget={overBudget(node)} />
      ),
      code: ({ node, className, ...props }) => {
        if (classNamesOf(node).includes("math-inline")) {
          return (
            <span className="not-prose">
              <FormulaView
                source={(node ? textOf(node) : "").replace(/\n$/, "")}
                display={node?.properties.dataDisplay !== undefined}
                inline
                overBudget={overBudget(node)}
              />
            </span>
          );
        }
        return <BLOCKS.code node={node} className={className} {...props} />;
      },
      pre: ({ node }) => {
        const code = node && codeChild(node);
        if (code && classNamesOf(code).includes("math-display")) {
          return (
            <div className="not-prose">
              <FormulaView source={fenceOf(node).text} display inline={false} overBudget={overBudget(node) || overBudget(code)} />
            </div>
          );
        }
        if (node && fenceOf(node).language === "mermaid") {
          return (
            <div className="not-prose">
              <MermaidBlock source={fenceOf(node).text} overBudget={overBudget(code)} theme={theme} />
            </div>
          );
        }
        return <div className="not-prose my-4">{node ? renderCode(fenceOf(node)) : null}</div>;
      },
      a: ({ node, href, children, ...rest }) => {
        // The links `remark-gfm` makes for a footnote and back from it. They stay in the document.
        const props = rest as Record<string, unknown>;
        if (href?.startsWith("#") && (props["data-footnote-ref"] !== undefined || props["data-footnote-backref"] !== undefined)) {
          return (
            <DocumentLink {...rest} href={href} onActivate={(event) => revealInDocument(event.currentTarget.closest("[data-markdown-document]") ?? event.currentTarget, href.slice(1))}>
              {children}
            </DocumentLink>
          );
        }
        const address = resolveProjectAddress(path, href);
        const unnamed = node ? node.children.map(textOf).join("").trim() === "" : false;
        if (address.kind === "anchor") {
          return (
            <DocumentLink {...rest} href={href ?? "#"} onActivate={(event) => revealInDocument(event.currentTarget.closest("[data-markdown-document]") ?? event.currentTarget, address.id)}>
              {children}
            </DocumentLink>
          );
        }
        if (address.kind === "external" && onOpenLink) {
          return (
            <ExternalLink href={address.href} title={address.href} style={{ color: "var(--document-link)" }} onOpen={() => onOpenLink(address.href)}>
              {unnamed ? address.href : children}
            </ExternalLink>
          );
        }
        if (address.kind === "file" && onOpenFile && !isDirectory?.(address.path)) {
          const where = address.fragment ? `${address.path}#${address.fragment}` : address.path;
          return (
            <DocumentLink href={href ?? where} title={where} onActivate={() => onOpenFile(address.path, address.fragment)}>
              {unnamed ? where : children}
            </DocumentLink>
          );
        }
        return <>{children}</>;
      },
    }),
    [renderCode, onOpenLink, onOpenFile, isDirectory, path, project, theme, embedded],
  );
  // The library words the footnotes' heading and each backlink's name in English.
  const remarkRehypeOptions = useMemo<Options["remarkRehypeOptions"]>(
    () => ({
      ...(embedded ? { clobberPrefix: `user-content-${idScope}-` } : {}),
      footnoteLabel: t("viewer.markdown.footnotes"),
      footnoteBackLabel: (referenceIndex, rereferenceIndex) =>
        t("viewer.markdown.footnoteBack", {
          reference: rereferenceIndex > 1 ? `${referenceIndex + 1}-${rereferenceIndex}` : referenceIndex + 1,
        }),
    }),
    [t, embedded, idScope],
  );
  return (
    <div ref={frameRef} className="contents">
      <Markdown remarkPlugins={remarkPlugins} rehypePlugins={rehypePlugins} remarkRehypeOptions={remarkRehypeOptions} components={components}>
        {text}
      </Markdown>
    </div>
  );
});
