"""生成应用图标：深蓝渐变背景 + 月牙 + 三条时间条。

- 桌面：scripts/app-icon.png（圆角方块）→ pnpm tauri icon scripts/app-icon.png
- Android：自适应图标（背景层 + 前景层，前景缩进安全区）+ 旧版方形/圆形图标，
  直接写入 src-tauri/gen/android/app/src/main/res/

用法：python scripts/gen-icon.py
"""

from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

S = 2048  # 2 倍超采样，最后缩到目标尺寸
ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "scripts" / "app-icon.png"
ANDROID_RES = ROOT / "src-tauri" / "gen" / "android" / "app" / "src" / "main" / "res"

# Android 各密度：自适应图层 108dp，旧版图标 48dp
DENSITIES = {"mdpi": 1, "hdpi": 1.5, "xhdpi": 2, "xxhdpi": 3, "xxxhdpi": 4}
# 前景内容缩放：内容外接框对角线需落在直径 66dp 的安全圆内
FOREGROUND_SCALE = 0.64


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def background() -> Image.Image:
    """纵向渐变 + 左上角柔光"""
    bg = Image.new("RGBA", (S, S))
    top, bottom = (36, 52, 112), (12, 16, 36)
    d = ImageDraw.Draw(bg)
    for y in range(S):
        d.line([(0, y), (S, y)], fill=lerp(top, bottom, y / S) + (255,))
    glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse([-500, -600, 1300, 1100], fill=(110, 150, 255, 110))
    return Image.alpha_composite(bg, glow.filter(ImageFilter.GaussianBlur(260)))


def foreground() -> Image.Image:
    """透明底上的月牙（含光晕）与三条时间条"""
    fg = Image.new("RGBA", (S, S), (0, 0, 0, 0))

    moon = Image.new("L", (S, S), 0)
    cx, cy, r = 1430, 600, 330
    ImageDraw.Draw(moon).ellipse([cx - r, cy - r, cx + r, cy + r], fill=255)
    cut = Image.new("L", (S, S), 0)
    ImageDraw.Draw(cut).ellipse([cx - r + 170, cy - r - 120, cx + r + 170, cy + r - 120], fill=255)
    moon = ImageChops.subtract(moon, cut)
    halo = Image.new("RGBA", (S, S), (255, 214, 140, 0))
    halo.putalpha(moon.filter(ImageFilter.GaussianBlur(60)).point(lambda v: v * 0.55))
    fg = Image.alpha_composite(fg, halo)
    fg.paste((255, 222, 150, 255), (0, 0), moon)

    # 半透明轨道需单独图层再合成，直接画会覆盖 alpha
    x0, x1, h = 380, 1668, 128
    bars = [(1150, 0.72, (79, 140, 255)), (1390, 0.46, (34, 176, 125)), (1630, 0.88, (240, 160, 32))]
    tracks = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    td = ImageDraw.Draw(tracks)
    for y, _, _ in bars:
        td.rounded_rectangle([x0, y, x1, y + h], radius=h // 2, fill=(255, 255, 255, 38))
    fg = Image.alpha_composite(fg, tracks)
    d = ImageDraw.Draw(fg)
    for y, frac, color in bars:
        d.rounded_rectangle([x0, y, x0 + (x1 - x0) * frac, y + h], radius=h // 2, fill=color + (255,))
    return fg


def masked(img: Image.Image, shape: str) -> Image.Image:
    mask = Image.new("L", (S, S), 0)
    md = ImageDraw.Draw(mask)
    if shape == "round":
        md.ellipse([0, 0, S - 1, S - 1], fill=255)
    else:
        md.rounded_rectangle([0, 0, S - 1, S - 1], radius=460, fill=255)
    out = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    out.paste(img, (0, 0), mask)
    return out


def shrink(img: Image.Image, scale: float) -> Image.Image:
    """把内容按比例缩小并居中（用于自适应图标前景的安全区）"""
    side = round(S * scale)
    out = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    out.paste(img.resize((side, side), Image.LANCZOS), ((S - side) // 2, (S - side) // 2))
    return out


def main() -> None:
    bg, fg = background(), foreground()
    full = Image.alpha_composite(bg, fg)

    masked(full, "square").resize((1024, 1024), Image.LANCZOS).save(OUT)
    print(f"wrote {OUT}")

    if not ANDROID_RES.exists():
        print("skip android: run `pnpm tauri android init` first")
        return
    fg_adaptive = shrink(fg, FOREGROUND_SCALE)
    # 旧版图标（Android 7 及以下）：同样把内容收进安全区，四周留出背景
    legacy = Image.alpha_composite(bg, shrink(fg, 0.82))
    for name, k in DENSITIES.items():
        d = ANDROID_RES / f"mipmap-{name}"
        d.mkdir(parents=True, exist_ok=True)
        layer = round(108 * k)
        icon = round(48 * k)
        fg_adaptive.resize((layer, layer), Image.LANCZOS).save(d / "ic_launcher_foreground.png")
        bg.resize((layer, layer), Image.LANCZOS).save(d / "ic_launcher_background.png")
        masked(legacy, "square").resize((icon, icon), Image.LANCZOS).save(d / "ic_launcher.png")
        masked(legacy, "round").resize((icon, icon), Image.LANCZOS).save(d / "ic_launcher_round.png")
    anydpi = ANDROID_RES / "mipmap-anydpi-v26"
    anydpi.mkdir(exist_ok=True)
    xml = """<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
  <background android:drawable="@mipmap/ic_launcher_background"/>
  <foreground android:drawable="@mipmap/ic_launcher_foreground"/>
</adaptive-icon>
"""
    for name in ("ic_launcher.xml", "ic_launcher_round.xml"):
        (anydpi / name).write_text(xml, encoding="utf-8")
    print(f"wrote android icons -> {ANDROID_RES}")

    # 预览：模拟启动器的圆形 / 圆角方形裁切，方便目视检查安全区
    preview = Image.new("RGBA", (S * 2 + 200, S), (40, 40, 40, 255))
    adaptive = Image.alpha_composite(bg, fg_adaptive)
    # 启动器显示 108dp 中间的 72dp
    crop = round(S * 18 / 108)
    shown = adaptive.crop((crop, crop, S - crop, S - crop)).resize((S, S), Image.LANCZOS)
    preview.paste(masked(shown, "round"), (0, 0), masked(shown, "round"))
    preview.paste(masked(shown, "square"), (S + 200, 0), masked(shown, "square"))
    preview.resize((preview.width // 4, preview.height // 4), Image.LANCZOS).save(ROOT / "scripts" / "android-icon-preview.png")


if __name__ == "__main__":
    main()
