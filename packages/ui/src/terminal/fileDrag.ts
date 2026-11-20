/** A file the user dropped on a terminal, as the page's `drop` event gave it. */
export interface DroppedFile {
  file: File;
  /** The dropped item is a folder, which a page cannot read (the `File` is only a placeholder). */
  folder: boolean;
}

/** Whether a drag carries files, which is all a page can tell of it before the drop. */
export function dragsFiles(data: DataTransfer | null): boolean {
  return Array.from(data?.types ?? []).includes("Files");
}

/** What a drop carries. Read in the `drop` event itself: the data store is emptied once the handler returns, and with
 * it what tells a folder from a file. */
export function filesOfDrop(data: DataTransfer): DroppedFile[] {
  return Array.from(data.items)
    .filter((item) => item.kind === "file")
    .flatMap((item) => {
      const file = item.getAsFile();
      return file ? [{ file, folder: item.webkitGetAsEntry?.()?.isDirectory === true }] : [];
    });
}
