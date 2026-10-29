#!/usr/bin/env python3
"""Generate the brand assets under crates/server/static/brand/.

These are committed files rather than things rendered per request, and this
script is how they are regenerated, so none of them is an unexplained binary:

- og.png, the card a shared link shows (1200x630, the size every platform crops
  to a 1.91:1 card);
- favicon.svg, favicon-16.png, favicon-32.png and apple-touch-icon.png;
- wordmark.svg and wordmark-dark.svg, the README banner, with the name drawn as
  outlines so it looks the same without the font installed.

Everything is built from what the site already ships: the stylesheet's palette,
the one typeface (Source Sans 3, read from the static directory and set to the
weight needed), and the mark, six seats in two rows with three filled. Nothing
is fetched.

Needs `pillow` and `fonttools[woff]`:

    pip install pillow "fonttools[woff]"
    python3 scripts/make_og_image.py
"""

import io
import pathlib
import sys

from PIL import Image, ImageDraw, ImageFont
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
FONT = ROOT / "crates/server/static/fonts/SourceSans3-latin.woff2"
BRAND = ROOT / "crates/server/static/brand"

# The stylesheet's palette (crates/server/assets/tailwind.css), kept in sync by
# hand; there are few values and they change about never.
PAPER = (255, 255, 255)
SUNKEN = (242, 245, 249)
INK = (27, 31, 36)
INK_MUTED = (84, 91, 100)
HAIRLINE = (223, 227, 232)
RING = (185, 192, 200)  # the unfilled seats of the mark

# The mark: three columns, two rows, filled seats first-row-left to right.
MARK = [True, True, False, True, False, False]

HEADLINE = "An open, source-backed record of public political life."
SUBLINE = "Every fact links to the source it came from."
DOMAIN = "open-public.com"
NAME = "open-public"


def instance(weight: int) -> TTFont:
    """The variable font fixed at one weight, as plain TrueType."""
    if not FONT.exists():
        sys.exit(f"missing font: {FONT}")
    font = TTFont(str(FONT))
    if "fvar" in font:
        font = instantiateVariableFont(font, {"wght": weight})
    font.flavor = None
    return font


def pil_font(weight: int, size: int) -> ImageFont.FreeTypeFont:
    buf = io.BytesIO()
    instance(weight).save(buf)
    buf.seek(0)
    return ImageFont.truetype(buf, size)


def hex_of(rgb) -> str:
    return "#%02x%02x%02x" % rgb


def draw_mark(d: ImageDraw.ImageDraw, x: float, y: float, dot: float, gap: float, filled, empty):
    for i, on in enumerate(MARK):
        cx = x + (i % 3) * (dot + gap)
        cy = y + (i // 3) * (dot + gap)
        d.ellipse([cx, cy, cx + dot, cy + dot], fill=filled if on else empty)


def wrap(d, text, font, max_width):
    """Break text into lines that fit, on whole words."""
    words, lines, line = text.split(), [], ""
    for word in words:
        trial = f"{line} {word}".strip()
        if d.textlength(trial, font=font) <= max_width or not line:
            line = trial
        else:
            lines.append(line)
            line = word
    if line:
        lines.append(line)
    return lines


def og_image():
    width, height, margin = 1200, 630, 84
    img = Image.new("RGB", (width, height), PAPER)
    d = ImageDraw.Draw(img)
    # The pale band the pages open with, behind the claim.
    d.rectangle([0, 0, width, height - 150], fill=SUNKEN)
    d.line([(0, height - 150), (width, height - 150)], fill=HAIRLINE, width=2)

    name = pil_font(800, 40)
    head = pil_font(800, 64)
    sub = pil_font(400, 30)
    foot = pil_font(600, 26)

    dot, gap = 11, 4
    draw_mark(d, margin, margin + 2, dot, gap, INK, RING)
    d.text((margin + 3 * dot + 2 * gap + 18, margin + dot + gap / 2), NAME,
           font=name, fill=INK, anchor="lm")

    y = 210
    for line in wrap(d, HEADLINE, head, width - 2 * margin):
        d.text((margin, y), line, font=head, fill=INK)
        y += 76
    d.text((margin, y + 14), SUBLINE, font=sub, fill=INK_MUTED)

    d.text((margin, height - 75), DOMAIN, font=foot, fill=INK_MUTED, anchor="lm")
    out = BRAND / "og.png"
    img.save(out, "PNG", optimize=True)
    return out


def icon_png(size: int, name: str):
    """The mark on white, scaled to the icon size."""
    scale = 8  # draw large, then downsample, for smooth edges
    s = size * scale
    img = Image.new("RGB", (s, s), PAPER)
    d = ImageDraw.Draw(img)
    pad = s * 0.14
    gap = s * 0.06
    dot = (s - 2 * pad - 2 * gap) / 3
    top = (s - (2 * dot + gap)) / 2
    draw_mark(d, pad, top, dot, gap, INK, RING)
    img = img.resize((size, size), Image.LANCZOS)
    out = BRAND / name
    img.save(out, "PNG", optimize=True)
    return out


def favicon_svg():
    pad, gap = 4.5, 1.9
    dot = (32 - 2 * pad - 2 * gap) / 3
    top = (32 - (2 * dot + gap)) / 2
    circles = []
    for i, on in enumerate(MARK):
        cx = pad + (i % 3) * (dot + gap) + dot / 2
        cy = top + (i // 3) * (dot + gap) + dot / 2
        fill = hex_of(INK) if on else hex_of(RING)
        circles.append(f'  <circle cx="{cx:.2f}" cy="{cy:.2f}" r="{dot / 2:.2f}" fill="{fill}"/>')
    svg = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" role="img" aria-label="open-public">\n'
           f'  <rect width="32" height="32" rx="6" fill="{hex_of(PAPER)}"/>\n'
           + "\n".join(circles) + "\n</svg>\n")
    out = BRAND / "favicon.svg"
    out.write_text(svg)
    return out


def text_path(font: TTFont, text: str, size: float, x: float, baseline: float):
    """The text's outlines as one SVG path, and its advance width."""
    cmap = font.getBestCmap()
    glyphs = font.getGlyphSet()
    upm = font["head"].unitsPerEm
    k = size / upm
    pen = SVGPathPen(glyphs)
    cursor = 0.0
    for ch in text:
        gname = cmap[ord(ch)]
        # Font units are y-up; SVG is y-down, so flip around the baseline.
        tp = TransformPen(pen, (k, 0, 0, -k, x + cursor * k, baseline))
        glyphs[gname].draw(tp)
        cursor += font["hmtx"][gname][0]
    return pen.getCommands(), cursor * k


def wordmark(dark: bool):
    ink = (238, 241, 244) if dark else INK
    ring = (110, 118, 128) if dark else RING
    font = instance(800)
    size = 40
    dot, gap = 9, 3.5
    x0 = 3 * dot + 2 * gap + 14
    d, advance = text_path(font, NAME, size, x0, 38)
    width = x0 + advance + 2
    circles = []
    top = 10.0
    for i, on in enumerate(MARK):
        cx = (i % 3) * (dot + gap) + dot / 2
        cy = top + (i // 3) * (dot + gap) + dot / 2
        fill = hex_of(ink) if on else hex_of(ring)
        circles.append(f'  <circle cx="{cx:.2f}" cy="{cy:.2f}" r="{dot / 2:.2f}" fill="{fill}"/>')
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width:.1f} 50" '
           f'width="{width:.0f}" height="50" role="img" aria-label="open-public">\n'
           + "\n".join(circles) + "\n"
           f'  <path fill="{hex_of(ink)}" d="{d}"/>\n</svg>\n')
    out = BRAND / ("wordmark-dark.svg" if dark else "wordmark.svg")
    out.write_text(svg)
    return out


def main() -> int:
    BRAND.mkdir(parents=True, exist_ok=True)
    made = [
        og_image(),
        icon_png(16, "favicon-16.png"),
        icon_png(32, "favicon-32.png"),
        icon_png(180, "apple-touch-icon.png"),
        favicon_svg(),
        wordmark(dark=False),
        wordmark(dark=True),
    ]
    for p in made:
        print(f"{p.relative_to(ROOT)}  {p.stat().st_size // 1024 or 1} KB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
