// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { translatorFor } from "../i18n/catalog";
import { looseRequestSender, type RequestHandler } from "../requests";
import { base64Of, imageInPaste, isSavedAsIs, MAX_PASTED_IMAGE_BYTES, pasteImageHandlers, pastedPath, savePastedImagePath } from "./pastedImage";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const t = translatorFor("en");
const MIB = 1024 * 1024;

const dataTransfer = (files: File[]) => ({ files }) as unknown as DataTransfer;

describe("the image a paste carries", () => {
  it("is the first image file of the paste", () => {
    const text = new File(["x"], "a.txt", { type: "text/plain" });
    const first = new File(["1"], "a.png", { type: "image/png" });
    const second = new File(["2"], "b.jpg", { type: "image/jpeg" });
    expect(imageInPaste(dataTransfer([text, first, second]))).toBe(first);
  });

  it.each([
    ["no clipboard data", null],
    ["no files", dataTransfer([])],
    ["only other files", dataTransfer([new File(["x"], "a.txt", { type: "text/plain" }), new File(["x"], "b")])],
  ])("is none for %s", (_name, data) => {
    expect(imageInPaste(data)).toBeUndefined();
  });

  it("takes any image type, as the paste names it", () => {
    const bmp = new File(["x"], "a.bmp", { type: "image/bmp" });
    expect(imageInPaste(dataTransfer([bmp]))).toBe(bmp);
  });
});

describe("the path as pasted", () => {
  it.each(["/home/u/.octoboard/run/s/pasted/paste-1-0.png", "C:\\Users\\u\\run\\paste-1-0.png"])("is bare when it has no whitespace (%s)", (path) => {
    expect(pastedPath(path)).toBe(path);
  });

  it.each(["/Users/some one/run/paste.png", "/tmp/a\tb.png", "/tmp/a\nb.png"])("is wrapped in single quotes when it has whitespace (%j)", (path) => {
    expect(pastedPath(path)).toBe(`'${path}'`);
  });
});

describe("a path with a quote", () => {
  it("is quoted as a POSIX shell quotes it when it also has whitespace", () => {
    expect(pastedPath("/Users/o'neil x/run/paste.png")).toBe("'/Users/o'\\''neil x/run/paste.png'");
    expect(pastedPath("/a'b'c d/p.png")).toBe("'/a'\\''b'\\''c d/p.png'");
  });

  it("stays bare when it has no whitespace", () => {
    expect(pastedPath("/Users/o'neil/run/paste.png")).toBe("/Users/o'neil/run/paste.png");
  });
});

describe("the handlers of a terminal that pastes images", () => {
  const PATH = "/data/run/s-1/pasted/paste-1-0.png";

  /** Handlers over a stand-in daemon that answers `save_pasted_image` with `path`, and the requests it received. */
  function handlers(options: { path?: string; fail?: unknown; readImage?: () => Promise<Blob | undefined> } = {}) {
    const requests: Parameters<RequestHandler>[0][] = [];
    const handler: RequestHandler = async (body) => {
      requests.push(body);
      if (options.fail) throw options.fail;
      return { type: "pasted_image_saved", id: null, path: options.path ?? PATH };
    };
    const request = looseRequestSender(handler);
    return { requests, pasting: pasteImageHandlers(() => ({ t, request, takesImages: () => true }), options.readImage) };
  }

  const base64 = (bytes: Uint8Array) => Buffer.from(bytes).toString("base64");
  const alerts = () => errorAlerts.getSnapshot();

  it("read where the platform reads an image, absent where it cannot", async () => {
    expect(handlers().pasting.read).toBeUndefined();
    const image = new Blob(["x"], { type: "image/png" });
    const readImage = vi.fn(async () => image);
    const { pasting } = handlers({ readImage });
    await expect(pasting.read!()).resolves.toBe(image);
    expect(readImage).toHaveBeenCalledOnce();
  });

  it.each(["image/png", "image/jpeg", "image/gif", "image/webp"])("save sends a %s as the bytes it is and answers the path the daemon gave", async (type) => {
    const bytes = Uint8Array.from([137, 80, 78, 71, 0, 255, 128]);
    const { requests, pasting } = handlers();
    await expect(pasting.save(new Blob([bytes], { type }), "s-1")).resolves.toBe(PATH);
    expect(requests).toEqual([{ type: "save_pasted_image", session: "s-1", data: base64(bytes) }]);
    expect(alerts()).toEqual([]);
  });

  it("save quotes a path with a space", async () => {
    const { pasting } = handlers({ path: "/Users/some one/run/s-1/pasted/paste-1-0.png" });
    await expect(pasting.save(new Blob(["x"], { type: "image/png" }), "s-1")).resolves.toBe("'/Users/some one/run/s-1/pasted/paste-1-0.png'");
  });

  it("save takes an image of exactly the limit", async () => {
    const { requests, pasting } = handlers();
    await expect(pasting.save(new Blob([new Uint8Array(MAX_PASTED_IMAGE_BYTES)], { type: "image/png" }), "s-1")).resolves.toBe(PATH);
    expect(requests).toHaveLength(1);
    expect(alerts()).toEqual([]);
  });

  it("save refuses an image over 10 MiB without sending it, saying so, and pastes nothing", async () => {
    expect(MAX_PASTED_IMAGE_BYTES).toBe(10 * MIB);
    const { requests, pasting } = handlers();
    await expect(pasting.save(new Blob([new Uint8Array(MAX_PASTED_IMAGE_BYTES + 1)], { type: "image/png" }), "s-1")).resolves.toBeUndefined();
    expect(requests).toEqual([]);
    expect(alerts()).toHaveLength(1);
    expect(alerts()[0]).toMatchObject({ title: "Could not paste the image", message: "The image is larger than 10 MiB.", session: "s-1", classification: { kind: "actionable", recovery: "none" } });
  });

  it("save pastes nothing and raises an alert when the daemon refuses the image", async () => {
    const { requests, pasting } = handlers({ fail: new Error("the image is not a PNG, JPEG, GIF or WebP file") });
    await expect(pasting.save(new Blob(["x"], { type: "image/png" }), "s-1")).resolves.toBeUndefined();
    expect(requests).toHaveLength(1);
    expect(alerts()).toHaveLength(1);
    expect(alerts()[0]).toMatchObject({ title: "Could not paste the image", session: "s-1" });
  });

  it("save answers each call through the translator and request sender current at that time", async () => {
    let current = handlers();
    const latest = vi.fn(() => ({ t, request: looseRequestSender(async (body) => { current.requests.push(body); return { type: "pasted_image_saved", id: null, path: "/x" }; }), takesImages: () => true }));
    const pasting = pasteImageHandlers(latest, undefined);
    const first = current;
    await pasting.save(new Blob(["x"], { type: "image/png" }), "s-1");
    current = handlers();
    await pasting.save(new Blob(["x"], { type: "image/png" }), "s-2");
    expect(first.requests).toMatchObject([{ session: "s-1" }]);
    expect(current.requests).toMatchObject([{ session: "s-2" }]);
    expect(latest).toHaveBeenCalledTimes(2);
  });

  describe("an image of another type", () => {
    const PNG = Uint8Array.from([137, 80, 78, 71, 1, 2, 3]);

    /** `createImageBitmap` over a bitmap of 4 by 3, and a canvas that encodes to `encoded` (null for an encoding that
     * fails): jsdom has neither. */
    function stubDecoding(encoded: Blob | null) {
      const close = vi.fn();
      const createImageBitmap = vi.fn(async () => ({ width: 4, height: 3, close }));
      vi.stubGlobal("createImageBitmap", createImageBitmap);
      const drawn: unknown[][] = [];
      const sizes: [number, number][] = [];
      vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(function (this: HTMLCanvasElement) {
        sizes.push([this.width, this.height]);
        return { drawImage: (...args: unknown[]) => drawn.push(args) } as unknown as CanvasRenderingContext2D;
      } as unknown as typeof HTMLCanvasElement.prototype.getContext);
      const toBlob = vi.spyOn(HTMLCanvasElement.prototype, "toBlob").mockImplementation((callback, type) => callback(encoded && new Blob([encoded], { type: type ?? "" })));
      return { close, createImageBitmap, drawn, sizes, toBlob };
    }

    it("is sent as a PNG drawn from it at its own size", async () => {
      const decoding = stubDecoding(new Blob([PNG]));
      const bmp = new Blob(["bmp bytes"], { type: "image/bmp" });
      const { requests, pasting } = handlers();
      await expect(pasting.save(bmp, "s-1")).resolves.toBe(PATH);
      expect(decoding.createImageBitmap).toHaveBeenCalledExactlyOnceWith(bmp);
      expect(decoding.sizes).toEqual([[4, 3]]);
      expect(decoding.drawn).toHaveLength(1);
      expect(decoding.drawn[0].slice(1)).toEqual([0, 0]);
      expect(decoding.toBlob.mock.calls[0][1]).toBe("image/png");
      expect(requests).toEqual([{ type: "save_pasted_image", session: "s-1", data: base64(PNG) }]);
      expect(decoding.close).toHaveBeenCalledOnce();
    });

    it("pastes nothing, with an alert, when it cannot be decoded", async () => {
      vi.stubGlobal("createImageBitmap", vi.fn(async () => { throw new Error("cannot decode"); }));
      const { requests, pasting } = handlers();
      await expect(pasting.save(new Blob(["x"], { type: "image/bmp" }), "s-1")).resolves.toBeUndefined();
      expect(requests).toEqual([]);
      expect(alerts()).toHaveLength(1);
      expect(alerts()[0]).toMatchObject({ title: "Could not paste the image", session: "s-1", message: "The image on the clipboard is in a format that cannot be converted.", classification: { kind: "actionable", recovery: "none" } });
    });

    it("pastes nothing, with an alert, when the PNG cannot be encoded, and still releases the bitmap", async () => {
      const decoding = stubDecoding(null);
      const { requests, pasting } = handlers();
      await expect(pasting.save(new Blob(["x"], { type: "image/bmp" }), "s-1")).resolves.toBeUndefined();
      expect(requests).toEqual([]);
      expect(alerts()).toHaveLength(1);
      expect(alerts()[0]).toMatchObject({ title: "Could not paste the image", message: "The image on the clipboard is in a format that cannot be converted.", classification: { kind: "actionable", recovery: "none" } });
      expect(decoding.close).toHaveBeenCalledOnce();
    });

    it("pastes nothing, with an alert, when there is no canvas to draw on, and still releases the bitmap", async () => {
      const decoding = stubDecoding(new Blob([PNG]));
      vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
      const { requests, pasting } = handlers();
      await expect(pasting.save(new Blob(["x"], { type: "image/bmp" }), "s-1")).resolves.toBeUndefined();
      expect(requests).toEqual([]);
      expect(alerts()).toHaveLength(1);
      expect(alerts()[0]).toMatchObject({ title: "Could not paste the image", message: "The image on the clipboard is in a format that cannot be converted.", classification: { kind: "actionable", recovery: "none" } });
      expect(decoding.close).toHaveBeenCalledOnce();
    });

    it("is refused over 10 MiB once it is a PNG, not by the size it came in", async () => {
      stubDecoding(new Blob([new Uint8Array(MAX_PASTED_IMAGE_BYTES + 1)]));
      const { requests, pasting } = handlers();
      await expect(pasting.save(new Blob(["small"], { type: "image/bmp" }), "s-1")).resolves.toBeUndefined();
      expect(requests).toEqual([]);
      expect(alerts()[0]).toMatchObject({ message: "The image is larger than 10 MiB." });
    });
  });
});

describe("whether the daemon saves an image as it is", () => {
  it.each(["image/png", "image/jpeg", "image/gif", "image/webp"])("is so for a %s", (type) => {
    expect(isSavedAsIs(new Blob(["x"], { type }))).toBe(true);
  });

  it.each(["image/bmp", "image/svg+xml", "text/plain", ""])("is not so for the type %j", (type) => {
    expect(isSavedAsIs(new Blob(["x"], { type }))).toBe(false);
  });

  it("is so up to exactly 10 MiB and not beyond", () => {
    expect(isSavedAsIs(new Blob([new Uint8Array(MAX_PASTED_IMAGE_BYTES)], { type: "image/png" }))).toBe(true);
    expect(isSavedAsIs(new Blob([new Uint8Array(MAX_PASTED_IMAGE_BYTES + 1)], { type: "image/png" }))).toBe(false);
  });
});

describe("the base64 text of a blob", () => {
  it("is that of its bytes alone, with no data URL prefix", async () => {
    await expect(base64Of(new Blob([Uint8Array.from([137, 80, 78, 71, 0, 255])], { type: "image/png" }))).resolves.toBe(Buffer.from([137, 80, 78, 71, 0, 255]).toString("base64"));
  });

  it("is empty for an empty blob", async () => {
    await expect(base64Of(new Blob([]))).resolves.toBe("");
  });

  it("rejects when the blob cannot be read", async () => {
    vi.spyOn(FileReader.prototype, "readAsDataURL").mockImplementation(function (this: FileReader) {
      this.onerror?.(new ProgressEvent("error") as ProgressEvent<FileReader>);
    });
    await expect(base64Of(new Blob(["x"]))).rejects.toBeInstanceOf(Error);
  });
});

describe("saving an image on the daemon of a session", () => {
  it("sends it as it is and answers the path, quoted where it has whitespace", async () => {
    const requests: unknown[] = [];
    const request = looseRequestSender(async (body) => {
      requests.push(body);
      return { type: "pasted_image_saved", id: null, path: "/Users/some one/pasted/p.png" };
    });
    await expect(pasteImageHandlers(() => ({ t, request, takesImages: () => true }), undefined).save(new Blob(["abc"], { type: "image/png" }), "s-9")).resolves.toBe("'/Users/some one/pasted/p.png'");
    expect(requests).toEqual([{ type: "save_pasted_image", session: "s-9", data: Buffer.from("abc").toString("base64") }]);
  });

  it("rejects, sending nothing, for an image over 10 MiB", async () => {
    const send = vi.fn();
    const request = looseRequestSender(async (body) => {
      send(body);
      return { type: "pasted_image_saved", id: null, path: "/x" };
    });
    await expect(savePastedImagePath(new Blob([new Uint8Array(MAX_PASTED_IMAGE_BYTES + 1)], { type: "image/png" }), "s-9", { t, request })).rejects.toThrow("The image is larger than 10 MiB.");
    expect(send).not.toHaveBeenCalled();
  });

  it("rejects with the daemon's failure", async () => {
    const request = looseRequestSender(async () => {
      throw new Error("refused");
    });
    await expect(savePastedImagePath(new Blob(["abc"], { type: "image/png" }), "s-9", { t, request })).rejects.toThrow("refused");
  });
});
