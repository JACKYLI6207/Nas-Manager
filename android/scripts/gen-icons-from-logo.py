from pathlib import Path
from PIL import Image

root = Path(__file__).resolve().parents[1]
for candidate in (root / "app-icon.png", root / "logo.png", root / "logo.ico"):
    if candidate.is_file():
        logo_path = candidate
        break
else:
    raise SystemExit(f"Missing app-icon.png, logo.png, or logo.ico in {root}")

logo = Image.open(logo_path).convert("RGBA")

icons_dir = root / "src-tauri" / "icons"
icons_dir.mkdir(parents=True, exist_ok=True)
for name, size in [
    ("32x32.png", 32),
    ("128x128.png", 128),
    ("128x128@2x.png", 256),
    ("icon.png", 512),
    ("Square30x30Logo.png", 30),
    ("Square44x44Logo.png", 44),
    ("Square71x71Logo.png", 71),
    ("Square89x89Logo.png", 89),
    ("Square107x107Logo.png", 107),
    ("Square142x142Logo.png", 142),
    ("Square150x150Logo.png", 150),
    ("Square284x284Logo.png", 284),
    ("Square310x310Logo.png", 310),
    ("StoreLogo.png", 50),
]:
    logo.resize((size, size), Image.Resampling.LANCZOS).save(icons_dir / name)

res = root / "src-tauri" / "gen" / "android" / "app" / "src" / "main" / "res"
for folder, size in {
    "mipmap-mdpi": 48,
    "mipmap-hdpi": 72,
    "mipmap-xhdpi": 96,
    "mipmap-xxhdpi": 144,
    "mipmap-xxxhdpi": 192,
}.items():
    d = res / folder
    d.mkdir(parents=True, exist_ok=True)
    img = logo.resize((size, size), Image.Resampling.LANCZOS)
    img.save(d / "ic_launcher.png")
    img.save(d / "ic_launcher_round.png")
    img.save(d / "ic_launcher_foreground.png")

print(f"Icons generated from {logo_path.name}")
