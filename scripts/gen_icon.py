#!/usr/bin/env python3
"""Generate Shizuka's app icons: a white kanji on a black, high-contrast
e-ink style tile, matching the app's black & white aesthetic.

Outputs (committed to the repo):
  assets/icon/Shizuka.icns                             macOS bundle icon
  assets/icon/Shizuka.ico                              Windows exe resource
  ios/Assets.xcassets/AppIcon.appiconset/icon-1024.png  iOS (square, no alpha)

Usage:
  scripts/gen_icon.py [KANJI]   # default: 学 (chosen for legibility at 16-32px;
                                # 静, the app name, turns to mush in the Dock)
"""

import pathlib
import shutil
import subprocess
import sys
import tempfile
import unicodedata

from PIL import Image, ImageDraw, ImageFont

MASTER = 1024  # largest icon size, also the drawing canvas
KANJI = sys.argv[1] if len(sys.argv) > 1 else "学"

ROOT = pathlib.Path(__file__).resolve().parent.parent
ICON_DIR = ROOT / "assets" / "icon"
IOS_SET = ROOT / "ios" / "Assets.xcassets" / "AppIcon.appiconset"

# macOS applies its own squircle to Finder/Dock icons only if we provide the
# shape: draw it (radius ~22.4% of the tile, Apple's ratio).
MAC_RADIUS = int(MASTER * 0.2237)
MAC_BORDER = 8  # white hairline so the black tile reads on dark backgrounds


def find_font() -> pathlib.Path:
    """Locates the bold Hiragino Sans (W6) used for the kanji glyph."""
    # macOS stores these file names decomposed (NFD), so match accordingly.
    pattern = unicodedata.normalize("NFD", "ヒラギノ角ゴシック W6.ttc")
    fonts = pathlib.Path("/System/Library/Fonts")
    for candidate in fonts.iterdir():
        if pattern in unicodedata.normalize("NFD", candidate.name):
            return candidate
    # Fallbacks: any Hiragino weight, then any CJK-capable TTC in the folder.
    for name in ("ヒラギノ角ゴシック", "ヒラギノ明朝"):
        pattern = unicodedata.normalize("NFD", name)
        for candidate in sorted(fonts.iterdir()):
            if pattern in unicodedata.normalize("NFD", candidate.name):
                return candidate
    raise SystemExit("No Japanese font found under /System/Library/Fonts")


FONT_PATH = find_font()


def fit_font(draw: ImageDraw.ImageDraw, box: int) -> ImageFont.FreeTypeFont:
    """Returns a font sized so the kanji fills ~72% of the tile."""
    size = int(box * 0.72)
    while size > 8:
        font = ImageFont.truetype(str(FONT_PATH), size)
        x0, y0, x1, y1 = font.getbbox(KANJI)
        if (x1 - x0) <= box * 0.78 and (y1 - y0) <= box * 0.80:
            return font
        size -= 8
    return ImageFont.truetype(str(FONT_PATH), 8)


def draw_tile(rounded: bool) -> Image.Image:
    """Draws the 1024px master tile: black background, white kanji."""
    img = Image.new("RGBA", (MASTER, MASTER), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    if rounded:
        draw.rounded_rectangle(
            [0, 0, MASTER - 1, MASTER - 1],
            radius=MAC_RADIUS,
            fill=(0, 0, 0, 255),
            outline=(255, 255, 255, 255),
            width=MAC_BORDER,
        )
    else:
        draw.rectangle([0, 0, MASTER - 1, MASTER - 1], fill=(0, 0, 0, 255))

    font = fit_font(draw, MASTER)
    x0, y0, x1, y1 = font.getbbox(KANJI)
    x = MASTER / 2 - (x0 + x1) / 2
    y = MASTER / 2 - (y0 + y1) / 2
    draw.text((x, y), KANJI, font=font, fill=(255, 255, 255, 255))
    return img


def build_icns(mac: Image.Image, dest: pathlib.Path) -> None:
    """Renders every macOS icon size into an .iconset, then packs it."""
    # iconutil requires the folder name to end in ".iconset".
    iconset = pathlib.Path(tempfile.mkdtemp(prefix="shizuka-", suffix=".iconset"))
    try:
        for name, px in (
            ("16x16", 16),
            ("32x32", 32),
            ("128x128", 128),
            ("256x256", 256),
            ("512x512", 512),
        ):
            mac.resize((px, px), Image.Resampling.LANCZOS).save(
                iconset / f"icon_{name}.png"
            )
            mac.resize((px * 2, px * 2), Image.Resampling.LANCZOS).save(
                iconset / f"icon_{name}@2x.png"
            )
        if dest.exists():
            dest.unlink()
        subprocess.run(
            ["iconutil", "-c", "icns", "-o", str(dest), str(iconset)],
            check=True,
        )
    finally:
        shutil.rmtree(iconset, ignore_errors=True)


def main() -> None:
    ICON_DIR.mkdir(parents=True, exist_ok=True)
    IOS_SET.mkdir(parents=True, exist_ok=True)

    mac = draw_tile(rounded=True)  # alpha, squircle: macOS
    square = draw_tile(rounded=False)  # full bleed: iOS + Windows

    build_icns(mac, ICON_DIR / "Shizuka.icns")
    square.save(
        ICON_DIR / "Shizuka.ico",
        format="ICO",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64),
               (128, 128), (256, 256)],
    )
    # App Store icons must be opaque RGB (no alpha channel).
    square.convert("RGB").save(IOS_SET / "icon-1024.png", optimize=True)
    (IOS_SET / "Contents.json").write_text(
        """{
  "images" : [
    {
      "filename" : "icon-1024.png",
      "idiom" : "universal",
      "platform" : "ios",
      "size" : "1024x1024"
    }
  ],
  "info" : {
    "author" : "xcode",
    "version" : 1
  }
}
"""
    )

    # Legibility previews: real 16/32px renders blown up with nearest-neighbour
    # so the pixel grid is visible when eyeballing the kanji.
    preview_dir = pathlib.Path(tempfile.gettempdir()) / "shizuka-icon-preview"
    preview_dir.mkdir(exist_ok=True)
    mac.resize((16, 16), Image.Resampling.LANCZOS).resize(
        (160, 160), Image.Resampling.NEAREST
    ).save(preview_dir / "preview-16.png")
    mac.resize((32, 32), Image.Resampling.LANCZOS).resize(
        (160, 160), Image.Resampling.NEAREST
    ).save(preview_dir / "preview-32.png")
    mac.save(preview_dir / "preview-1024.png")

    print(f"Kanji: {KANJI}   font: {FONT_PATH.name}")
    print(f"Built {ICON_DIR / 'Shizuka.icns'}")
    print(f"Built {ICON_DIR / 'Shizuka.ico'}")
    print(f"Built {IOS_SET / 'icon-1024.png'}")
    print(f"Previews: {preview_dir}")


if __name__ == "__main__":
    main()
