import React from "react";

import { Button } from "../primitives";
import { FadedScroller, useFadeRegion } from "../components/scroll/scrollFade";
import { ExternalLink } from "../components/text/ExternalLink";
import { AvatarField } from "../dialogs/AvatarField";

const SIZES = ["xs", "sm", "md", "lg"] as const;
const VARIANTS = ["primary", "secondary", "danger"] as const;
const LINK_SIZES = ["xs", "sm"] as const;
const LINK_VARIANTS = ["secondary", "outline"] as const;

/** Each size and variant beside the same button idle, so the centered pending indicator and unchanged width can be
 * compared, plus the link drawn as a button (`ExternalLink`) with its leading spinner and the console dialog's
 * icon-only avatar field. Real pending actions live in the scenarios of their dialogs and rows; no screen holds a
 * pending button of every size at once. */
export function ButtonsGallery(): React.ReactElement {
  const region = useFadeRegion({ label: "Pending buttons" });
  return (
    <main className={`flex h-full min-h-0 flex-col ${region.insetRing}`}>
      <FadedScroller region={region} className="min-h-0 flex-1 p-6">
        <div className="mx-auto flex max-w-3xl flex-col gap-8">
          <h1 className="text-lg font-semibold">Pending buttons</h1>
          <section className="flex flex-col gap-4" aria-labelledby="pending-sizes-title">
            <h2 id="pending-sizes-title" className="font-semibold">Each size and variant, idle then pending</h2>
            <div className="flex flex-col gap-4 rounded-lg bg-surface p-4 shadow-(--panel-shadow)">
              {SIZES.map((size) => (
                <div key={size} className="flex flex-wrap items-center gap-x-6 gap-y-3">
                  <span className="w-10 text-sm text-app-muted">{size}</span>
                  {VARIANTS.map((variant) => (
                    <span key={variant} className="flex items-center gap-2">
                      <Button size={size} variant={variant}>Save</Button>
                      <Button size={size} variant={variant} pending>Save</Button>
                    </span>
                  ))}
                </div>
              ))}
            </div>
          </section>
          <section className="flex flex-col gap-4" aria-labelledby="pending-link-title">
            <h2 id="pending-link-title" className="font-semibold">Link drawn as a button, idle then pending</h2>
            <div className="flex flex-col gap-4 rounded-lg bg-surface p-4 shadow-(--panel-shadow)">
              {LINK_SIZES.map((size) => (
                <div key={size} className="flex flex-wrap items-center gap-x-6 gap-y-3">
                  <span className="w-10 text-sm text-app-muted">{size}</span>
                  {LINK_VARIANTS.map((variant) => (
                    <span key={variant} className="flex items-center gap-2">
                      <ExternalLink appearance="button" size={size} variant={variant} href="https://example.com/">Learn more</ExternalLink>
                      <ExternalLink appearance="button" size={size} variant={variant} href="https://example.com/" isPending>Learn more</ExternalLink>
                    </span>
                  ))}
                </div>
              ))}
            </div>
          </section>
          <section className="flex flex-col gap-4" aria-labelledby="pending-icon-title">
            <h2 id="pending-icon-title" className="font-semibold">Icon only: the avatar field, idle then reading a picked image</h2>
            <div className="flex items-center gap-6 rounded-lg bg-surface p-4 shadow-(--panel-shadow)">
              <AvatarField icon={null} onPick={() => {}} onRemove={() => {}} />
              <AvatarField icon={null} onPick={() => {}} onRemove={() => {}} isPending />
            </div>
          </section>
        </div>
      </FadedScroller>
    </main>
  );
}
