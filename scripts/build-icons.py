#!/usr/bin/env python3
"""Generate app icons in all formats packaging needs.

Source:
  assets/icons/source.png      master artwork (square, ideally >= 1024x1024)

Outputs (all under assets/icons/):
  png/icon_<size>.png          16, 32, 64, 128, 256, 512, 1024
  orange.png                   1024x1024 master PNG (used by Linux/.desktop/AppImage)
  orange.icns                  macOS, generated via `iconutil` (must run on macOS)
  orange.ico                   Windows, multi-resolution

Re-run after replacing source.png. The output files are checked into the repo
so packaging CI does not need Pillow installed.
"""
from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ICONS = ROOT / "assets" / "icons"
PNG_DIR = ICONS / "png"
SOURCE = ICONS / "source.png"

SIZES = [16, 32, 64, 128, 256, 512, 1024]


def load_source() -> Image.Image:
    if not SOURCE.exists():
        raise SystemExit(f"missing source artwork: {SOURCE.relative_to(ROOT)}")
    img = Image.open(SOURCE).convert("RGBA")
    if img.size[0] != img.size[1]:
        # Pad to a square so resizing keeps aspect ratio.
        side = max(img.size)
        square = Image.new("RGBA", (side, side), (0, 0, 0, 0))
        square.paste(img, ((side - img.size[0]) // 2, (side - img.size[1]) // 2))
        img = square
    return img


def render(source: Image.Image, size: int) -> Image.Image:
    return source.resize((size, size), Image.LANCZOS)


def write_pngs(source: Image.Image) -> dict[int, Path]:
    PNG_DIR.mkdir(parents=True, exist_ok=True)
    paths = {}
    for size in SIZES:
        img = render(source, size)
        path = PNG_DIR / f"icon_{size}.png"
        img.save(path, "PNG", optimize=True)
        paths[size] = path
        print(f"  wrote {path.relative_to(ROOT)}")
    # Master PNG (for Linux desktop file & AppImage)
    master = PNG_DIR / "icon_1024.png"
    shutil.copy(master, ICONS / "orange.png")
    print(f"  wrote {(ICONS / 'orange.png').relative_to(ROOT)}")
    return paths


def write_ico(paths: dict[int, Path]) -> None:
    """Pillow can save multi-resolution ICO directly."""
    base = Image.open(paths[256]).convert("RGBA")
    ico_path = ICONS / "orange.ico"
    ico_sizes = [(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    base.save(ico_path, format="ICO", sizes=ico_sizes)
    print(f"  wrote {ico_path.relative_to(ROOT)}")


def write_icns(paths: dict[int, Path]) -> None:
    """Build .iconset/ then call iconutil. macOS only."""
    if not shutil.which("iconutil"):
        print("  iconutil not found; skipping .icns (run this on macOS)")
        return

    iconset = ICONS / "orange.iconset"
    if iconset.exists():
        shutil.rmtree(iconset)
    iconset.mkdir()

    # Apple's expected iconset naming
    apple_mapping = [
        (16, "icon_16x16.png"),
        (32, "icon_16x16@2x.png"),
        (32, "icon_32x32.png"),
        (64, "icon_32x32@2x.png"),
        (128, "icon_128x128.png"),
        (256, "icon_128x128@2x.png"),
        (256, "icon_256x256.png"),
        (512, "icon_256x256@2x.png"),
        (512, "icon_512x512.png"),
        (1024, "icon_512x512@2x.png"),
    ]
    for size, name in apple_mapping:
        shutil.copy(paths[size], iconset / name)

    out = ICONS / "orange.icns"
    subprocess.run(
        ["iconutil", "-c", "icns", str(iconset), "-o", str(out)],
        check=True,
    )
    shutil.rmtree(iconset)
    print(f"  wrote {out.relative_to(ROOT)}")


def main() -> int:
    print(f"generating icons under {ICONS.relative_to(ROOT)}/")
    source = load_source()
    print(f"  source: {SOURCE.relative_to(ROOT)} ({source.size[0]}x{source.size[1]})")
    paths = write_pngs(source)
    write_ico(paths)
    write_icns(paths)
    print("done")
    return 0


if __name__ == "__main__":
    sys.exit(main())
