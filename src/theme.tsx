import { createContext, useContext, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";

export interface Theme {
  /** 背景遮罩浓度 0~0.95 */
  overlay: number;
  /** 背景模糊 px */
  blur: number;
  /** 图片垂直焦点 0~100% */
  focusY: number;
  /** 主题色；null = 从背景图自动取色 */
  accent: string | null;
  /** 小组件缩放 */
  widgetScale: number;
}

export const DEFAULT_THEME: Theme = { overlay: 0.55, blur: 0, focusY: 30, accent: null, widgetScale: 1 };
const FALLBACK_ACCENT = "#6d8cff";
export const SCALE_MIN = 0.75;
export const SCALE_MAX = 1.6;

interface ThemeState {
  /** 全部设置项原始值 */
  settings: Record<string, unknown>;
  theme: Theme;
  pinned: boolean;
  background: string | null;
  accent: string;
  saveTheme: (patch: Partial<Theme>) => Promise<void>;
}

const Ctx = createContext<ThemeState | null>(null);

export function useTheme() {
  const v = useContext(Ctx);
  if (!v) throw new Error("ThemeRoot missing");
  return v;
}

/** 加载主题设置与背景图，写入 CSS 变量；任一窗口修改后全部同步 */
export function ThemeRoot({ children }: { children: React.ReactNode }) {
  const [settings, setSettings] = useState<Record<string, unknown>>({});
  const [theme, setTheme] = useState<Theme>(DEFAULT_THEME);
  const [pinned, setPinned] = useState(true);
  const [background, setBackground] = useState<string | null>(null);
  const [autoAccent, setAutoAccent] = useState<string | null>(null);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    const load = async () => {
      const [s, bg] = await Promise.all([api.getSettings(), api.getBackground()]);
      setSettings(s);
      setTheme({ ...DEFAULT_THEME, ...((s.theme as Partial<Theme>) ?? {}) });
      setPinned(s.pinned !== false);
      setBackground(bg);
      setReady(true);
    };
    load();
    const unlisten = listen("settings-changed", load);
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (!background) return setAutoAccent(null);
    extractAccent(background).then(setAutoAccent, () => setAutoAccent(null));
  }, [background]);

  const accent = theme.accent ?? autoAccent ?? FALLBACK_ACCENT;

  useEffect(() => {
    const s = document.documentElement.style;
    s.setProperty("--accent", accent);
    s.setProperty("--overlay", String(theme.overlay));
    s.setProperty("--blur", `${theme.blur}px`);
    s.setProperty("--focus-y", `${theme.focusY}%`);
    s.setProperty("--bg-image", background ? `url("${background}")` : "none");
  }, [accent, theme, background]);

  const value = useMemo<ThemeState>(
    () => ({
      settings,
      theme,
      pinned,
      background,
      accent,
      saveTheme: async (patch) => {
        const next = { ...theme, ...patch };
        setTheme(next);
        await api.setSetting("theme", next);
      },
    }),
    [settings, theme, pinned, background, accent],
  );

  return ready ? <Ctx.Provider value={value}>{children}</Ctx.Provider> : null;
}

/** 背景层：图片 + 遮罩；无图片时为主题色渐变 */
export function Backdrop({ extraBlur = 0 }: { extraBlur?: number }) {
  const { background } = useTheme();
  return (
    <div className="backdrop" aria-hidden>
      {background && (
        <div className="backdrop-img" style={{ filter: `blur(calc(var(--blur) + ${extraBlur}px))` }} />
      )}
      <div className={`backdrop-shade ${background ? "" : "plain"}`} />
    </div>
  );
}

function loadImage(src: string) {
  return new Promise<HTMLImageElement>((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = reject;
    img.src = src;
  });
}

/** 读入用户选择的图片，长边压缩到 1920 并转成 JPEG data URL */
export async function prepareImage(file: File): Promise<string> {
  const url = URL.createObjectURL(file);
  try {
    const img = await loadImage(url);
    const k = Math.min(1, 1920 / Math.max(img.width, img.height));
    const c = document.createElement("canvas");
    c.width = Math.round(img.width * k);
    c.height = Math.round(img.height * k);
    c.getContext("2d")!.drawImage(img, 0, 0, c.width, c.height);
    return c.toDataURL("image/jpeg", 0.88);
  } finally {
    URL.revokeObjectURL(url);
  }
}

/** 从图片中取一个适合做强调色的颜色：按饱和度加权平均，再提亮 */
export async function extractAccent(src: string): Promise<string> {
  const img = await loadImage(src);
  const n = 32;
  const c = document.createElement("canvas");
  c.width = c.height = n;
  const ctx = c.getContext("2d")!;
  ctx.drawImage(img, 0, 0, n, n);
  const px = ctx.getImageData(0, 0, n, n).data;
  let r = 0, g = 0, b = 0, w = 0;
  for (let i = 0; i < px.length; i += 4) {
    const [h, s, l] = rgbToHsl(px[i], px[i + 1], px[i + 2]);
    void h;
    const weight = s * (1 - Math.abs(l - 0.5) * 1.6) + 0.02;
    r += px[i] * weight;
    g += px[i + 1] * weight;
    b += px[i + 2] * weight;
    w += weight;
  }
  const [h, s] = rgbToHsl(r / w, g / w, b / w);
  return hslToHex(h, Math.max(0.5, Math.min(0.85, s * 1.6)), 0.64);
}

function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  r /= 255;
  g /= 255;
  b /= 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  const h =
    max === r ? ((g - b) / d + (g < b ? 6 : 0)) / 6 : max === g ? ((b - r) / d + 2) / 6 : ((r - g) / d + 4) / 6;
  return [h, s, l];
}

function hslToHex(h: number, s: number, l: number) {
  const f = (n: number) => {
    const k = (n + h * 12) % 12;
    const a = s * Math.min(l, 1 - l);
    const c = l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(c * 255)
      .toString(16)
      .padStart(2, "0");
  };
  return `#${f(0)}${f(8)}${f(4)}`;
}
