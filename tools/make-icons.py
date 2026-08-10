#!/usr/bin/env python3
"""Generate the application icon set from the Lucide `notebook-pen` glyph.

Run from the repository root:

    python3 tools/make-icons.py

Everything under `app/assets/icons/` is produced by this script and nothing else, so a
change to the mark means editing here and re-running rather than touching binaries by
hand. The two `.svg` files it writes are the human-editable masters; the rasters are
derived from them.

Requires `cairosvg` and `pillow` (both pip), and `iconutil` for the `.icns` (macOS only,
part of the OS). Without `iconutil` the script still writes everything else and says which
file it skipped.

## Why the tile, and not the bare glyph

Lucide strokes are drawn for 24px UI chrome. Shipped as a transparent app icon they read
as a missing asset beside filled icons in a dock or taskbar, so the glyph is set in a
rounded square.

## Why the tile is not flat

A flat `#171717` tile is nearly invisible on a dark Windows taskbar (`#202020`-ish), which
is the platform where the icon is smallest and matters most. Two fixes, both monochrome
because the product has no accent hue: a two-stop near-black gradient, and a hairline edge
in the app's own dark `--line-2`. The silhouette then reads on any ground without
inventing a brand colour.

## Three optical treatments, deliberately

`notebook-pen` carries four binding ticks down its left edge, and they are the known cost
of this glyph: each tick spans 4 units of a 24-unit grid, which is under a third of a pixel
at 16px.

- **Above 48px** — Lucide's own weight, untouched.
- **32 and 48px** — a heavier stroke, so the ticks stay separate instead of blurring into
  the notebook's edge. Ordinary icon hinting, not a different mark.
- **16px** — the ticks are dropped and the glyph enlarged. Rendering them at this size
  produces a smudge along the left edge and costs the notebook-and-pen silhouette that
  makes the icon recognisable at all. Apple and Microsoft both simplify their own marks at
  small sizes for the same reason. Verified by eye at 6× magnification, not assumed.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    import cairosvg
    from PIL import Image
except ImportError as missing:  # pragma: no cover - a setup problem, not a code path
    sys.exit(f"missing dependency: {missing.name}. Try: pip install cairosvg pillow")

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "app" / "assets" / "icons"

# Verbatim from lucide-static v1.31.0 `icons/notebook-pen.svg`, ISC. Same geometry as the
# `Icon::Brand` arm in app/src/ui/kit/icon.rs — if one changes, change both.
GLYPH = """\
    <path d="M13.4 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-7.4"/>
    <path d="M2 6h4"/>
    <path d="M2 10h4"/>
    <path d="M2 14h4"/>
    <path d="M2 18h4"/>
    <path d="M21.378 5.626a1 1 0 1 0-3.004-3.004l-5.01 5.012a2 2 0 0 0-.506.854l-.837 \
2.87a.5.5 0 0 0 .62.62l2.87-.837a2 2 0 0 0 .854-.506z"/>"""

CANVAS = 1024
#: Fraction of the canvas the tile fills. macOS expects art inset inside the 1024 grid
#: (824/1024, per Apple's icon template); Windows and Linux expect it full-bleed.
FULL_BLEED = 1.0
MACOS_INSET = 824 / 1024
#: Corner radius as a fraction of the tile's side. 0.225 is Apple's 185.4/824, and reads
#: correctly on the other two platforms as well.
RADIUS = 0.225
#: The four binding ticks, split out so the 16px variant can leave them off.
RINGS = """
    <path d="M2 6h4"/>
    <path d="M2 10h4"/>
    <path d="M2 14h4"/>
    <path d="M2 18h4"/>"""

#: How much of the tile the glyph occupies. Below ~0.55 the mark looks lost; above ~0.62 it
#: crowds the corners. The simplified variant may go larger because dropping the ticks
#: takes 2 units off the glyph's left edge and there is nothing near the corners.
GLYPH_FRACTION = 0.58
GLYPH_FRACTION_SIMPLE = 0.66

INK_TOP = "#242424"
INK_BOTTOM = "#101010"
EDGE = "#3a3a3a"  # the app's dark --line-2
GLYPH_COLOR = "#fafafa"  # the app's dark --ink

#: Lucide's own stroke, and the heavier one used at or below `HINT_BELOW` px.
STROKE = 2.0
STROKE_HINTED = 2.6
HINT_BELOW = 48
#: At or below this, the binding ticks are dropped — see the module docstring.
SIMPLIFY_AT_OR_BELOW = 16


def master(inset: float, stroke: float, simplify: bool = False) -> str:
    """One tile as SVG, at `CANVAS` square."""
    side = CANVAS * inset
    origin = (CANVAS - side) / 2
    radius = side * RADIUS

    glyph = GLYPH if not simplify else GLYPH.replace(RINGS, "")
    glyph_side = side * (GLYPH_FRACTION_SIMPLE if simplify else GLYPH_FRACTION)
    scale = glyph_side / 24
    # Centred on the canvas, not on the glyph's own bounding box: `notebook-pen` spans
    # x 2..21.4 on the 24 grid, so its visual centre is within a fifth of a unit of the
    # grid's centre and centring the viewBox is correct to well under a pixel.
    offset = (CANVAS - glyph_side) / 2

    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{CANVAS}" height="{CANVAS}" \
viewBox="0 0 {CANVAS} {CANVAS}">
  <!-- Generated by tools/make-icons.py — edit that, not this. -->
  <defs>
    <linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{INK_TOP}"/>
      <stop offset="1" stop-color="{INK_BOTTOM}"/>
    </linearGradient>
  </defs>
  <rect x="{origin:g}" y="{origin:g}" width="{side:g}" height="{side:g}" rx="{radius:g}"
        fill="url(#tile)"/>
  <!-- Hairline edge, so the tile keeps an outline against a dark taskbar. -->
  <rect x="{origin + 1:g}" y="{origin + 1:g}" width="{side - 2:g}" height="{side - 2:g}"
        rx="{radius - 1:g}" fill="none" stroke="{EDGE}" stroke-width="2" opacity="0.55"/>
  <g transform="translate({offset:g} {offset:g}) scale({scale:g})"
     fill="none" stroke="{GLYPH_COLOR}" stroke-width="{stroke:g}"
     stroke-linecap="round" stroke-linejoin="round">
{glyph}
  </g>
</svg>
"""


def render(svg: str, size: int, dest: Path) -> None:
    cairosvg.svg2png(
        bytestring=svg.encode(), write_to=str(dest), output_width=size, output_height=size
    )


def tile_for(size: int, inset: float) -> str:
    """The right optical treatment for a given output size."""
    return master(
        inset,
        STROKE_HINTED if size <= HINT_BELOW else STROKE,
        simplify=size <= SIMPLIFY_AT_OR_BELOW,
    )


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)

    # The masters, committed so the mark is editable without running anything.
    full = master(FULL_BLEED, STROKE)
    inset = master(MACOS_INSET, STROKE)
    (OUT / "logo.svg").write_text(full)
    (OUT / "logo-macos.svg").write_text(inset)
    written = ["logo.svg", "logo-macos.svg"]

    # Linux and the generic set. `dx bundle` reads sizes off the images themselves, so the
    # @2x name is a convention for humans rather than something the bundler parses.
    for size, name in [
        (32, "32x32.png"),
        (128, "128x128.png"),
        (256, "128x128@2x.png"),
        (256, "256x256.png"),
        (512, "512x512.png"),
    ]:
        render(tile_for(size, FULL_BLEED), size, OUT / name)
        written.append(name)

    # The runtime window icon, decoded by the app at startup so `dx serve` and Linux
    # window managers show the mark too — a bundle icon does neither.
    render(tile_for(64, FULL_BLEED), 64, OUT / "window-icon.png")
    written.append("window-icon.png")

    # Windows. Pillow writes every size into one container; 256 is the largest Explorer
    # uses and 16 is the title bar.
    with tempfile.TemporaryDirectory() as tmp:
        frames = []
        for size in (16, 32, 48, 64, 128, 256):
            path = Path(tmp) / f"ico-{size}.png"
            render(tile_for(size, FULL_BLEED), size, path)
            frames.append(Image.open(path).convert("RGBA"))
        # `append_images` carries the smaller frames; `sizes` would make Pillow resample
        # them itself and throw away the hinted strokes.
        frames[-1].save(
            OUT / "icon.ico", format="ICO", append_images=frames[:-1], sizes=[
                (f.width, f.height) for f in frames
            ]
        )
        written.append("icon.ico")

    # macOS. `iconutil` is the only way to write a .icns Finder and the dock both accept.
    if shutil.which("iconutil") is None:
        print("iconutil not found — skipping icon.icns (run this on macOS to produce it)")
    else:
        with tempfile.TemporaryDirectory() as tmp:
            iconset = Path(tmp) / "icon.iconset"
            iconset.mkdir()
            for size in (16, 32, 128, 256, 512):
                render(tile_for(size, MACOS_INSET), size, iconset / f"icon_{size}x{size}.png")
                render(
                    tile_for(size * 2, MACOS_INSET),
                    size * 2,
                    iconset / f"icon_{size}x{size}@2x.png",
                )
            subprocess.run(
                ["iconutil", "-c", "icns", str(iconset), "-o", str(OUT / "icon.icns")],
                check=True,
            )
        written.append("icon.icns")

    print(f"wrote {len(written)} files to {OUT.relative_to(ROOT)}:")
    for name in written:
        size = (OUT / name).stat().st_size
        print(f"  {name:<20} {size:>9,} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
