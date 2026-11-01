import React, { useEffect, useState } from "react";

import { StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { useHasPainted } from "../components/motion/useHasPainted";
import { useT } from "../i18n/react";
import { useOctoboardTheme } from "../preferences/theme";
import { BodyView } from "./BodyView";
import { ChangeView } from "./ChangeView";
import type { ViewerContent, ViewerSubject } from "./content";
import type { DiffLayout } from "./diffLayout";
import { FIRST_ANNOUNCEMENT_DELAY_MS } from "./notices";
import { Disconnected, DISCONNECTED, Failure, Loading, ViewerFirstContentContext, ViewerPaintedContext } from "./viewerStates";

/** Whether the content is still on its way: the file or change being read, or a conflict waiting
 * for its file. */
function isWaiting(content: ViewerContent): boolean {
  return content.state === "loading" || (content.state === "conflict" && !content.body && content.message === undefined);
}

export function ViewerContentView(props: React.ComponentProps<typeof ViewerContentBody>): React.ReactElement {
  const t = useT();
  const { content } = props.subject;
  const painted = useHasPainted();
  // Set once the first content that is not the wait for it has been drawn.
  const [settled, setSettled] = useState(false);
  useEffect(() => {
    if (!isWaiting(content)) setSettled(true);
  }, [content]);
  const announcement =
    isWaiting(content)
      ? t("viewer.loading")
      : content.state === "disconnected"
        ? t(DISCONNECTED[content.what].sentence)
        : undefined;
  // Outside the body, which swaps its whole tree as the content arrives, so the region is there
  // before its text is.
  return (
    <>
      <StatusAnnouncer text={announcement} firstWriteDelayMs={FIRST_ANNOUNCEMENT_DELAY_MS} />
      <ViewerPaintedContext value={painted}>
        <ViewerFirstContentContext value={!settled}>
          <ViewerContentBody {...props} />
        </ViewerFirstContentContext>
      </ViewerPaintedContext>
    </>
  );
}

function ViewerContentBody({
  subject,
  name,
  layout,
  onLayoutChange,
  line,
}: {
  subject: ViewerSubject;
  name: string;
  layout: DiffLayout;
  onLayoutChange: (layout: DiffLayout) => void;
  line?: number;
}): React.ReactElement {
  const t = useT();
  const { resolved: theme } = useOctoboardTheme();
  const { content, key } = subject;
  switch (content.state) {
    case "loading":
      return <Loading />;
    case "error":
      return <Failure title={t("viewer.error")} message={content.message} path={subject.path} />;
    case "disconnected":
      return <Disconnected what={content.what} />;
    case "file":
      return <BodyView resetKey={key} name={name} body={content.body} theme={theme} asDocument line={line} />;
    case "conflict":
      // The conflict and its kind are the title's status badge; a conflicted file that could not be read
      // is that failure alone.
      if (content.body) return <BodyView resetKey={key} name={name} body={content.body} theme={theme} />;
      return content.message !== undefined ? <Failure title={t("viewer.error")} message={content.message} path={subject.path} /> : <Loading />;
    case "change":
      return (
        <ChangeView
          resetKey={key}
          path={subject.path}
          name={name}
          change={content.change}
          layout={layout}
          onLayoutChange={onLayoutChange}
          theme={theme}
        />
      );
  }
}
