// The one module that touches the Markdown library (`react-markdown` with `remark-gfm`). It is
// loaded lazily by `MarkdownSurface.tsx`, the first time a document is shown. The document it draws
// is inert: the library turns raw HTML in the text into text, an image is its alt text and never a
// request, and an address is only ever handed to `onOpenLink`, never followed.
import React, { memo, useMemo } from "react";
import Markdown, { type Components, type ExtraProps, type Options } from "react-markdown";
import remarkGfm from "remark-gfm";

import { useFocusVisibleProps } from "../components/useFocusVisibleProps";
import { useT } from "../i18n/react";
import { RENDER_BUDGETS } from "./budgets";

// The library's syntax tree types, which it does not export by name.
type Element = NonNullable<ExtraProps["node"]>;
type ElementContent = Element["children"][number];

export interface MarkdownDocumentProps {
  text: string;
  /** Draws a fenced block of code; `language` is the fence's info word, when it has one, and
   * `highlight` is false for a block past the document's budget of highlighted ones. */
  renderCode: (code: { language?: string; text: string; highlight: boolean }) => React.ReactNode;
  /** Opens an address the document links to; without it a link is drawn as its text alone. */
  onOpenLink?: (url: string) => void;
}

/** The address of a link that may be opened: absolute, and one a person or a mail client takes.
 * Anything else — a relative path, an anchor, a script — is not a place this document can send
 * the user to (the footnote links of the document itself are its own case, `DocumentLink`). */
export function openableAddress(href: string | undefined): string | undefined {
  if (!href) return undefined;
  try {
    const url = new URL(href.trim());
    return url.protocol === "http:" || url.protocol === "https:" || url.protocol === "mailto:" ? url.href : undefined;
  } catch {
    return undefined;
  }
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

/** The Markdown syntax tree as far as `limitFences` reads it. */
interface MdNode {
  type: string;
  value?: string;
  children?: MdNode[];
  data?: { hProperties?: Record<string, unknown> };
}

/** Marks the fenced blocks past the document's budget (`RENDER_BUDGETS.documentFences`) as plain,
 * for `fenceOf`. Each highlighted block is a renderer instance of its own, so the count and the total
 * length of what is highlighted are bounded, in document order. Only a block that stays highlighted
 * counts against either, so one over-size block does not turn the later, small ones plain; a single block
 * longer than the total budget is plain itself, though a whole file of that size is highlighted: it
 * would be one instance drawing all of it inside the document's scroll, not a file's own frame. */
function limitFences() {
  return (tree: MdNode) => {
    let blocks = 0;
    let chars = 0;
    const visit = (node: MdNode) => {
      if (node.type === "code") {
        const length = node.value?.length ?? 0;
        if (blocks + 1 > RENDER_BUDGETS.documentFences || chars + length > RENDER_BUDGETS.documentFenceChars) {
          node.data = { ...node.data, hProperties: { ...node.data?.hProperties, dataPlain: true } };
        } else {
          blocks += 1;
          chars += length;
        }
      }
      node.children?.forEach(visit);
    };
    visit(tree);
  };
}

const REMARK_PLUGINS: NonNullable<Options["remarkPlugins"]> = [remarkGfm, limitFences];

/** A link in the document. It never navigates the viewer: an address is handed to `onActivate`
 * (the platform opens it), and so is a footnote's, which scrolls to its target within the document. */
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
      // Native, not `TitledControl`'s tooltip: the link is a piece of running text, not a control that
      // has a name of its own to give, and the tooltip would take it out of the line to be wrapped
      // in a trigger. It shows where the link goes before it is followed.
      title={title}
      {...focus}
      className="rounded-sm break-words text-(--document-link) underline underline-offset-2 outline-none data-focus-visible:ring-2 data-focus-visible:ring-focus"
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

/** Moves to the element a footnote link names, inside the document it is in; the browser's own
 * navigation to a fragment would move the viewer's page. Focus goes with it, so a screen reader
 * follows. */
function followFootnote(link: HTMLAnchorElement, href: string): void {
  // The library writes the fragment and the id it names through the same encoding (a footnote
  // labelled in Chinese is percent-encoded in both), so they are compared as they are, not decoded.
  const fragment = href.slice(1);
  const ids = [...(link.closest("[data-markdown-document]")?.querySelectorAll<HTMLElement>("[id]") ?? [])];
  const target = ids.find((element) => element.id === fragment);
  if (!target) return;
  // Not a no-op: the getter answers -1 for an element with no `tabindex` attribute, a definition's
  // `<li>`, while the setter writes the attribute, which is what lets it take focus from a script. A
  // link back to the reference answers 0, and writing -1 there would take it out of the Tab order.
  if (target.tabIndex < 0) target.tabIndex = -1;
  target.focus({ preventScroll: true });
  target.scrollIntoView({ block: "nearest" });
}

/** An element the library draws, with a class string of ours merged into the classes the library
 * gives it of its own (`contains-task-list`, `sr-only`, `language-ts`): the `className` prop is
 * merged, never replaced, and both sets of utilities are on the element, the stylesheet and not
 * their order deciding which wins. The one exception is the footnote heading, which the library
 * marks `sr-only` and which keeps to that alone: it is for screen readers, and the styling of a
 * heading would show it. Made once, here, so every element keeps one identity for the library to
 * keep mounted. */
function styled(
  tag:
    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "ul" | "ol" | "li" | "blockquote" | "hr" | "table" | "th" | "td" | "code" | "section",
  base: string,
  attributes?: Record<string, string>,
) {
  return function Styled({ node: _, className, ...props }: ExtraProps & React.HTMLAttributes<HTMLElement>): React.ReactElement {
    const screenReaderOnly = className?.split(" ").includes("sr-only");
    return React.createElement(tag, {
      ...attributes,
      ...props,
      className: screenReaderOnly ? className : className ? `${base} ${className}` : base,
    });
  };
}

const HEADING = "mt-6 mb-3 font-semibold break-words first:mt-0";

// A list, a table or a quote is one block and takes one direction from its first strong letter
// (`dir="auto"`), as the paragraphs and headings do each for themselves; so a list that starts in
// English and goes on in Arabic is drawn left to right throughout. Giving its items a direction of
// their own would put their markers outside the list's padding.
const BLOCKS = {
  h1: styled("h1", `${HEADING} border-b border-current/25 pb-1 text-2xl`, { dir: "auto" }),
  h2: styled("h2", `${HEADING} border-b border-current/25 pb-1 text-xl`, { dir: "auto" }),
  h3: styled("h3", `${HEADING} text-lg`, { dir: "auto" }),
  h4: styled("h4", `${HEADING} text-base`, { dir: "auto" }),
  h5: styled("h5", `${HEADING} text-sm`, { dir: "auto" }),
  h6: styled("h6", `${HEADING} text-sm`, { dir: "auto" }),
  p: styled("p", "my-3 break-words first:mt-0 last:mb-0", { dir: "auto" }),
  ul: styled("ul", "my-3 list-disc ps-6", { dir: "auto" }),
  ol: styled("ol", "my-3 list-decimal ps-6", { dir: "auto" }),
  // A task list item shows its box in place of a bullet.
  li: styled("li", "my-1 break-words [&.task-list-item]:list-none"),
  blockquote: styled("blockquote", "my-3 break-words border-s-4 border-current/40 ps-4 italic", { dir: "auto" }),
  hr: styled("hr", "my-6 border-current/25"),
  table: styled("table", "my-3 w-full border-collapse", { dir: "auto" }),
  // A column's alignment reaches the cell as an inline `text-align` (the library turns `align` into
  // one), which wins over `text-start`; that is only the default, as a header cell would otherwise
  // take the browser's own centring. The alignment stays the author's physical left and right, not
  // start and end: GFM specifies it so and every other renderer shows it so, and only an unaligned
  // column follows the table's own direction.
  th: styled("th", "border border-current/25 px-3 py-1.5 text-start font-semibold break-words"),
  td: styled("td", "border border-current/25 px-3 py-1.5 text-start break-words"),
  // Reached for inline code only: a fenced block's `<pre>` is drawn whole.
  code: styled("code", "rounded bg-current/10 px-1 py-0.5 font-mono text-[0.9em] break-words", { dir: "ltr" }),
  // The footnotes, where `remark-gfm` makes any.
  section: styled("section", "mt-6 border-t border-current/25 pt-3 text-xs"),
} satisfies Components;

// A task list's boxes are drawn as the checked or unchecked state they record, not as controls. The
// library already marks them disabled; this keeps them inert whatever it does.
function TaskBox({ node: _, className, ...props }: ExtraProps & React.InputHTMLAttributes<HTMLInputElement>): React.ReactElement {
  return <input {...props} disabled className={className ? `me-2 align-middle ${className}` : "me-2 align-middle"} />;
}

// Never fetched: an image is its description, which is all the document can know of it.
function ImageAlt({ alt }: { alt?: string }): React.ReactElement | null {
  return alt ? <span>{alt}</span> : null;
}

/** A Markdown text as a document, drawn again only when its text or callbacks change. Paragraphs
 * and headings take the direction of their own first strong letter (`dir="auto"`), as the document
 * does as a whole, so a right-to-left passage in a left-to-right document, or the reverse, reads from
 * its own side, with its quote bar there too; the frame around it follows the app's direction. */
export const MarkdownDocument = memo(function MarkdownDocument({
  text,
  renderCode,
  onOpenLink,
}: MarkdownDocumentProps): React.ReactElement {
  const t = useT();
  // Memoised: the library takes a new component for an element to be a new kind of element and
  // mounts it afresh, which would take the focus of a link with it and draw every fenced block again
  // each time the caller renders (as it does when the frame gains or loses its focus ring).
  const components = useMemo<Components>(
    () => ({
      ...BLOCKS,
      input: TaskBox,
      img: ImageAlt,
      pre: ({ node }) => <div className="my-3">{node ? renderCode(fenceOf(node)) : null}</div>,
      a: ({ node, href, children, ...rest }) => {
        // The links `remark-gfm` makes for a footnote and back from it. They stay in the document, so
        // they are not for the platform to open (and `openableAddress` is not relaxed for them): the
        // one fragment links the document follows. A fragment link of the author's own has no target
        // (headings carry no id) and is its text alone, like any address that is not openable.
        const props = rest as Record<string, unknown>;
        if (href?.startsWith("#") && (props["data-footnote-ref"] !== undefined || props["data-footnote-backref"] !== undefined)) {
          return (
            <DocumentLink {...rest} href={href} onActivate={(event) => followFootnote(event.currentTarget, href)}>
              {children}
            </DocumentLink>
          );
        }
        const address = openableAddress(href);
        if (!address || !onOpenLink) return <>{children}</>;
        // A link whose text is empty or only an image has no name for a screen reader but its address.
        const unnamed = node ? node.children.map(textOf).join("").trim() === "" : false;
        return (
          <DocumentLink href={address} title={address} onActivate={() => onOpenLink(address)}>
            {unnamed ? address : children}
          </DocumentLink>
        );
      },
    }),
    [renderCode, onOpenLink],
  );
  // The library words the footnotes' heading and each backlink's name in English.
  const remarkRehypeOptions = useMemo<Options["remarkRehypeOptions"]>(
    () => ({
      footnoteLabel: t("viewer.markdown.footnotes"),
      footnoteBackLabel: (referenceIndex, rereferenceIndex) =>
        t("viewer.markdown.footnoteBack", {
          reference: rereferenceIndex > 1 ? `${referenceIndex + 1}-${rereferenceIndex}` : referenceIndex + 1,
        }),
    }),
    [t],
  );
  return (
    // Raw HTML in the text comes out as bare text with no block around it, which takes the direction
    // of the document as a whole.
    <div dir="auto" data-markdown-document="">
      <Markdown remarkPlugins={REMARK_PLUGINS} remarkRehypeOptions={remarkRehypeOptions} components={components}>
        {text}
      </Markdown>
    </div>
  );
});
