"""生成复盘提醒提示音：G 大调上行琶音，柔和钟琴音色 + 简单混响。

用法：python scripts/gen-chime.py   → 输出 src-tauri/sounds/review-chime.wav
"""

import wave
from pathlib import Path

import numpy as np

SR = 44100
OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "sounds" / "review-chime.wav"

# (频率 Hz, 起始秒, 力度)
NOTES = [(392.00, 0.00, 0.80), (493.88, 0.16, 0.75), (587.33, 0.32, 0.75), (783.99, 0.50, 0.90)]
# 泛音（倍数, 相对音量, 衰减倍率）——高次泛音衰减更快，听感更圆润
PARTIALS = [(1.0, 1.0, 1.0), (2.0, 0.35, 1.8), (3.0, 0.12, 2.6), (4.2, 0.05, 3.5)]
TOTAL = 3.2


def note(freq: float, vel: float) -> np.ndarray:
    t = np.arange(int(SR * 2.6)) / SR
    out = np.zeros_like(t)
    for mult, amp, decay_k in PARTIALS:
        decay = np.exp(-t * 2.2 * decay_k)
        # 轻微失谐两路叠加，增加温暖感
        for detune in (-0.8, 0.8):
            out += amp * decay * np.sin(2 * np.pi * (freq * mult + detune) * t)
    attack = np.minimum(1.0, t / 0.006)
    return out * attack * vel


def reverb(x: np.ndarray) -> np.ndarray:
    y = x.copy()
    for delay_ms, gain in [(37, 0.32), (53, 0.26), (79, 0.2), (113, 0.15), (149, 0.11), (211, 0.08)]:
        d = int(SR * delay_ms / 1000)
        y[d:] += x[:-d] * gain
    return y


def main() -> None:
    mix = np.zeros(int(SR * TOTAL))
    for freq, start, vel in NOTES:
        n = note(freq, vel)
        s = int(SR * start)
        e = min(len(mix), s + len(n))
        mix[s:e] += n[: e - s]
    mix = reverb(mix)
    # 尾部淡出，避免截断爆音
    fade = int(SR * 0.4)
    mix[-fade:] *= np.linspace(1, 0, fade)
    mix = mix / np.max(np.abs(mix)) * 0.5  # 峰值 -6 dBFS
    pcm = (mix * 32767).astype("<i2")
    OUT.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(OUT), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(pcm.tobytes())
    print(f"wrote {OUT} ({OUT.stat().st_size // 1024} KB)")


if __name__ == "__main__":
    main()
