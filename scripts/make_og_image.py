#!/usr/bin/env python3
"""Generate the social preview image served at /static/brand/og.png.

This is the card a link to the site shows when it is shared. It is a committed
asset rather than something rendered per request, and this script is how it is
regenerated so the file is not an unexplained binary blob.

It is deliberately built from what the site already ships: the same palette as
the stylesheet, the same two fonts as the pages, and the solid ink block that
serves as the mark. Nothing is fetched; the fonts are read from the static
directory and decompressed in memory.

Needs `pillow` and `fonttools[woff]`:

    pip install pillow "fonttools[woff]"
    python3 scripts/make_og_image.py

The size is the one every platform crops to a 1.91:1 card.
"""

import io
import pathlib
import sys

from PIL import Image, ImageDraw, ImageFont
from fontTools.ttLib import TTFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
FONTS = ROOT / "crates/server/static/fonts"
OUT = ROOT / "crates/server/static/brand/og.png"

WIDTH, HEIGHT = 1200, 630
MARGIN = 84

# The stylesheet's palette, resolved from oklch to sRGB. Kept in sync by hand;
# there are three values and they change about never.
PAPER = (242, 244, 246)
INK = (21, 24, 30)
INK_MUTED = (80, 83, 88)
HAIRLINE = (214, 217, 221)

HEADLINE = "An open, source-backed record of public political life."
SUBLINE = "Every fact links to the source it came from."
DOMAIN = "open-public.com"


def load(woff2: str, size: int) -> ImageFont.FreeTypeFont:
    """A drawable font from the woff2 the site serves, decompressed in memory."""
    path = FONTS / woff2
    if not path.exists():
        sys.exit(f"missing font: {path}")
    font = TTFont(str(path))
    buf = io.BytesIO()
    font.flavor = None  # write plain TrueType rather than woff2
    font.save(buf)
    buf.seek(0)
    return ImageFont.truetype(buf, size)


def wrap(draw, text, font, max_width):
    """Break text into lines that fit, on whole words."""
    words, lines, line = text.split(), [], ""
    for word in words:
        trial = f"{line} {word}".strip()
        if draw.textlength(trial, font=font) <= max_width or not line:
            line = trial
        else:
            lines.append(line)
            line = word
    if line:
        lines.append(line)
    return lines


def main() -> int:
    img = Image.new("RGB", (WIDTH, HEIGHT), PAPER)
    d = ImageDraw.Draw(img)

    mono = load("IBMPlexMono-SemiBold.woff2", 30)
    mono_small = load("IBMPlexMono-Medium.woff2", 24)
    sans = load("PublicSans.woff2", 62)
    sans_small = load("PublicSans.woff2", 28)

    # The mark: a solid ink block, then the wordmark in monospace beside it.
    block = 34
    d.rectangle([MARGIN, MARGIN, MARGIN + block, MARGIN + block], fill=INK)
    d.text((MARGIN + block + 18, MARGIN + block / 2), "open-public",
           font=mono, fill=INK, anchor="lm")

    # The claim, wrapped, set large. This image is shown for every page, so it
    # says what the site is rather than anything page-specific.
    y = 250
    for line in wrap(d, HEADLINE, sans, WIDTH - 2 * MARGIN):
        d.text((MARGIN, y), line, font=sans, fill=INK)
        y += 78

    d.text((MARGIN, y + 16), SUBLINE, font=sans_small, fill=INK_MUTED)

    # A hairline above the footer, the same structural rule the pages use.
    rule_y = HEIGHT - MARGIN - 52
    d.line([(MARGIN, rule_y), (WIDTH - MARGIN, rule_y)], fill=HAIRLINE, width=2)
    d.text((MARGIN, HEIGHT - MARGIN - 16), DOMAIN,
           font=mono_small, fill=INK_MUTED, anchor="ls")

    OUT.parent.mkdir(parents=True, exist_ok=True)
    img.save(OUT, "PNG", optimize=True)
    print(f"{OUT.relative_to(ROOT)}  {OUT.stat().st_size // 1024} KB  {WIDTH}x{HEIGHT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
