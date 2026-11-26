// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { CodedError } from "../errors/codedError";
import { droppedFileEntries } from "../errors/droppedFileEntries";
import { translatorFor } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { AttachmentLimits } from "../protocol";
import { looseRequestSender, type RequestHandler } from "../requests";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { afterQueuedWork } from "../test/clock";
import { dropUploads } from "./dropUploads";
import { droppedPathsText, dropFilesHandler, uploadFiles } from "./droppedFiles";
import { dragsFiles, filesOfDrop, type DroppedFile } from "./fileDrag";
import { MAX_PASTED_IMAGE_BYTES } from "./pastedImage";

afterEach(() => {
  vi.restoreAllMocks();
  acknowledgeAllAlerts();
});

const t = translatorFor("en");
const base64 = (text: string) => Buffer.from(text).toString("base64");

describe("the text pasted for dropped paths", () => {
  it("is the path and one space after it", () => {
    expect(droppedPathsText(["/Users/u/notes.txt"])).toBe("/Users/u/notes.txt ");
  });

  it("joins several paths with single spaces and ends with one space", () => {
    expect(droppedPathsText(["/a/one", "/b/two", "/c/three"])).toBe("/a/one /b/two /c/three ");
  });

  it("wraps only a path with whitespace in single quotes", () => {
    expect(droppedPathsText(["/Users/some one/a b.txt", "/plain/path"])).toBe("'/Users/some one/a b.txt' /plain/path ");
  });

  it("writes a single quote inside a quoted path as '\\''", () => {
    expect(droppedPathsText(["/tmp/it's here.txt"])).toBe("'/tmp/it'\\''s here.txt' ");
  });
});

describe("whether a drag carries files", () => {
  it.each([
    ["no data", null, false],
    ["no types", { types: [] }, false],
    ["only text", { types: ["text/plain", "text/uri-list"] }, false],
    ["files", { types: ["Files"] }, true],
    ["files among other types", { types: ["text/plain", "Files"] }, true],
  ])("is decided by the types of %s", (_name, data, expected) => {
    expect(dragsFiles(data as unknown as DataTransfer)).toBe(expected);
  });
});

describe("the files a drop carries", () => {
  type Item = { kind: string; getAsFile: () => File | null; webkitGetAsEntry?: () => { isDirectory: boolean } | null };
  const dataOf = (items: Item[]) => ({ items }) as unknown as DataTransfer;
  const item = (file: File | null, entry?: { isDirectory: boolean } | null): Item => ({
    kind: "file",
    getAsFile: () => file,
    webkitGetAsEntry: entry === undefined ? undefined : () => entry,
  });

  it("are the file items, each with whether it is a folder", () => {
    const file = new File(["x"], "a.txt");
    const folder = new File([""], "docs");
    const result = filesOfDrop(dataOf([item(file, { isDirectory: false }), item(folder, { isDirectory: true })]));
    expect(result).toEqual([
      { file, folder: false },
      { file: folder, folder: true },
    ]);
    expect(result[0].file).toBe(file);
    expect(result[1].file).toBe(folder);
  });

  it("leave out items that are not files, and files the item cannot give", () => {
    const file = new File(["x"], "a.txt");
    const result = filesOfDrop(dataOf([{ kind: "string", getAsFile: () => new File(["no"], "no.txt") }, item(null, { isDirectory: false }), item(file, { isDirectory: false })]));
    expect(result.map((dropped) => dropped.file)).toEqual([file]);
  });

  it("take an item as a file where the entry is unavailable", () => {
    const file = new File(["x"], "a.txt");
    expect(filesOfDrop(dataOf([item(file), item(file, null)])).map((dropped) => dropped.folder)).toEqual([false, false]);
  });

  it("are none for a drop with no items", () => {
    expect(filesOfDrop(dataOf([]))).toEqual([]);
  });
});

describe("uploading dropped files", () => {
  const LIMITS: AttachmentLimits = { max_bytes: 1000, max_part_bytes: 4 };
  const alerts = () => errorAlerts.getSnapshot();
  const file = (name: string, content: string, type = "") => ({ file: new File([content], name, { type }), folder: false }) satisfies DroppedFile;

  type Body = Parameters<RequestHandler>[0];

  /** The handlers over a stand-in daemon: `fail` rejects the requests it names, and every request is recorded. */
  function setup({ limits = LIMITS as AttachmentLimits | null, fail = {} as Record<string, unknown>, gate, takesImages = true }: { limits?: AttachmentLimits | null; fail?: Record<string, unknown>; gate?: (body: Body) => Promise<void>; takesImages?: boolean } = {}) {
    const requests: Body[] = [];
    let uploads = 0;
    let inFlight = 0;
    let mostInFlight = 0;
    const handler: RequestHandler = async (body) => {
      requests.push(body);
      inFlight += 1;
      mostInFlight = Math.max(mostInFlight, inFlight);
      try {
        await gate?.(body);
        if (body.type in fail) throw fail[body.type];
        switch (body.type) {
          case "save_pasted_image":
            return { type: "pasted_image_saved", id: null, path: `/run/${body.session}/pasted/image.png` };
          case "create_attachment_upload":
            uploads += 1;
            return { type: "attachment_upload_created", id: null, upload: `up-${uploads}` };
          case "write_attachment_part":
          case "delete_attachment_upload":
            return { type: "ack", id: null };
          case "save_attachment":
            return { type: "attachment_saved", id: null, path: `/run/${body.session}/attachments/${body.upload}.bin` };
          default:
            throw new Error(`unexpected request ${body.type}`);
        }
      } finally {
        inFlight -= 1;
      }
    };
    const request = looseRequestSender(handler);
    let current = { t, request, attachmentLimits: limits, takesImages: () => takesImages };
    const latest = vi.fn(() => current);
    return {
      requests,
      latest,
      mostInFlight: () => mostInFlight,
      setLimits: (next: AttachmentLimits | null) => {
        current = { ...current, attachmentLimits: next };
      },
      upload: (files: DroppedFile[], session = "s-1", wanted?: () => boolean) => dropFilesHandler(latest).upload(files, session, wanted),
    };
  }

  describe("an image the daemon takes as it is", () => {
    it.each(["image/png", "image/jpeg", "image/gif", "image/webp"])("is saved as a pasted image (%s), and its path answered", async (type) => {
      const { requests, upload } = setup();
      await expect(upload([file("shot.img", "abc", type)])).resolves.toBe("/run/s-1/pasted/image.png ");
      expect(requests).toEqual([{ type: "save_pasted_image", session: "s-1", data: base64("abc") }]);
      expect(alerts()).toEqual([]);
    });

    it("is saved the same way where the daemon takes no attachments at all", async () => {
      const { requests, upload } = setup({ limits: null });
      await expect(upload([file("shot.png", "abc", "image/png")])).resolves.toBe("/run/s-1/pasted/image.png ");
      expect(requests.map((body) => body.type)).toEqual(["save_pasted_image"]);
      expect(alerts()).toEqual([]);
    });

    it("of exactly 10 MiB is saved as a pasted image", async () => {
      const { requests, upload } = setup({ limits: { max_bytes: 100 * 1024 * 1024, max_part_bytes: 1024 * 1024 } });
      const image = { file: new File([new Uint8Array(MAX_PASTED_IMAGE_BYTES)], "big.png", { type: "image/png" }), folder: false };
      await expect(upload([image])).resolves.toBe("/run/s-1/pasted/image.png ");
      expect(requests.map((body) => body.type)).toEqual(["save_pasted_image"]);
    });

    it("over 10 MiB is attached as any other file", async () => {
      const { requests, upload } = setup({ limits: { max_bytes: 100 * 1024 * 1024, max_part_bytes: 4 * 1024 * 1024 } });
      const image = { file: new File([new Uint8Array(MAX_PASTED_IMAGE_BYTES + 1)], "big.png", { type: "image/png" }), folder: false };
      await expect(upload([image])).resolves.toBe("/run/s-1/attachments/up-1.bin ");
      expect(requests.map((body) => body.type)).toEqual(["create_attachment_upload", "write_attachment_part", "write_attachment_part", "write_attachment_part", "save_attachment"]);
    });

    it("is refused where it is over 10 MiB and the daemon takes no attachments", async () => {
      const { requests, upload } = setup({ limits: null });
      const image = { file: new File([new Uint8Array(MAX_PASTED_IMAGE_BYTES + 1)], "big.png", { type: "image/png" }), folder: false };
      await expect(upload([image])).resolves.toBeUndefined();
      expect(requests).toEqual([]);
      expect(alerts()).toHaveLength(1);
    });

    it("is pasted with a path that has whitespace quoted once", async () => {
      const spaced = dropFilesHandler(() => ({
        t,
        attachmentLimits: LIMITS,
        takesImages: () => true,
        request: looseRequestSender(async () => ({ type: "pasted_image_saved", id: null, path: "/Users/some one/run/pasted/p.png" })),
      }));
      await expect(spaced.upload([file("shot.png", "abc", "image/png")], "s-1")).resolves.toBe("'/Users/some one/run/pasted/p.png' ");
    });

    it("raises an alert and pastes nothing when the daemon refuses it", async () => {
      const { upload } = setup({ fail: { save_pasted_image: new Error("not an image") } });
      await expect(upload([file("shot.png", "abc", "image/png")])).resolves.toBeUndefined();
      expect(alerts()).toHaveLength(1);
      expect(alerts()[0]).toMatchObject({ title: "Could not attach the file", session: "s-1" });
    });
  });

  describe("an image dropped on a session whose agent takes no images", () => {
    it("is attached as any other file, never saved as a pasted image", async () => {
      const { requests, upload } = setup({ takesImages: false });
      await expect(upload([file("shot.png", "abc", "image/png")])).resolves.toBe("/run/s-1/attachments/up-1.bin ");
      expect(requests.map((body) => body.type)).toEqual(["create_attachment_upload", "write_attachment_part", "save_attachment"]);
      expect(alerts()).toEqual([]);
    });

    it("is refused as a file the daemon cannot take where the daemon takes no attachments at all", async () => {
      const { requests, upload } = setup({ takesImages: false, limits: null });
      await expect(upload([file("shot.png", "abc", "image/png")])).resolves.toBeUndefined();
      expect(requests).toEqual([]);
      expect(alerts()).toHaveLength(1);
    });
  });

  describe("any other file", () => {
    it("is created as an upload, written in parts of at most the daemon's part size at advancing offsets, then saved, and its path answered", async () => {
      const { requests, upload } = setup();
      await expect(upload([file("notes.txt", "0123456789", "text/plain")])).resolves.toBe("/run/s-1/attachments/up-1.bin ");
      expect(requests).toEqual([
        { type: "create_attachment_upload", session: "s-1", name: "notes.txt", size: 10 },
        { type: "write_attachment_part", upload: "up-1", offset: 0, data: base64("0123") },
        { type: "write_attachment_part", upload: "up-1", offset: 4, data: base64("4567") },
        { type: "write_attachment_part", upload: "up-1", offset: 8, data: base64("89") },
        { type: "save_attachment", session: "s-1", upload: "up-1" },
      ]);
      expect(alerts()).toEqual([]);
    });

    it("of exactly a whole number of parts sends no empty part", async () => {
      const { requests, upload } = setup();
      await upload([file("a.bin", "01234567")]);
      expect(requests.filter((body) => body.type === "write_attachment_part")).toEqual([
        { type: "write_attachment_part", upload: "up-1", offset: 0, data: base64("0123") },
        { type: "write_attachment_part", upload: "up-1", offset: 4, data: base64("4567") },
      ]);
    });

    it("smaller than a part is written in one", async () => {
      const { requests, upload } = setup();
      await upload([file("a.bin", "ab")]);
      expect(requests.filter((body) => body.type === "write_attachment_part")).toEqual([{ type: "write_attachment_part", upload: "up-1", offset: 0, data: base64("ab") }]);
    });

    it("of exactly the daemon's limit is taken", async () => {
      const { requests, upload } = setup({ limits: { max_bytes: 6, max_part_bytes: 4 } });
      await expect(upload([file("a.bin", "012345")])).resolves.toBe("/run/s-1/attachments/up-1.bin ");
      expect(requests[0]).toMatchObject({ type: "create_attachment_upload", size: 6 });
    });

    it("quotes a path with whitespace", async () => {
      const quoted = dropFilesHandler(() => ({
        t,
        attachmentLimits: LIMITS,
        takesImages: () => true,
        request: looseRequestSender(async (body) => {
          if (body.type === "create_attachment_upload") return { type: "attachment_upload_created", id: null, upload: "u" };
          if (body.type === "save_attachment") return { type: "attachment_saved", id: null, path: "/Users/some one/run/a b.bin" };
          return { type: "ack", id: null };
        }),
      }));
      await expect(quoted.upload([file("a b.bin", "x")], "s-1")).resolves.toBe("'/Users/some one/run/a b.bin' ");
    });

    it("discards the upload, and raises an alert, when a part is refused", async () => {
      const { requests, upload } = setup({ fail: { write_attachment_part: new Error("disk full") } });
      await expect(upload([file("a.bin", "0123456789")])).resolves.toBeUndefined();
      expect(requests.map((body) => body.type)).toEqual(["create_attachment_upload", "write_attachment_part", "delete_attachment_upload"]);
      expect(requests.at(-1)).toEqual({ type: "delete_attachment_upload", upload: "up-1" });
      expect(alerts()).toHaveLength(1);
      expect(alerts()[0]).toMatchObject({ title: "Could not attach the file", session: "s-1" });
    });

    it("discards the upload when saving it is refused", async () => {
      const { requests, upload } = setup({ fail: { save_attachment: new Error("refused") } });
      await expect(upload([file("a.bin", "abc")])).resolves.toBeUndefined();
      expect(requests.map((body) => body.type)).toEqual(["create_attachment_upload", "write_attachment_part", "save_attachment", "delete_attachment_upload"]);
      expect(requests.at(-1)).toEqual({ type: "delete_attachment_upload", upload: "up-1" });
      expect(alerts()).toHaveLength(1);
    });

    it("raises the original failure even when discarding the upload fails too", async () => {
      const { upload } = setup({ fail: { write_attachment_part: new Error("disk full"), delete_attachment_upload: new Error("gone") } });
      await expect(upload([file("a.bin", "abc")])).resolves.toBeUndefined();
      expect(alerts()).toHaveLength(1);
    });

    it("sends no discard where the upload could not even be created, and raises an alert", async () => {
      const { requests, upload } = setup({ fail: { create_attachment_upload: new Error("refused") } });
      await expect(upload([file("a.bin", "abc")])).resolves.toBeUndefined();
      expect(requests.map((body) => body.type)).toEqual(["create_attachment_upload"]);
      expect(alerts()).toHaveLength(1);
    });

    it("is discarded, with an alert saying it could not be read, when its bytes cannot be read", async () => {
      vi.spyOn(FileReader.prototype, "readAsDataURL").mockImplementation(function (this: FileReader) {
        this.onerror?.(new ProgressEvent("error") as ProgressEvent<FileReader>);
      });
      const { requests, upload } = setup();
      await expect(upload([file("broken.bin", "abc")])).resolves.toBeUndefined();
      expect(requests.map((body) => body.type)).toEqual(["create_attachment_upload", "delete_attachment_upload"]);
      expect(alerts()).toHaveLength(1);
      expect(alerts()[0]).toMatchObject({ title: "Could not attach the file", message: `${isolate("broken.bin")} could not be read.` });
    });
  });

  describe("a file refused before anything is sent", () => {
    it("is a folder", async () => {
      const { requests, upload } = setup();
      await expect(upload([{ file: new File([""], "docs"), folder: true }])).resolves.toBeUndefined();
      expect(requests).toEqual([]);
      expect(alerts()).toHaveLength(1);
      expect(alerts()[0]).toMatchObject({ title: "Could not attach the file", message: `${isolate("docs")} is a folder, and only files can be attached.`, session: "s-1" });
    });

    it("is empty", async () => {
      const { requests, upload } = setup();
      await expect(upload([file("empty.txt", "")])).resolves.toBeUndefined();
      expect(requests).toEqual([]);
      expect(alerts()[0]).toMatchObject({ title: "Could not attach the file", message: `${isolate("empty.txt")} is empty.` });
    });

    it("is an empty image, which is no image to save", async () => {
      const { requests, upload } = setup();
      await expect(upload([file("empty.png", "", "image/png")])).resolves.toBeUndefined();
      expect(requests).toEqual([]);
      expect(alerts()[0]).toMatchObject({ message: `${isolate("empty.png")} is empty.` });
    });

    it("is not one of the images the daemon takes where its snapshot has no attachment limits, said as such, naming the daemon's age and the 10 MiB limit", async () => {
      const { requests, upload } = setup({ limits: null });
      await expect(upload([file("notes.txt", "abc", "text/plain")])).resolves.toBeUndefined();
      expect(requests).toEqual([]);
      expect(alerts()).toHaveLength(1);
      expect(alerts()[0]).toMatchObject({ title: "Could not attach the file", error: { code: "dropped_file_images_only", params: { name: "notes.txt", limit_mib: "10" } } });
      const { message } = alerts()[0];
      expect(message).toContain(isolate("notes.txt"));
      expect(message).toContain("too old");
      expect(message).toContain("PNG, JPEG, GIF and WebP");
      expect(message).toContain("10 MiB");
      expect(message).not.toContain("not an image");
    });

    it("is an image of another type where the daemon's snapshot has no attachment limits, which is refused the same way", async () => {
      const { requests, upload } = setup({ limits: null });
      await expect(upload([file("pic.bmp", "abc", "image/bmp")])).resolves.toBeUndefined();
      expect(requests).toEqual([]);
      expect(alerts()[0]).toMatchObject({ error: { code: "dropped_file_images_only" } });
    });

    it("is an image over 10 MiB where the daemon's snapshot has no attachment limits, which is refused the same way", async () => {
      const { requests, upload } = setup({ limits: null });
      const big = { file: new File([new Uint8Array(MAX_PASTED_IMAGE_BYTES + 1)], "big.png", { type: "image/png" }), folder: false };
      await expect(upload([big])).resolves.toBeUndefined();
      expect(requests).toEqual([]);
      expect(alerts()[0]).toMatchObject({ error: { code: "dropped_file_images_only", params: { limit_mib: "10" } } });
    });

    it("is not an image and larger than the daemon's limit, which the alert names in MiB", async () => {
      const { requests, upload } = setup({ limits: { max_bytes: 3 * 1024 * 1024 + 512, max_part_bytes: 1024 } });
      const big = { file: new File([new Uint8Array(3 * 1024 * 1024 + 513)], "big.zip"), folder: false };
      await expect(upload([big])).resolves.toBeUndefined();
      expect(requests).toEqual([]);
      expect(alerts()[0]).toMatchObject({ title: "Could not attach the file", message: `${isolate("big.zip")} is larger than 3 MiB.` });
    });
  });

  describe("several files", () => {
    it("are uploaded one after another, never two uploads at once", async () => {
      let release!: () => void;
      const held = new Promise<void>((resolve) => {
        release = resolve;
      });
      const { requests, mostInFlight, upload } = setup({
        gate: async (body) => {
          if (body.type === "save_attachment" && body.upload === "up-1") await held;
        },
      });
      const done = upload([file("first.bin", "abcdef"), file("second.bin", "ghi")]);
      await vi.waitFor(() => expect(requests.at(-1)).toMatchObject({ type: "save_attachment", upload: "up-1" }));
      // Give anything wrongly started a chance to be sent.
      await afterQueuedWork();
      expect(requests.filter((body) => body.type === "create_attachment_upload")).toHaveLength(1);
      release();
      await expect(done).resolves.toBe("/run/s-1/attachments/up-1.bin /run/s-1/attachments/up-2.bin ");
      expect(mostInFlight()).toBe(1);
      expect(requests.filter((body) => body.type === "create_attachment_upload")).toEqual([
        { type: "create_attachment_upload", session: "s-1", name: "first.bin", size: 6 },
        { type: "create_attachment_upload", session: "s-1", name: "second.bin", size: 3 },
      ]);
    });

    it("answer the paths of those that arrived in the order dropped, an image and a file among them", async () => {
      const { upload } = setup();
      await expect(upload([file("a.png", "abc", "image/png"), file("b.txt", "abc")])).resolves.toBe("/run/s-1/pasted/image.png /run/s-1/attachments/up-1.bin ");
    });

    it("tell the caller of each file as it arrives, with its path and whether it went as an image, and answer all that arrived", async () => {
      const { latest } = setup();
      const seen: Array<[string, string, boolean]> = [];
      const answered = await uploadFiles([file("a.png", "abc", "image/png"), { file: new File([""], "docs"), folder: true }, file("b.txt", "abc")], "s-1", latest, {
        onUploaded: (uploaded) => seen.push([uploaded.file.file.name, uploaded.path, uploaded.image]),
      });
      expect(seen).toEqual([
        ["a.png", "/run/s-1/pasted/image.png", true],
        ["b.txt", "/run/s-1/attachments/up-1.bin", false],
      ]);
      expect(answered?.map(({ path, image }) => [path, image])).toEqual([
        ["/run/s-1/pasted/image.png", true],
        ["/run/s-1/attachments/up-1.bin", false],
      ]);
    });

    it("send an image as a file, and say so, where the session's agent takes no images", async () => {
      const { latest } = setup({ takesImages: false });
      const answered = await uploadFiles([file("a.png", "abc", "image/png")], "s-1", latest);
      expect(answered?.map(({ path, image }) => [path, image])).toEqual([["/run/s-1/attachments/up-1.bin", false]]);
    });

    it("raise one alert for a single refused file, as it is, and send the others", async () => {
      const { requests, upload } = setup();
      await expect(upload([{ file: new File([""], "docs"), folder: true }, file("ok.txt", "abc")])).resolves.toBe("/run/s-1/attachments/up-1.bin ");
      expect(requests.filter((body) => body.type === "create_attachment_upload")).toEqual([{ type: "create_attachment_upload", session: "s-1", name: "ok.txt", size: 3 }]);
      expect(alerts()).toHaveLength(1);
      expect(alerts()[0]).toMatchObject({ title: "Could not attach the file", message: `${isolate("docs")} is a folder, and only files can be attached.`, session: "s-1" });
    });

    it("raise a single alert for two or more refused files, listing each, and still send the others", async () => {
      const { requests, upload } = setup();
      await expect(upload([{ file: new File([""], "docs"), folder: true }, file("ok.txt", "abc"), file("empty.txt", "")])).resolves.toBe("/run/s-1/attachments/up-1.bin ");
      expect(requests.filter((body) => body.type === "create_attachment_upload")).toEqual([{ type: "create_attachment_upload", session: "s-1", name: "ok.txt", size: 3 }]);
      expect(alerts()).toHaveLength(1);
      const [alert] = alerts();
      expect(alert).toMatchObject({ title: "Could not attach the files", session: "s-1", error: { code: "dropped_files_refused" } });
      const { code, params } = alert.error as CodedError;
      expect(droppedFileEntries(code, params)?.map(({ name, code: reason }) => [name, reason])).toEqual([
        ["docs", "dropped_file_folder"],
        ["empty.txt", "dropped_file_empty"],
      ]);
      expect(alert.message).toContain("These files were not attached, each for its own reason:");
    });

    it("list a file the daemon failed with its code and message, among those the window refused, in the order dropped", async () => {
      const { upload } = setup({ fail: { create_attachment_upload: new CodedError("Nothing is selected to export.", "export_empty", {}) } });
      await expect(upload([file("a.bin", "abc"), { file: new File([""], "docs"), folder: true }])).resolves.toBeUndefined();
      expect(alerts()).toHaveLength(1);
      const { code, params } = alerts()[0].error as CodedError;
      expect(code).toBe("dropped_files_refused");
      expect(droppedFileEntries(code, params)).toMatchObject([
        { name: "a.bin", code: "export_empty", message: "Nothing is selected to export." },
        { name: "docs", code: "dropped_file_folder" },
      ]);
    });

    it("answer nothing, with one alert, where none arrived", async () => {
      const { upload } = setup();
      await expect(upload([{ file: new File([""], "docs"), folder: true }, file("empty.txt", "")])).resolves.toBeUndefined();
      expect(alerts()).toHaveLength(1);
      expect(alerts()[0]).toMatchObject({ title: "Could not attach the files" });
    });

    it("answer nothing for no files", async () => {
      const { requests, upload } = setup();
      await expect(upload([])).resolves.toBeUndefined();
      expect(requests).toEqual([]);
    });

    it("read the translator, request sender and limits current at the time of each file", async () => {
      const { latest, requests, setLimits, upload } = setup();
      const first = file("first.txt", "abc");
      const second = file("second.txt", "abc");
      const done = upload([first, second]);
      // The limits the daemon's next snapshot brings are what the next file is judged by.
      setLimits(null);
      await expect(done).resolves.toBeDefined();
      expect(latest.mock.calls.length).toBeGreaterThanOrEqual(2);
      expect(requests.some((body) => body.type === "create_attachment_upload" && body.name === "second.txt")).toBe(false);
      expect(alerts()).toHaveLength(1);
    });
  });

  describe("a failure of the session itself", () => {
    it.each(["session_not_running", "unknown_session"])("(%s) ends the drop: no further file is sent, the refusals so far get their alert, then the failure its own", async (failure) => {
      const { requests, upload } = setup({ fail: { create_attachment_upload: new CodedError("fallback", failure, { session: "s-1" }) } });
      const done = upload([{ file: new File([""], "docs"), folder: true }, file("a.bin", "abc"), file("b.bin", "abc")]);
      await done;
      expect(requests.filter((body) => body.type === "create_attachment_upload")).toEqual([{ type: "create_attachment_upload", session: "s-1", name: "a.bin", size: 3 }]);
      expect(alerts()).toHaveLength(2);
      expect(alerts()[0]).toMatchObject({ title: "Could not attach the file", message: `${isolate("docs")} is a folder, and only files can be attached.` });
      expect(alerts()[1]).toMatchObject({ error: { code: failure } });
    });

    it("raises a single alert when no file was refused before it", async () => {
      const { requests, upload } = setup({ fail: { create_attachment_upload: new CodedError("fallback", "session_not_running", {}) } });
      await upload([file("a.bin", "abc"), file("b.bin", "abc")]);
      expect(requests.filter((body) => body.type === "create_attachment_upload")).toHaveLength(1);
      expect(alerts()).toHaveLength(1);
      expect(alerts()[0]).toMatchObject({ error: { code: "session_not_running" }, session: "s-1" });
    });

    it("ends the drop, and discards the upload, when a part is refused for it", async () => {
      const { requests, upload } = setup({ fail: { write_attachment_part: new CodedError("fallback", "session_not_running", {}) } });
      await upload([file("a.bin", "abc"), file("b.bin", "abc")]);
      expect(requests.map((body) => body.type)).toEqual(["create_attachment_upload", "write_attachment_part", "delete_attachment_upload"]);
      expect(alerts()).toHaveLength(1);
    });

    it("leaves the files that arrived before it pasted", async () => {
      let created = 0;
      const request = looseRequestSender(async (body) => {
        if (body.type === "create_attachment_upload") {
          if (++created === 2) throw new CodedError("fallback", "session_not_running", {});
          return { type: "attachment_upload_created", id: null, upload: "u" };
        }
        if (body.type === "save_attachment") return { type: "attachment_saved", id: null, path: "/run/first.bin" };
        return { type: "ack", id: null };
      });
      const handler = dropFilesHandler(() => ({ t, request, attachmentLimits: LIMITS, takesImages: () => true }));
      await expect(handler.upload([file("first.bin", "abc"), file("second.bin", "abc")], "s-1")).resolves.toBe("/run/first.bin ");
      expect(alerts()).toHaveLength(1);
    });
  });

  describe("a drop whose destination has gone", () => {
    it("sends nothing, pastes nothing and raises no alert where it is gone before the first file", async () => {
      const { requests, upload } = setup();
      await expect(upload([file("a.bin", "abc"), { file: new File([""], "docs"), folder: true }], "s-1", () => false)).resolves.toBeUndefined();
      expect(requests).toEqual([]);
      expect(alerts()).toEqual([]);
    });

    it("stops quietly, pasting nothing, once it is gone, and does not tell the refusals so far", async () => {
      const { requests, upload } = setup();
      // The destination is gone once the first file that is sent has been saved.
      const wanted = () => !requests.some((body) => body.type === "save_attachment");
      await expect(upload([{ file: new File([""], "docs"), folder: true }, file("a.bin", "abc"), file("b.bin", "abc")], "s-1", wanted)).resolves.toBeUndefined();
      expect(requests.filter((body) => body.type === "create_attachment_upload")).toEqual([{ type: "create_attachment_upload", session: "s-1", name: "a.bin", size: 3 }]);
      expect(alerts()).toEqual([]);
    });

    it("is asked before a file is sent, never after", async () => {
      const { requests, upload } = setup();
      const sentWhenAsked: number[] = [];
      await upload([file("a.bin", "abc"), file("b.bin", "abc")], "s-1", () => {
        sentWhenAsked.push(requests.length);
        return true;
      });
      // Before the first file nothing is sent; before the second the whole first file (create, one part, save) is.
      expect(sentWhenAsked.slice(0, 2)).toEqual([0, 3]);
    });
  });

  describe("the queue of the window", () => {
    it("holds a drop made while another is uploading, on any session, until that one is done", async () => {
      let release!: () => void;
      const held = new Promise<void>((resolve) => {
        release = resolve;
      });
      const { requests, mostInFlight, upload } = setup({
        gate: async (body) => {
          if (body.type === "save_attachment" && body.upload === "up-1") await held;
        },
      });
      const first = upload([file("first.bin", "abc")], "s-1");
      const second = upload([file("second.bin", "abc")], "s-2");
      await vi.waitFor(() => expect(requests.at(-1)).toMatchObject({ type: "save_attachment", upload: "up-1" }));
      await afterQueuedWork();
      expect(requests.filter((body) => body.type === "create_attachment_upload")).toHaveLength(1);
      release();
      await expect(first).resolves.toBe("/run/s-1/attachments/up-1.bin ");
      await expect(second).resolves.toBe("/run/s-2/attachments/up-2.bin ");
      expect(mostInFlight()).toBe(1);
    });

    it("judges a drop that waited by what is true when its turn comes", async () => {
      let release!: () => void;
      const held = new Promise<void>((resolve) => {
        release = resolve;
      });
      const { requests, setLimits, upload } = setup({
        gate: async (body) => {
          if (body.type === "save_attachment") await held;
        },
      });
      const first = upload([file("first.bin", "abc")], "s-1");
      const second = upload([file("second.bin", "abc")], "s-2");
      await vi.waitFor(() => expect(requests.at(-1)).toMatchObject({ type: "save_attachment" }));
      setLimits(null);
      release();
      await first;
      await expect(second).resolves.toBeUndefined();
      expect(requests.some((body) => body.type === "create_attachment_upload" && body.name === "second.bin")).toBe(false);
    });

    it("shows the session's progress, counting all its queued files, refused ones too, until the last is done", async () => {
      let release!: () => void;
      const held = new Promise<void>((resolve) => {
        release = resolve;
      });
      const { requests, upload } = setup({
        gate: async (body) => {
          if (body.type === "create_attachment_upload" && body.name === "first.bin") await held;
        },
      });
      const first = upload([file("first.bin", "abc"), { file: new File([""], "docs"), folder: true }], "s-1");
      const second = upload([file("later.bin", "abc")], "s-1");
      await vi.waitFor(() => expect(requests).toHaveLength(1));
      expect(dropUploads.progress("s-1")).toEqual({ index: 1, total: 3, name: "first.bin" });
      expect(dropUploads.progress("s-2")).toBeUndefined();
      release();
      await Promise.all([first, second]);
      expect(dropUploads.progress("s-1")).toBeUndefined();
    });
  });

  describe("the fraction of a file uploaded", () => {
    it("is 0 once the upload is created, then what the daemon has acknowledged after each part, and is not told for the next file until it says", async () => {
      const told: Array<number | undefined> = [];
      const stop = dropUploads.subscribe(() => told.push(dropUploads.progress("s-1")?.fraction));
      const { upload } = setup();
      await upload([file("a.bin", "0123456789"), file("b.bin", "ab")]);
      stop();
      // a.bin: 10 bytes in parts of 4, then b.bin: 2 bytes in one part.
      expect(told.filter((fraction) => fraction !== undefined)).toEqual([0, 0.4, 0.8, 1, 0, 1]);
    });

    it("is what the daemon has acknowledged while a part is awaited, not what is sent", async () => {
      let release!: () => void;
      const held = new Promise<void>((resolve) => {
        release = resolve;
      });
      let writes = 0;
      const { requests, upload } = setup({
        gate: async (body) => {
          if (body.type === "write_attachment_part" && ++writes === 2) await held;
        },
      });
      const done = upload([file("a.bin", "0123456789")]);
      await vi.waitFor(() => expect(requests.filter((body) => body.type === "write_attachment_part")).toHaveLength(2));
      expect(dropUploads.progress("s-1")?.fraction).toBe(0.4);
      release();
      await done;
      expect(dropUploads.progress("s-1")).toBeUndefined();
    });

    it("is not told for an image saved as a pasted one", async () => {
      let release!: () => void;
      const held = new Promise<void>((resolve) => {
        release = resolve;
      });
      const { requests, upload } = setup({
        gate: async (body) => {
          if (body.type === "save_pasted_image") await held;
        },
      });
      const done = upload([file("shot.png", "abc", "image/png")]);
      await vi.waitFor(() => expect(requests).toHaveLength(1));
      expect(dropUploads.progress("s-1")).toMatchObject({ name: "shot.png", fraction: undefined });
      release();
      await done;
    });
  });

  describe("cancelling the uploads of the session", () => {
    /** A drop of `names` whose first part write stays held until `release`, so the test can cancel with a file under way. */
    function holdingFirstPart() {
      let release!: () => void;
      const held = new Promise<void>((resolve) => {
        release = resolve;
      });
      let holding = false;
      const setupHeld = setup({
        gate: async (body) => {
          if (body.type === "write_attachment_part" && !holding) {
            holding = true;
            await held;
          }
        },
      });
      return { ...setupHeld, release };
    }

    it("ends the file under way at its next step, discarding its upload, uploads no further file, and pastes nothing", async () => {
      const { requests, release, upload } = holdingFirstPart();
      const done = upload([file("big.bin", "0123456789"), file("next.bin", "abc")], "s-1");
      await vi.waitFor(() => expect(requests.map((body) => body.type)).toEqual(["create_attachment_upload", "write_attachment_part"]));
      dropUploads.cancel("s-1");
      release();
      await expect(done).resolves.toBeUndefined();
      expect(requests.map((body) => body.type)).toEqual(["create_attachment_upload", "write_attachment_part", "delete_attachment_upload"]);
      expect(requests.at(-1)).toEqual({ type: "delete_attachment_upload", upload: "up-1" });
      expect(alerts()).toEqual([]);
      expect(dropUploads.progress("s-1")).toBeUndefined();
    });

    it("still tells the refusals gathered before it", async () => {
      const { requests, release, upload } = holdingFirstPart();
      const done = upload([{ file: new File([""], "docs"), folder: true }, file("big.bin", "0123456789"), file("next.bin", "abc")], "s-1");
      await vi.waitFor(() => expect(requests.some((body) => body.type === "write_attachment_part")).toBe(true));
      dropUploads.cancel("s-1");
      release();
      await expect(done).resolves.toBeUndefined();
      expect(alerts()).toHaveLength(1);
      expect(alerts()[0]).toMatchObject({ title: "Could not attach the file", message: `${isolate("docs")} is a folder, and only files can be attached.` });
    });

    it("stops a drop queued behind it for the same session without uploading anything, and leaves another session's drop to go on", async () => {
      const { requests, release, upload } = holdingFirstPart();
      const first = upload([file("big.bin", "0123456789")], "s-1");
      const sameSession = upload([file("queued.bin", "abc")], "s-1");
      const otherSession = upload([file("other.bin", "abc")], "s-2");
      await vi.waitFor(() => expect(requests.some((body) => body.type === "write_attachment_part")).toBe(true));
      dropUploads.cancel("s-1");
      release();
      await expect(first).resolves.toBeUndefined();
      await expect(sameSession).resolves.toBeUndefined();
      await expect(otherSession).resolves.toBe("/run/s-2/attachments/up-2.bin ");
      expect(requests.some((body) => body.type === "create_attachment_upload" && body.name === "queued.bin")).toBe(false);
    });

    it("does not stop a drop made after Cancel while the cancelled drop is still finishing", async () => {
      const { requests, release, upload } = holdingFirstPart();
      const first = upload([file("big.bin", "0123456789")], "s-1");
      await vi.waitFor(() => expect(requests.some((body) => body.type === "write_attachment_part")).toBe(true));
      dropUploads.cancel("s-1");
      const after = upload([file("after.bin", "abc")], "s-1");
      release();
      await expect(first).resolves.toBeUndefined();
      await expect(after).resolves.toBe("/run/s-1/attachments/up-2.bin ");
    });

    it("does not stop a drop made after the uploads were all done", async () => {
      const { upload } = setup();
      await upload([file("a.bin", "abc")], "s-1");
      dropUploads.cancel("s-1");
      await expect(upload([file("b.bin", "abc")], "s-1")).resolves.toBe("/run/s-1/attachments/up-2.bin ");
    });
  });
});
