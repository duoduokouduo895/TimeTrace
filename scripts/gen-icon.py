"""生成应用图标源图：深蓝渐变圆角方块 + 月牙 + 三条时间条。

用法：python scripts/gen-icon.py && pnpm tauri icon scripts/app-icon.png
"""

from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

S = 2048  # 2 倍超采样，最后缩到 1024
OUT = Path(__file__).resolve().parent / "app-icon.png"


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def main() -> None:
    # 背景：纵向渐变
    bg = Image.new("RGBA", (S, S))
    top, bottom = (36, 52, 112), (12, 16, 36)
    d = ImageDraw.Draw(bg)
    for y in range(S):
        d.line([(0, y), (S, y)], fill=lerp(top, bottom, y / S) + (255,))

    # 左上角柔光
    glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse([-500, -600, 1300, 1100], fill=(110, 150, 255, 110))
    bg = Image.alpha_composite(bg, glow.filter(ImageFilter.GaussianBlur(260)))

    # 月牙：亮圆减去偏移的圆
    moon = Image.new("L", (S, S), 0)
    md = ImageDraw.Draw(moon)
    cx, cy, r = 1430, 600, 330
    md.ellipse([cx - r, cy - r, cx + r, cy + r], fill=255)
    cut = Image.new("L", (S, S), 0)
    ImageDraw.Draw(cut).ellipse([cx - r + 170, cy - r - 120, cx + r + 170, cy + r - 120], fill=255)
    moon = ImageChops.subtract(moon, cut)
    halo = Image.new("RGBA", (S, S), (255, 214, 140, 0))
    halo.putalpha(moon.filter(ImageFilter.GaussianBlur(60)).point(lambda v: v * 0.55))
    bg = Image.alpha_composite(bg, halo)
    bg.paste((255, 222, 150, 255), (0, 0), moon)

    # 三条时间条（半透明轨道需单独图层再合成，直接画会覆盖 alpha）
    x0, x1, h = 380, 1668, 128
    bars = [(1150, 0.72, (79, 140, 255)), (1390, 0.46, (34, 176, 125)), (1630, 0.88, (240, 160, 32))]
    tracks = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    td = ImageDraw.Draw(tracks)
    for y, _, _ in bars:
        td.rounded_rectangle([x0, y, x1, y + h], radius=h // 2, fill=(255, 255, 255, 38))
    bg = Image.alpha_composite(bg, tracks)
    d = ImageDraw.Draw(bg)
    for y, frac, color in bars:
        d.rounded_rectangle([x0, y, x0 + (x1 - x0) * frac, y + h], radius=h // 2, fill=color + (255,))

    # 圆角遮罩
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, S - 1, S - 1], radius=460, fill=255)
    icon = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    icon.paste(bg, (0, 0), mask)
    icon.resize((1024, 1024), Image.LANCZOS).save(OUT)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
