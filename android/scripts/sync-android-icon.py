"""Regenerate Tauri/Android launcher icons from app-icon.png (or local logo.ico / logo.png)."""
from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
LOGO_PNG = ROOT / "logo.png"
LOGO_ICO = ROOT / "logo.ico"
APP_ICON = ROOT / "app-icon.png"
ICONS_DIR = ROOT / "src-tauri" / "icons"


def resolve_source_logo() -> Path:
    for candidate in (APP_ICON, LOGO_PNG, LOGO_ICO):
        if candidate.is_file():
            return candidate
    raise FileNotFoundError(f"Missing {APP_ICON}, {LOGO_PNG}, or {LOGO_ICO}")


def main() -> int:
    try:
        source = resolve_source_logo()
    except FileNotFoundError as err:
        print(err, file=sys.stderr)
        return 1

    if source.suffix.lower() == ".png":
        img = Image.open(source).convert("RGBA")
    else:
        img = Image.open(source).convert("RGBA")

    if img.size != (1024, 1024):
        img = img.resize((1024, 1024), Image.Resampling.LANCZOS)

    img.save(APP_ICON)
    ICONS_DIR.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, ICONS_DIR / f"icon{source.suffix.lower()}")

    cmd = ["pnpm", "tauri", "icon", str(APP_ICON), "-o", str(ICONS_DIR)]
    print("Running:", " ".join(cmd))
    completed = subprocess.run(cmd, cwd=ROOT, check=False, shell=sys.platform == "win32")
    return completed.returncode


if __name__ == "__main__":
    raise SystemExit(main())
