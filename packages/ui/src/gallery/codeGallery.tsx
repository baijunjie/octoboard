import { Alert } from "@heroui/react";
import React from "react";

import { CodeBlock } from "../components/CodeBlock";
import { InlineChip, type InlineChipSize } from "../components/InlineChip";
import { TintedAlert } from "../components/TintedAlert";
import { CodeToneContext, type CodeColor } from "../components/codeTone";
import { FadedScroller, useFadeRegion } from "../components/scrollFade";

const COLORS: CodeColor[] = ["default", "primary", "secondary", "success", "warning", "danger"];
const SIZES: InlineChipSize[] = ["xs", "sm", "md", "lg"];
const STATUSES = ["default", "accent", "success", "warning", "danger"] as const;

/** One settled comparison of the shared inline chips and code blocks, both outside and inside alert contexts. */
export function CodeGallery(): React.ReactElement {
  const region = useFadeRegion({ label: "Inline chips, sizes and alert tones" });
  return (
    <main className={`flex h-full min-h-0 flex-col ${region.focused ? "ring-2 ring-app-focus ring-inset" : ""}`}>
      <FadedScroller region={region} className="min-h-0 flex-1 p-6">
        <div className="mx-auto flex max-w-5xl flex-col gap-6">
          <h1 className="text-lg font-semibold">Inline chips, sizes and alert tones</h1>
          <section className="flex flex-col gap-3" aria-labelledby="chip-prose-title">
            <h2 id="chip-prose-title" className="font-semibold">Inherited prose: equal size, height and baseline</h2>
            {[12, 14, 16].map((size) => (
              <div key={size} data-prose-size={size} className="min-w-0 rounded-lg border border-transparent shadow-(--panel-shadow) bg-surface p-3" style={{ fontSize: size }}>
                <p className="mb-3 font-medium">{size}px — {size === 12 ? "Field description / validation" : size === 14 ? "Body / label" : "Dialog heading"}</p>
                <p data-prose-comparison="" className="leading-relaxed">
                  Remove <InlineChip size="inherit" data-chip-kind="name">Main 控制台</InlineChip>, choose <InlineChip size="inherit" data-chip-kind="label">Skip 跳过</InlineChip>,
                  open <InlineChip size="inherit" monospace data-chip-kind="file">AGENTS.md</InlineChip> in <InlineChip size="inherit" monospace data-chip-kind="path">~/Projects</InlineChip>,
                  or type <InlineChip size="inherit" className="italic" data-chip-kind="confirmation">Delete 删除</InlineChip> to confirm.
                </p>
                <p className="mt-3 leading-relaxed">Long path: <InlineChip size="inherit" monospace wrap>/Users/dev/Projects/octoboard/packages/ui/src/components/InlineChip.tsx</InlineChip> stays within the paragraph.</p>
                <p dir="rtl" data-prose-rtl="" className="mt-3 leading-relaxed">افتح <InlineChip size="inherit" monospace>AGENTS.md</InlineChip> في <InlineChip size="inherit" monospace wrap>/Users/dev/Projects/octoboard</InlineChip> ثم اختر <InlineChip size="inherit">Main</InlineChip>.</p>
              </div>
            ))}
          </section>
          <section className="flex flex-col gap-3" aria-labelledby="chip-sizes-title">
            <h2 id="chip-sizes-title" className="font-semibold">Four sizes, regular and monospace</h2>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
              {SIZES.map((size) => (
                <div key={size} data-chip-size-sample={size} className="flex min-w-0 flex-col items-start gap-3 rounded-lg border border-transparent shadow-(--panel-shadow) bg-surface p-3">
                  <h3 className="text-sm font-medium">{size}</h3>
                  <InlineChip size={size}>Ready</InlineChip>
                  <InlineChip size={size} monospace>AGENTS.md</InlineChip>
                  <p className="w-full min-w-0 text-sm">Path: <InlineChip size={size} monospace wrap>/Users/dev/Projects/octoboard/packages/ui/src/components/InlineChip.tsx</InlineChip></p>
                </div>
              ))}
            </div>
          </section>
          <section className="flex flex-col gap-3" aria-labelledby="code-colors-title">
            <h2 id="code-colors-title" className="font-semibold">Six colors outside alerts</h2>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {COLORS.map((color) => (
                <div key={color} data-code-sample={color} className="min-w-0 rounded-lg border border-transparent shadow-(--panel-shadow) bg-surface p-3">
                  <p className="mb-2 text-sm">{color}: <InlineChip monospace wrap size="inherit" color={color}>VITE_DAEMON_PORT</InlineChip></p>
                  <CodeBlock color={color}>{"env -0\nOCTOBOARD_DAEMON_PORT=5174"}</CodeBlock>
                </div>
              ))}
            </div>
          </section>
          <section className="flex flex-col gap-3" aria-labelledby="alert-tones-title">
            <h2 id="alert-tones-title" className="font-semibold">Alert tone overrides each requested chip color</h2>
            {STATUSES.map((status) => (
              <CodeToneContext.Provider key={status} value={status === "accent" ? "primary" : status}>
                <div className="flex flex-col gap-3" data-alert-tone-sample={status}>
                  <TintedAlert status={status} data-alert-sample={status}>
                    <Alert.Indicator aria-hidden="true" />
                    <Alert.Content>
                      <Alert.Title>{status}</Alert.Title>
                      <Alert.Description>
                        {COLORS.map((color, index) => <React.Fragment key={color}>{index > 0 && "; "}{color}: <InlineChip monospace wrap size="inherit" color={color}>CLAUDE.md</InlineChip></React.Fragment>)}
                      </Alert.Description>
                    </Alert.Content>
                  </TintedAlert>
                  <CodeBlock>{"env -0 && printf '%s' 'OCTOBOARD-ENV-SNAPSHOT-END-12345678-1234-1234-1234-123456789abc'"}</CodeBlock>
                </div>
              </CodeToneContext.Provider>
            ))}
          </section>
          <section className="flex flex-col gap-3 rounded-lg bg-surface p-3" aria-labelledby="nested-tones-title">
            <h2 id="nested-tones-title" className="font-semibold">Nearest alert wins; outside chips keep their own tone</h2>
            <CodeToneContext.Provider value="warning">
              <TintedAlert status="warning" data-nested-sample="outer">
                <Alert.Indicator aria-hidden="true" />
                <Alert.Content>
                  <Alert.Title>Outer warning</Alert.Title>
                  <Alert.Description>Outer warning: <InlineChip monospace wrap size="inherit" color="danger">AGENTS.md</InlineChip></Alert.Description>
                </Alert.Content>
              </TintedAlert>
              <TintedAlert status="success" data-nested-sample="inner">
                <Alert.Indicator aria-hidden="true" />
                <Alert.Content>
                  <Alert.Title>Inner success</Alert.Title>
                  <Alert.Description>Inner success: <InlineChip monospace wrap size="inherit" color="danger">.claude</InlineChip></Alert.Description>
                </Alert.Content>
              </TintedAlert>
              <p>Outer warning again: <InlineChip monospace wrap size="inherit" color="primary">.codex</InlineChip></p>
            </CodeToneContext.Provider>
            <p>After both alerts: <InlineChip monospace wrap size="inherit" color="secondary">~/Projects</InlineChip></p>
          </section>
        </div>
      </FadedScroller>
    </main>
  );
}
