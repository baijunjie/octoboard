import { CircleCheck } from "lucide-react";
import React from "react";

import { CodeBlock } from "../components/CodeBlock";
import { StatusMark } from "../components/StatusMark";
import { InlineReference, type InlineReferenceSize } from "../components/InlineReference";
import { StatusBadge, type StatusBadgeColor, type StatusBadgeSize, type StatusBadgeVariant } from "../components/StatusBadge";
import { TintedAlert } from "../components/TintedAlert";
import { CodeToneContext, type CodeColor } from "../components/codeTone";
import { FadedScroller, useFadeRegion } from "../components/scrollFade";

const COLORS: CodeColor[] = ["default", "accent", "help", "success", "warning", "danger"];
const STATUSES = ["default", "accent", "success", "warning", "danger"] as const;
const BADGE_COLORS: StatusBadgeColor[] = [...STATUSES];
const BADGE_VARIANTS: StatusBadgeVariant[] = ["soft", "bordered"];
const BADGE_SIZES: StatusBadgeSize[] = ["sm", "md", "lg"];

/** The copy styles a chip sits in (docs/ui-accessibility.md, "A chip's size follows the line it sits in"), each with
 * the sizes whose drawn height stays below its line height: the content area, about 1.2 times the font size, plus
 * twice the size's block padding. */
const COPY_LINES: { font: number; line: number; use: string; sizes: InlineReferenceSize[] }[] = [
  { font: 12, line: 16, use: "Field errors, secondary descriptions, tooltips", sizes: ["xs"] },
  { font: 14, line: 20, use: "Body copy, labels, Alert descriptions", sizes: ["xs", "sm"] },
  { font: 16, line: 24, use: "Dialog headings", sizes: ["xs", "sm", "md"] },
  { font: 14, line: 24, use: "Alert titles", sizes: ["xs", "sm", "md", "lg"] },
];
const LONG_PATHS = ["/Users/dev/Projects/octoboard/packages/ui/src/components/InlineReference.tsx", "/Users/dev/Projects/octoboard/packages/ui/src/components/StatusBadge.tsx"];

/** One settled comparison of the shared inline references and code blocks, both outside and inside alert contexts. */
export function CodeGallery(): React.ReactElement {
  const region = useFadeRegion({ label: "Inline references, status badges, status marks and alert tones" });
  return (
    <main className={`flex h-full min-h-0 flex-col ${region.focused ? "ring-2 ring-focus ring-inset" : ""}`}>
      <FadedScroller region={region} className="min-h-0 flex-1 p-6">
        <div className="mx-auto flex max-w-5xl flex-col gap-6">
          <h1 className="text-lg font-semibold">Inline references, status badges, status marks and alert tones</h1>
          <section className="flex flex-col gap-3" aria-labelledby="chip-prose-title">
            <h2 id="chip-prose-title" className="font-semibold">Prose: the chip takes the text’s font, with equal height and baseline</h2>
            {[12, 14, 16].map((size) => (
              <div key={size} data-prose-size={size} className="min-w-0 rounded-lg border border-transparent shadow-(--panel-shadow) bg-surface p-3" style={{ fontSize: size }}>
                <p className="mb-3 font-medium">{size}px — {size === 12 ? "Field description / validation" : size === 14 ? "Body / label" : "Dialog heading"}</p>
                <p data-prose-comparison="" className="leading-relaxed">
                  Remove <InlineReference data-chip-kind="name">Main 控制台</InlineReference>, choose <InlineReference data-chip-kind="label">Skip 跳过</InlineReference>,
                  open <InlineReference monospace data-chip-kind="file">AGENTS.md</InlineReference> in <InlineReference monospace data-chip-kind="path">~/Projects</InlineReference>,
                  or type <InlineReference className="italic" data-chip-kind="confirmation">Delete 删除</InlineReference> to confirm.
                </p>
                <p className="mt-3 leading-relaxed">Long path: <InlineReference monospace wrap>/Users/dev/Projects/octoboard/packages/ui/src/components/InlineReference.tsx</InlineReference> stays within the paragraph.</p>
                <p dir="rtl" data-prose-rtl="" className="mt-3 leading-relaxed">افتح <InlineReference monospace>AGENTS.md</InlineReference> في <InlineReference monospace wrap>/Users/dev/Projects/octoboard</InlineReference> ثم اختر <InlineReference>Main</InlineReference>.</p>
              </div>
            ))}
          </section>
          <section className="flex flex-col gap-3" aria-labelledby="chip-sizes-title">
            <h2 id="chip-sizes-title" className="font-semibold">Sizes are padding steps, each in a line it fits</h2>
            <div className="grid gap-3 lg:grid-cols-2">
              {COPY_LINES.map(({ font, line, use, sizes }) => (
                <div key={`${font}/${line}`} data-chip-line-sample={`${font}/${line}`} className="flex min-w-0 flex-col items-start gap-3 rounded-lg border border-transparent shadow-(--panel-shadow) bg-surface p-3">
                  <h3 className="text-sm font-medium">{font}px text on {line}px lines</h3>
                  <p className="text-xs text-app-muted">{use}</p>
                  {sizes.map((size) => (
                    <p key={size} data-chip-size-sample={size} className="w-full min-w-0" style={{ fontSize: font, lineHeight: `${line}px` }}>
                      {size}: <InlineReference size={size}>Ready</InlineReference> <InlineReference size={size} monospace>AGENTS.md</InlineReference> <InlineReference size={size} monospace wrap>{LONG_PATHS[0]}</InlineReference> and <InlineReference size={size} monospace wrap>{LONG_PATHS[1]}</InlineReference>
                    </p>
                  ))}
                </div>
              ))}
            </div>
          </section>
          <section className="flex flex-col gap-3" aria-labelledby="status-badges-title">
            <h2 id="status-badges-title" className="font-semibold">Status badges: colour, variant and size</h2>
            <div className="grid gap-3 sm:grid-cols-2">
              {BADGE_VARIANTS.map((variant) => (
                <div key={variant} data-badge-variant-sample={variant} className="flex min-w-0 flex-col gap-2 rounded-lg border border-transparent shadow-(--panel-shadow) bg-surface p-3">
                  <h3 className="text-sm font-medium">{variant === "soft" ? "soft: borderless, for a state" : "bordered: a hairline edge, for a qualifier of the name"}</h3>
                  {BADGE_COLORS.map((color) => (
                    <p key={color} data-badge-sample={`${color}-${variant}`} className="flex min-w-0 items-center gap-2 text-sm">
                      <span className="w-16 shrink-0 text-app-muted">{color}</span>
                      <span className="min-w-0 truncate">Name before the badge</span>
                      <StatusBadge color={color} variant={variant} separated={false}>{color === "success" ? "Logged in" : color === "warning" ? "Not logged in" : color === "danger" ? "Deleted" : color === "accent" ? "Renamed" : "Pro"}</StatusBadge>
                    </p>
                  ))}
                </div>
              ))}
            </div>
            <div className="grid gap-3 sm:grid-cols-3">
              {BADGE_SIZES.map((size) => (
                <div key={size} data-badge-size-sample={size} className="flex min-w-0 flex-col items-start gap-3 rounded-lg border border-transparent shadow-(--panel-shadow) bg-surface p-3">
                  <h3 className="text-sm font-medium">{size}: {size === "sm" ? "20px, default" : size === "md" ? "24px" : "28px"}</h3>
                  {BADGE_VARIANTS.map((variant) => (
                    <StatusBadge key={variant} size={size} color="success" variant={variant} separated={false} className="inline-flex items-center gap-1">
                      <CircleCheck size={size === "lg" ? 16 : 12} aria-hidden="true" />
                      Granted
                    </StatusBadge>
                  ))}
                  <StatusBadge size={size} single separated={false}>M</StatusBadge>
                </div>
              ))}
            </div>
          </section>
          <section className="flex flex-col gap-3" aria-labelledby="status-marks-title">
            <h2 id="status-marks-title" className="font-semibold">Status marks: danger, warning and note</h2>
            <div className="grid gap-3 sm:grid-cols-3">
              <div data-status-mark-sample="danger" className="flex min-w-0 flex-col gap-2 rounded-lg border border-transparent shadow-(--panel-shadow) bg-surface p-3">
                <h3 className="text-sm font-medium">danger: the row has nothing to show</h3>
                <p className="flex items-center gap-1.5 text-sm">Default <StatusMark label="Usage unavailable">Claude Code took too long to report its usage.</StatusMark></p>
              </div>
              <div data-status-mark-sample="warning" className="flex min-w-0 flex-col gap-2 rounded-lg border border-transparent shadow-(--panel-shadow) bg-surface p-3">
                <h3 className="text-sm font-medium">warning: the row is usable but stale</h3>
                <p className="flex items-center gap-1.5 text-sm">Work <StatusMark tone="warning" label="Out of date: last read 12 minutes ago">Claude Code took too long to report its usage.</StatusMark></p>
              </div>
              <div data-status-mark-sample="bare" className="flex min-w-0 flex-col gap-2 rounded-lg border border-transparent shadow-(--panel-shadow) bg-surface p-3">
                <h3 className="text-sm font-medium">without a reason: the popover holds the name</h3>
                <p className="flex items-center gap-1.5 text-sm">Notes <StatusMark label="Failed" /></p>
              </div>
              <div data-status-mark-sample="word" className="flex min-w-0 flex-col gap-2 rounded-lg border border-transparent shadow-(--panel-shadow) bg-surface p-3">
                <h3 className="text-sm font-medium">a word and no reason: plain glyph and word</h3>
                <p className="flex items-center gap-1.5 text-sm">Website <StatusMark label="Website failed" text="Failed" /></p>
              </div>
              <div data-status-mark-sample="note" className="flex min-w-0 flex-col gap-2 rounded-lg border border-transparent shadow-(--panel-shadow) bg-surface p-3">
                <h3 className="text-sm font-medium">note: neither failed nor stale, a tooltip</h3>
                <p className="flex items-center gap-1.5 text-sm">Shared <StatusMark tone="note" label="Shared by two accounts">Both accounts read from this folder.</StatusMark></p>
              </div>
            </div>
          </section>
          <section className="flex flex-col gap-3" aria-labelledby="code-colors-title">
            <h2 id="code-colors-title" className="font-semibold">Six colors outside alerts</h2>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {COLORS.map((color) => (
                <div key={color} data-code-sample={color} className="min-w-0 rounded-lg border border-transparent shadow-(--panel-shadow) bg-surface p-3">
                  <p className="mb-2 text-sm">{color}: <InlineReference monospace wrap color={color}>VITE_DAEMON_PORT</InlineReference></p>
                  <CodeBlock color={color}>{"env -0\nOCTOBOARD_DAEMON_PORT=5174"}</CodeBlock>
                </div>
              ))}
            </div>
          </section>
          <section className="flex flex-col gap-3" aria-labelledby="alert-tones-title">
            <h2 id="alert-tones-title" className="font-semibold">Alert tone overrides each requested chip color</h2>
            {STATUSES.map((status) => (
              <CodeToneContext.Provider key={status} value={status}>
                <div className="flex flex-col gap-3" data-alert-tone-sample={status}>
                  <TintedAlert
                    status={status}
                    data-alert-sample={status}
                    title={status}
                    description={COLORS.map((color, index) => <React.Fragment key={color}>{index > 0 && "; "}{color}: <InlineReference monospace wrap color={color}>CLAUDE.md</InlineReference></React.Fragment>)}
                  />
                  <CodeBlock>{"env -0 && printf '%s' 'OCTOBOARD-ENV-SNAPSHOT-END-12345678-1234-1234-1234-123456789abc'"}</CodeBlock>
                </div>
              </CodeToneContext.Provider>
            ))}
          </section>
          <section className="flex flex-col gap-3 rounded-lg bg-surface p-3" aria-labelledby="nested-tones-title">
            <h2 id="nested-tones-title" className="font-semibold">Nearest alert wins; outside chips keep their own tone</h2>
            <CodeToneContext.Provider value="warning">
              <TintedAlert
                status="warning"
                data-nested-sample="outer"
                title="Outer warning"
                description={<>Outer warning: <InlineReference monospace wrap color="danger">AGENTS.md</InlineReference></>}
              />
              <TintedAlert
                status="success"
                data-nested-sample="inner"
                title="Inner success"
                description={<>Inner success: <InlineReference monospace wrap color="danger">.claude</InlineReference></>}
              />
              <p>Outer warning again: <InlineReference monospace wrap color="accent">.codex</InlineReference></p>
            </CodeToneContext.Provider>
            <p>After both alerts: <InlineReference monospace wrap color="help">~/Projects</InlineReference></p>
          </section>
        </div>
      </FadedScroller>
    </main>
  );
}
