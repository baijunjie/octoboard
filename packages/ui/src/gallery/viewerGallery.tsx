// The page a file-viewer scenario renders in place of the app: a list of the scenario's subjects,
// each opening the real `FileViewer`, whose Previous / Next walk the same list in its order. The
// viewer opens on the first subject when the page loads.
import { Button } from "@heroui/react";
import React, { useEffect, useRef, useState } from "react";

import type { ViewerSubject } from "../viewer/content";
import { FileViewer } from "../viewer/FileViewer";
import { displayWirePath } from "../wirePath";

export function ViewerGallery({ subjects, loadDelay }: { subjects: ViewerSubject[]; loadDelay?: number }): React.ReactElement {
  const [open, setOpen] = useState<number | null>(0);
  const [loaded, setLoaded] = useState<number | null>(loadDelay ? null : 0);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const show = (index: number | null) => {
    setOpen(index);
    clearTimeout(timer.current);
    if (!loadDelay) return setLoaded(index);
    setLoaded(null);
    timer.current = setTimeout(() => setLoaded(index), loadDelay);
  };
  useEffect(() => {
    if (loadDelay) show(0);
    return () => clearTimeout(timer.current);
  }, []);

  const subject = open === null ? undefined : subjects[open];
  const shown: ViewerSubject | undefined =
    subject && (loaded === open ? subject : { ...subject, content: { state: "loading" } });

  return (
    <main className="flex flex-col items-start gap-1 p-6">
      <h1 className="pb-2 text-base font-medium">Subjects</h1>
      {subjects.map((s, index) => (
        <Button key={s.key} variant="ghost" size="sm" onPress={() => show(index)}>
          <span dir="ltr">{displayWirePath(s.path)}</span>
          {s.source && <span className="text-muted">{s.source}</span>}
        </Button>
      ))}
      {shown && open !== null && (
        <FileViewer
          subject={shown}
          onClose={() => show(null)}
          navigation={{
            onPrevious: open > 0 ? () => show(open - 1) : undefined,
            onNext: open < subjects.length - 1 ? () => show(open + 1) : undefined,
          }}
        />
      )}
    </main>
  );
}
