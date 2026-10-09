#!/usr/bin/env python3
"""Draw one 1200×630 share image for each website language.

The card is the large-card size (1.91:1). Layout: the mascot on the left; on
the right the wordmark, the localized slogan, and the three homepage title
lines, with the site hostname along the bottom of that text. A right-to-left
title moves the mascot to the right and aligns the text to its inner edge. The
mascot and the wordmark are not mirrored.

The wordmark is public/wordmark.png, the same on every card. The slogan and
the title lines come from each locale catalog, and {appName} is filled from
config/app.json. The hostname comes from the origin written in site.config.ts.
The filename uses the route code (zh, not zh-Hans), which is the code in
seo/site-seo.ts. A line that does not fit, or a character the face cannot
draw, fails the run.

Arabic, Devanagari, and Thai have to be shaped. Pillow's wheels do not ship
libraqm, so this requires `brew install libraqm` and then finds it at import.
Latin, Cyrillic, and Vietnamese use Geist. Other scripts use the Noto Sans
family for that script. Japanese uses Hiragino Sans, which ships with macOS.
Fonts other than Hiragino are pinned to one google/fonts commit and cached
under ~/.cache/octoboard-og-fonts/.

Run from the repository: `python3 apps/web/scripts/generate-og-images.py`.
Needs macOS, Pillow, libraqm, and a network connection the first time a font
is missing from the cache. The PNGs are committed. The site build does not
run this script; a missing PNG fails when the Nuxt config loads.
"""

from __future__ import annotations

import json
import re
import unicodedata
import urllib.request
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, features

WEB = Path(__file__).resolve().parents[1]
REPO = WEB.parents[1]
LOGO = WEB / "public/logo.png"
WORDMARK = WEB / "public/wordmark.png"

WIDTH, HEIGHT = 1200, 630
BACKGROUND = (0x00, 0x00, 0x00)
TEXT_SECONDARY = (0xB8, 0xB8, 0xB8)
TEXT_TERTIARY = (0x77, 0x77, 0x77)
GLOW = (0xFF, 0x4B, 0x3E)

# Both sources use an integer divisor so the pixel grid stays intact.
# The mascot is 1254×1254. The wordmark is 2044×344.
ICON_SCALE = 3
WORDMARK_SCALE = 4
ICON_LEFT = 72
TEXT_GAP = 64
TEXT_RIGHT_MARGIN = 80
DOMAIN_TOP_FROM_BOTTOM = 72
TAGLINE_SIZES = (42, 28)
NAME_SLOGAN_GAP = 20
SLOGAN_TITLE_GAP = 28
SLOGAN_SIZES = (26, 20)
TEXT_BLOCK_LIFT = 12
DOMAIN_CLEARANCE = 24
HERO_LINES = ("heroTitle", "heroTitleSecond", "heroTitleAccent")

# Faces are a design choice. A new script needs a face that contains it;
# the glyph check fails until this table has one.
FONT_BY_CODE = {
    "ar": "NotoSansArabic",
    "hi": "NotoSansDevanagari",
    "ja": "HiraginoSans",
    "ko": "NotoSansKR",
    "th": "NotoSansThai",
    "zh": "NotoSansSC",
    "zh-Hant": "NotoSansTC",
}
FONT_COMMIT = "23e54b51ddffbc7713c583748e3bd86f62b1fa4a"
FONT_PATHS = {
    "Geist": "ofl/geist/Geist%5Bwght%5D.ttf",
    "NotoSansSC": "ofl/notosanssc/NotoSansSC%5Bwght%5D.ttf",
    "NotoSansTC": "ofl/notosanstc/NotoSansTC%5Bwght%5D.ttf",
    "NotoSansKR": "ofl/notosanskr/NotoSansKR%5Bwght%5D.ttf",
    "NotoSansArabic": "ofl/notosansarabic/NotoSansArabic%5Bwdth%2Cwght%5D.ttf",
    "NotoSansDevanagari": "ofl/notosansdevanagari/NotoSansDevanagari%5Bwdth%2Cwght%5D.ttf",
    "NotoSansThai": "ofl/notosansthai/NotoSansThai%5Bwdth%2Cwght%5D.ttf",
}
FONT_CACHE = Path.home() / ".cache/octoboard-og-fonts" / FONT_COMMIT
HIRAGINO_SANS = {
    500: "/System/Library/Fonts/ヒラギノ角ゴシック W5.ttc",
    700: "/System/Library/Fonts/ヒラギノ角ゴシック W7.ttc",
}


def locales() -> list[tuple[str, str]]:
    """Route code and catalog filename, in the order seo/site-seo.ts lists them."""
    text = (WEB / "seo/site-seo.ts").read_text("utf-8")
    block = re.search(r"export const locales[^=]*=\s*\[(.*?)\n\];", text, re.S)
    if not block:
        raise SystemExit("seo/site-seo.ts has no locales array")
    found = re.findall(
        r'code:\s*"([^"]+)"[\s\S]*?file:\s*"([^"]+)"',
        block.group(1),
    )
    if not found:
        raise SystemExit("seo/site-seo.ts locales have no code/file pairs")
    return found


def published_hostname() -> str:
    """Hostname of the origin literal in site.config.ts.

    An environment override is for one build. These files are the cards of
    the published site, so they follow the origin written in the file.
    """
    text = (WEB / "site.config.ts").read_text("utf-8")
    match = re.search(r'\|\|\s*"(https://[^"]+)"', text)
    if not match:
        raise SystemExit("site.config.ts has no fallback origin")
    hostname = re.sub(r"^https://", "", match.group(1)).strip("/")
    if not hostname or "/" in hostname:
        raise SystemExit(f"fallback origin {match.group(1)!r} is not a bare https host")
    return hostname


def app_name() -> str:
    name = json.loads((REPO / "config/app.json").read_text("utf-8")).get("name")
    if not isinstance(name, str) or not name.strip():
        raise SystemExit("config/app.json has no name")
    return name


def fill(template: str, name: str, code: str, key: str) -> str:
    result = template.replace("{appName}", name)
    if "{" in result:
        raise SystemExit(f"{code} {key} still has an unresolved placeholder")
    if not result.strip():
        raise SystemExit(f"{code} {key} is empty")
    return result


def catalog_lines(catalog: dict[str, str], name: str, code: str, keys: tuple[str, ...]) -> list[str]:
    lines = []
    for key in keys:
        value = catalog.get(key)
        if not isinstance(value, str):
            raise SystemExit(f"{code} is missing {key}")
        lines.append(fill(value, name, code, key))
    return lines


def is_rtl(texts: list[str]) -> bool:
    """True when the first strong character is right-to-left.

    Pass the title lines. The application name is Latin in every language,
    and checking it first would mark every card left-to-right.
    """
    for text in texts:
        for char in text:
            direction = unicodedata.bidirectional(char)
            if direction in ("R", "AL"):
                return True
            if direction == "L":
                return False
    return False


def font_file(name: str) -> Path:
    path = FONT_CACHE / f"{name}.ttf"
    if path.exists():
        return path
    FONT_CACHE.mkdir(parents=True, exist_ok=True)
    url = f"https://raw.githubusercontent.com/google/fonts/{FONT_COMMIT}/{FONT_PATHS[name]}"
    print(f"downloading {name}")
    partial = path.with_suffix(".part")
    with urllib.request.urlopen(url, timeout=180) as response:
        partial.write_bytes(response.read())
    partial.rename(path)
    return path


def apply_weight(font: ImageFont.FreeTypeFont, weight: int) -> None:
    axes = font.get_variation_axes()
    if not axes:
        return
    values = []
    for axis in axes:
        raw_name = axis["name"] if isinstance(axis, dict) else axis.name
        default = axis["default"] if isinstance(axis, dict) else axis.default
        # Pillow reports the axis name as raw bytes, including a trailing NUL.
        tag = (raw_name.decode() if isinstance(raw_name, bytes) else str(raw_name)).strip("\x00")
        values.append(float(weight) if tag in ("wght", "Weight") else float(default))
    font.set_variation_by_axes(values)


def load_font(name: str, size: int, weight: int) -> ImageFont.FreeTypeFont:
    if not features.check("raqm"):
        raise SystemExit("libraqm is missing; Arabic and Devanagari cannot be shaped (brew install libraqm)")
    if name == "HiraginoSans":
        path = HIRAGINO_SANS.get(weight)
        if path is None:
            raise SystemExit(f"no Hiragino Sans file for weight {weight}")
        if not Path(path).exists():
            raise SystemExit(f"Hiragino Sans is not at {path}")
        return ImageFont.truetype(path, size, index=0, layout_engine=ImageFont.Layout.RAQM)
    font = ImageFont.truetype(str(font_file(name)), size, layout_engine=ImageFont.Layout.RAQM)
    apply_weight(font, weight)
    return font


def fit_size(face: str, texts: list[str], max_width: int, sizes: tuple[int, int], weight: int) -> ImageFont.FreeTypeFont:
    size, floor = sizes
    while size >= floor:
        font = load_font(face, size, weight)
        if all(font.getlength(text) <= max_width for text in texts):
            return font
        size -= 2
    raise SystemExit(f"{texts!r} still wider than {max_width}px at {floor}px")


def glyph_bitmap(font: ImageFont.FreeTypeFont, text: str) -> bytes:
    canvas = Image.new("L", (font.size * 2, font.size * 2), 0)
    ImageDraw.Draw(canvas).text((0, 0), text, font=font, fill=255)
    return canvas.tobytes()


def check_glyphs(font: ImageFont.FreeTypeFont, texts: list[str], face: str) -> None:
    """Fail when a character draws as the font's .notdef glyph.

    Missing characters do not raise. They paint the same bitmap as a private-use
    code point, so that bitmap is the reference. Spaces are skipped.
    """
    missing_glyph = glyph_bitmap(font, "\ue000")
    seen: list[str] = []
    for char in "".join(texts):
        if char.isspace() or char in seen:
            continue
        if glyph_bitmap(font, char) == missing_glyph:
            seen.append(char)
    if seen:
        listed = ", ".join(f"U+{ord(char):04X} {char}" for char in seen)
        raise SystemExit(f"{face} cannot draw {listed}")


def scaled(path: Path, divisor: int) -> Image.Image:
    source = Image.open(path).convert("RGBA")
    if source.width % divisor or source.height % divisor:
        raise SystemExit(f"{path.name} is {source.size}, not divisible by {divisor}")
    size = (source.width // divisor, source.height // divisor)
    return source.resize(size, Image.NEAREST)


def mascot() -> Image.Image:
    return scaled(LOGO, ICON_SCALE)


def wordmark() -> Image.Image:
    return scaled(WORDMARK, WORDMARK_SCALE)


def glow(diameter: int) -> Image.Image:
    falloff = Image.radial_gradient("L").resize((diameter, diameter), Image.BICUBIC)
    edge = 181
    mask = falloff.point(lambda value: int(max(0, edge - value) / edge * 110))
    orb = Image.new("RGBA", (diameter, diameter), (*GLOW, 0))
    orb.putalpha(mask)
    return orb


def render(slogan: str, lines: list[str], face: str, domain: str, *, rtl: bool) -> Image.Image:
    image = Image.new("RGBA", (WIDTH, HEIGHT), (*BACKGROUND, 255))
    icon = mascot()
    icon_size = icon.width
    icon_x = WIDTH - ICON_LEFT - icon_size if rtl else ICON_LEFT
    icon_y = (HEIGHT - icon.height) // 2
    halo = glow(int(icon_size * 1.35))
    image.alpha_composite(
        halo,
        (icon_x + (icon_size - halo.width) // 2, icon_y + (icon.height - halo.height) // 2),
    )
    image.alpha_composite(icon, (icon_x, icon_y))

    if rtl:
        text_x = icon_x - TEXT_GAP
        max_width = text_x - TEXT_RIGHT_MARGIN
    else:
        text_x = icon_x + icon_size + TEXT_GAP
        max_width = WIDTH - text_x - TEXT_RIGHT_MARGIN
    anchor = "ra" if rtl else "la"
    brand = wordmark()
    if brand.width > max_width:
        raise SystemExit(f"{WORDMARK.name} is {brand.width}px wide and the column is {max_width}px")
    line_font = fit_size(face, lines, max_width, TAGLINE_SIZES, 500)
    slogan_font = fit_size(face, [slogan], max_width, SLOGAN_SIZES, 500)
    domain_font = load_font("Geist", 28, 500)
    check_glyphs(line_font, lines, face)
    check_glyphs(slogan_font, [slogan], face)
    check_glyphs(domain_font, [domain], "Geist")

    # Gaps are measured on ink, not on the pen origin. Faces sit on that
    # origin by different amounts, and a fixed origin gap changes with the face.
    slogan_top = slogan_font.getbbox(slogan)[1]
    slogan_height = slogan_font.getbbox(slogan)[3] - slogan_top
    line_top = line_font.getbbox(lines[0])[1]
    line_bottom = line_font.getbbox(lines[-1])[3]
    line_height = int(line_font.size * 1.35)
    block_height = (
        brand.height + NAME_SLOGAN_GAP + slogan_height + SLOGAN_TITLE_GAP
        + line_height * (len(lines) - 1) + (line_bottom - line_top)
    )
    top = (HEIGHT - block_height) // 2 - TEXT_BLOCK_LIFT
    domain_top = HEIGHT - DOMAIN_TOP_FROM_BOTTOM
    if top + block_height > domain_top - DOMAIN_CLEARANCE:
        raise SystemExit(f"{lines!r} reaches the hostname; shorten the title or the type")

    brand_x = text_x - brand.width if rtl else text_x
    image.alpha_composite(brand, (brand_x, top))
    draw = ImageDraw.Draw(image)
    slogan_y = top + brand.height + NAME_SLOGAN_GAP - slogan_top
    draw.text((text_x, slogan_y), slogan, font=slogan_font, fill=TEXT_TERTIARY, anchor=anchor)
    y = top + brand.height + NAME_SLOGAN_GAP + slogan_height + SLOGAN_TITLE_GAP - line_top
    for line in lines:
        draw.text((text_x, y), line, font=line_font, fill=TEXT_SECONDARY, anchor=anchor)
        y += line_height
    draw.text((text_x, domain_top), domain, font=domain_font, fill=TEXT_TERTIARY, anchor=anchor)
    return image.convert("RGB")


def main() -> None:
    name = app_name()
    domain = published_hostname()
    for code, filename in locales():
        face = FONT_BY_CODE.get(code, "Geist")
        catalog = json.loads((WEB / "i18n/locales" / filename).read_text("utf-8"))
        lines = catalog_lines(catalog, name, code, HERO_LINES)
        (slogan,) = catalog_lines(catalog, name, code, ("brandSlogan",))
        image = render(slogan, lines, face, domain, rtl=is_rtl(lines))
        destination = WEB / "public" / f"og-image-{code}.png"
        image.save(destination, "PNG", optimize=True)
        print(f"  public/og-image-{code}.png")


if __name__ == "__main__":
    main()
