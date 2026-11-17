// A retina-sized screenshot-like PNG for the viewer's zoom scenarios, drawn and encoded here when the scenario opens
// rather than kept as a blob in the source. It is made of flat areas, so it stays a few dozen kilobytes, and of the
// structures zooming is meant to be judged on: a labelled grid whose numbers are the pixel coordinates of each cell
// (to tell where a zoom landed), text-like bars, thin lines and a one-pixel checkerboard in every cell, whose squares
// turn into crisp blocks above 100%. Its `after` version is the same picture with a few cells changed, for the two
// sides of an image change, and its `cropped` version is that picture cut to a smaller canvas, for a change between
// two images of different sizes. Its `phone` version is an unrelated portrait picture, for a change between two images that
// share neither size nor shape.

export type ScreenshotVersion = "before" | "after" | "cropped" | "phone";

/** What each version is: the picture, whether its changed cells show, and the canvas's size (the picture is drawn from the top left). */
const VERSIONS: Record<ScreenshotVersion, { picture: "screen" | "phone"; after: boolean; width: number; height: number }> = {
  before: { picture: "screen", after: false, width: 2400, height: 1500 },
  after: { picture: "screen", after: true, width: 2400, height: 1500 },
  cropped: { picture: "screen", after: true, width: 1600, height: 1000 },
  phone: { picture: "phone", after: false, width: 900, height: 1600 },
};

const TITLE_BAR = 60;
const SIDEBAR = 360;
const CELL = { width: 240, height: 180 };
/** The cells, as [row, column], that the `after` picture shows highlighted with a red chip. */
const CHANGED_CELLS = [[0, 1], [1, 3], [2, 2], [4, 5], [6, 0]];

// Palette indices.
const BACKGROUND = 0;
const PANEL = 1;
const LINE = 2;
const INK = 3;
const MUTED = 4;
const WHITE = 5;
const BLACK = 6;
const TITLE = 7;
const CHIPS = [8, 9, 10, 11] as const;
const TRAFFIC = [11, 10, 9] as const;
const SELECTION = 12;
/** The phone picture's header, from top to bottom. */
const HEADER = [13, 14, 15, 16, 17, 18, 19, 20] as const;

const PALETTE = [
  [0xf4, 0xf5, 0xf7], [0xe6, 0xe8, 0xec], [0xc9, 0xcd, 0xd4], [0x2b, 0x2f, 0x36], [0x9a, 0xa1, 0xad], [0xff, 0xff, 0xff], [0x00, 0x00, 0x00],
  [0x1f, 0x23, 0x29], [0x2f, 0x6f, 0xed], [0x2f, 0xa4, 0x6a], [0xe8, 0x9a, 0x2b], [0xd6, 0x45, 0x45], [0xd5, 0xe3, 0xfc],
  [0x3b, 0x1f, 0x6e], [0x55, 0x24, 0x7a], [0x76, 0x2a, 0x82], [0x99, 0x30, 0x84], [0xbb, 0x3b, 0x80], [0xd9, 0x4f, 0x76], [0xee, 0x6b, 0x68], [0xf8, 0x8d, 0x5a],
] as const;

// Digits and the comma as 3 x 5 bitmaps, row by row.
const GLYPHS: Record<string, string> = {
  "0": "111101101101111", "1": "010110010010111", "2": "111001111100111", "3": "111001111001111", "4": "101101111001001",
  "5": "111100111001111", "6": "111100111101111", "7": "111001001001001", "8": "111101111101111", "9": "111101111001111",
  ",": "000000000010100",
};

/** A repeatable pseudo-random number from `seed`, so that the picture is the same every time. */
const hash = (seed: number) => {
  let x = Math.imul(seed ^ 0x9e3779b9, 0x85ebca6b);
  x ^= x >>> 13;
  x = Math.imul(x, 0xc2b2ae35);
  return (x ^ (x >>> 16)) >>> 0;
};

/** A canvas of palette indices, one filter byte (none) in front of every row, as PNG scanlines have it. */
function canvas(width: number, height: number) {
  const raw = new Uint8Array((width + 1) * height);
  const stride = width + 1;
  const set = (x: number, y: number, colour: number) => {
    if (x >= 0 && x < width && y >= 0 && y < height) raw[y * stride + 1 + x] = colour;
  };
  const rect = (x: number, y: number, w: number, h: number, colour: number) => {
    for (let row = y; row < y + h; row++) for (let column = x; column < x + w; column++) set(column, row, colour);
  };
  const disc = (cx: number, cy: number, radius: number, colour: number) => {
    for (let y = -radius; y <= radius; y++) for (let x = -radius; x <= radius; x++) if (x * x + y * y <= radius * radius) set(cx + x, cy + y, colour);
  };
  const text = (value: string, x: number, y: number, scale: number, colour: number) => {
    [...value].forEach((character, index) => {
      const glyph = GLYPHS[character];
      for (let i = 0; i < 15; i++) if (glyph[i] === "1") rect(x + (index * 4 + (i % 3)) * scale, y + Math.floor(i / 3) * scale, scale, scale, colour);
    });
  };
  const bars = (x: number, y: number, count: number, step: number, thickness: number, longest: number, seed: number) => {
    for (let i = 0; i < count; i++) rect(x, y + i * step, 80 + (hash(seed + i) % (longest - 80)), thickness, MUTED);
  };
  return { raw, set, rect, disc, text, bars };
}

/** A phone screen: a stepped gradient header with an avatar, then cards of text-like bars and a bottom bar of buttons. */
function drawPhone(width: number, height: number): Uint8Array {
  const { raw, rect, disc, text, bars } = canvas(width, height);
  rect(0, 0, width, height, PANEL);
  HEADER.forEach((colour, index) => rect(0, index * 50, width, 50, colour));
  rect(40, 24, 120, 14, WHITE);
  rect(width - 100, 24, 60, 14, WHITE);
  disc(width / 2, 300, 90, WHITE);
  disc(width / 2, 300, 70, HEADER[3]);
  rect(width / 2 - 160, 430, 320, 22, WHITE);
  rect(width / 2 - 100, 470, 200, 12, HEADER[7]);
  for (let card = 0; card < 4; card++) {
    const y = 560 + card * 250;
    rect(40, y, width - 80, 220, WHITE);
    rect(40, y, 10, 220, HEADER[card + 2]);
    disc(110, y + 70, 40, HEADER[card + 2]);
    text(`0,${y}`, 180, y + 40, 4, INK);
    bars(180, y + 100, 3, 24, 10, width - 260, card * 7);
    // Vertical hairlines, one pixel wide and one apart.
    for (let x = 70; x < width - 70; x += 2) rect(x, y + 190, 1, 18, INK);
  }
  rect(0, height - 120, width, 120, TITLE);
  for (let i = 0; i < 4; i++) disc(150 + i * 200, height - 60, 28, i === 0 ? CHIPS[0] : MUTED);
  return raw;
}

function draw({ picture, after, width, height }: { picture: "screen" | "phone"; after: boolean; width: number; height: number }): Uint8Array {
  if (picture === "phone") return drawPhone(width, height);
  const { raw, set, rect, disc, text, bars } = canvas(width, height);
  rect(0, 0, width, height, BACKGROUND);
  rect(0, 0, width, TITLE_BAR, TITLE);
  TRAFFIC.forEach((colour, index) => disc(40 + index * 40, TITLE_BAR / 2, 10, colour));
  rect(SIDEBAR, TITLE_BAR - 1, width - SIDEBAR, 1, LINE);

  rect(0, TITLE_BAR, SIDEBAR, height - TITLE_BAR, PANEL);
  rect(0, TITLE_BAR + 100, SIDEBAR, 44, SELECTION);
  bars(40, TITLE_BAR + 40, 1, 0, 14, SIDEBAR - 80, 1);
  bars(40, TITLE_BAR + 112, 30, 44, 14, SIDEBAR - 80, 100);
  rect(SIDEBAR - 1, TITLE_BAR, 1, height - TITLE_BAR, LINE);

  for (let row = 0; row * CELL.height < height - TITLE_BAR; row++) {
    for (let column = 0; SIDEBAR + column * CELL.width < width; column++) {
      const x = SIDEBAR + column * CELL.width;
      const y = TITLE_BAR + row * CELL.height;
      if ((row + column) % 2 === 0) rect(x, y, CELL.width, CELL.height, WHITE);
      rect(x, y, CELL.width, 1, LINE);
      rect(x, y, 1, CELL.height, LINE);
      if (after && CHANGED_CELLS.some(([r, c]) => r === row && c === column)) {
        rect(x + 1, y + 1, CELL.width - 1, CELL.height - 1, SELECTION);
        rect(x + CELL.width - 56, y + 16, 40, 8, CHIPS[3]);
      }
      rect(x + 16, y + 16, 28, 8, CHIPS[(row + column) % CHIPS.length]);
      text(`${x},${y}`, x + 16, y + 40, 3, INK);
      bars(x + 16, y + 76, 3, 14, 6, CELL.width - 40, row * 31 + column);
      // The one-pixel checkerboard: the finest detail the picture has.
      for (let j = 0; j < 24; j++) for (let i = 0; i < 24; i++) set(x + CELL.width - 40 + i, y + CELL.height - 40 + j, (i + j) % 2 === 0 ? BLACK : WHITE);
    }
  }
  return raw;
}

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (bytes: Uint8Array) => {
  let c = 0xffffffff;
  for (const byte of bytes) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};

function chunk(type: string, data: Uint8Array): Uint8Array {
  const out = new Uint8Array(12 + data.length);
  const view = new DataView(out.buffer);
  view.setUint32(0, data.length);
  for (let i = 0; i < 4; i++) out[4 + i] = type.charCodeAt(i);
  out.set(data, 8);
  view.setUint32(8 + data.length, crc32(out.subarray(4, 8 + data.length)));
  return out;
}

async function deflate(bytes: Uint8Array): Promise<Uint8Array> {
  const stream = new Response(bytes as BodyInit).body!.pipeThrough(new CompressionStream("deflate"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

const encoded = new Map<ScreenshotVersion, Promise<{ mediaType: string; data: string }>>();

/** The screenshot as the daemon sends an image: its media type and its bytes in base64. Each version is drawn once. */
export function screenshotImage(version: ScreenshotVersion = "before"): Promise<{ mediaType: string; data: string }> {
  if (!encoded.has(version)) encoded.set(version, (async () => {
    const header = new Uint8Array(13);
    const view = new DataView(header.buffer);
    const { width, height } = VERSIONS[version];
    view.setUint32(0, width);
    view.setUint32(4, height);
    header.set([8, 3, 0, 0, 0], 8); // 8 bits, palette, deflate, adaptive filtering, no interlace
    const parts = [
      new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
      chunk("IHDR", header),
      chunk("PLTE", Uint8Array.from(PALETTE.flat())),
      chunk("IDAT", await deflate(draw(VERSIONS[version]))),
      chunk("IEND", new Uint8Array(0)),
    ];
    const png = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
    let offset = 0;
    for (const part of parts) {
      png.set(part, offset);
      offset += part.length;
    }
    let binary = "";
    for (let i = 0; i < png.length; i += 0x8000) binary += String.fromCharCode(...png.subarray(i, i + 0x8000));
    return { mediaType: "image/png", data: btoa(binary) };
  })());
  return encoded.get(version)!;
}
