import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";

export type Kind = "gain" | "drain";

export interface Activity {
  id: string;
  name: string;
  color: string;
  kind: Kind;
  dailyMinutes: number;
  sort: number;
}

export interface Entry {
  id: string;
  activityId: string;
  day: string;
  startAt: number | null;
  endAt: number | null;
  seconds: number;
  source: "timer" | "manual" | "cli";
  note: string | null;
}

export interface Review {
  day: string;
  best: string;
  worst: string;
  tomorrow: string;
  score: number | null;
}

export interface DaySummary {
  day: string;
  today: string;
  dayStartHour: number;
  activities: Activity[];
  entries: Entry[];
  totals: Record<string, number>;
  yesterday: Record<string, number>;
  avg7: Record<string, number>;
  running: { activityId: string; startAt: number } | null;
  review: Review;
}

export interface DayStat {
  day: string;
  gain: number;
  drain: number;
  reviewed: boolean;
}

export interface ReminderConfig {
  enabled: boolean;
  time: string;
  sound: boolean;
}

export const DEFAULT_REMINDER: ReminderConfig = { enabled: true, time: "22:30", sound: true };

export const api = {
  daySummary: (day?: string) => invoke<DaySummary>("day_summary", { day }),
  monthOverview: (month: string) => invoke<DayStat[]>("month_overview", { month }),
  getSettings: () => invoke<Record<string, unknown>>("get_settings"),
  setSetting: (key: string, value: unknown) => invoke<void>("set_setting", { key, value }),
  setPinned: (pinned: boolean) => invoke<void>("set_pinned", { pinned }),
  getBackground: () => invoke<string | null>("get_background"),
  setBackground: (dataUrl: string) => invoke<void>("set_background", { dataUrl }),
  clearBackground: () => invoke<void>("clear_background"),
  dataDir: () => invoke<string>("data_dir"),
  openDataDir: () => invoke<void>("open_data_dir"),
  reminderPending: () => invoke<boolean>("reminder_pending"),
  snoozeReminder: (minutes: number) => invoke<void>("snooze_reminder", { minutes }),
  dismissReminder: () => invoke<void>("dismiss_reminder"),
  testReminder: () => invoke<void>("test_reminder"),
  playChime: () => invoke<void>("play_chime"),
  getAutostart: () => invoke<{ enabled: boolean; available: boolean }>("get_autostart"),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
  saveActivity: (activity: Omit<Activity, "id" | "sort"> & { id?: string }) =>
    invoke<string>("save_activity", { activity }),
  deleteActivity: (id: string) => invoke<void>("delete_activity", { id }),
  moveActivity: (id: string, dir: -1 | 1) => invoke<void>("move_activity", { id, dir }),
  startTimer: (activityId: string) => invoke<void>("start_timer", { activityId }),
  stopTimer: () => invoke<void>("stop_timer"),
  addEntry: (a: { activityId: string; seconds: number; startAt?: number; day?: string; note?: string }) =>
    invoke<string>("add_entry", a),
  deleteEntry: (id: string) => invoke<void>("delete_entry", { id }),
  saveReview: (review: Review) => invoke<void>("save_review", { review }),
  openReview: () => invoke<void>("open_review"),
};

/** 加载某天汇总；任何窗口写入后自动刷新。day 为空表示「今天」。 */
export function useDaySummary(day?: string) {
  const [data, setData] = useState<DaySummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(() => {
    api.daySummary(day).then(
      (s) => {
        setData(s);
        setError(null);
      },
      (e) => setError(String(e)),
    );
  }, [day]);

  useEffect(() => {
    reload();
    const unlisten = listen("data-changed", reload);
    // 每分钟刷新一次，处理跨过凌晨 4 点换日
    const t = setInterval(reload, 60_000);
    return () => {
      unlisten.then((f) => f());
      clearInterval(t);
    };
  }, [reload]);

  return { data, error, reload };
}

/** 每秒触发重渲染，用于计时显示 */
export function useNow(active = true) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!active) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [active]);
  return now;
}

/** 当日某项累计秒数（含正在计时的部分） */
export function secondsOf(s: DaySummary, activityId: string, now: number) {
  let sec = s.totals[activityId] ?? 0;
  if (s.running && s.running.activityId === activityId && s.day === s.today) {
    sec += Math.max(0, Math.floor((now - s.running.startAt) / 1000));
  }
  return sec;
}

export function useMonthOverview(month: string) {
  const [data, setData] = useState<DayStat[]>([]);
  useEffect(() => {
    let alive = true;
    const load = () => api.monthOverview(month).then((d) => alive && setData(d));
    load();
    const unlisten = listen("data-changed", load);
    return () => {
      alive = false;
      unlisten.then((f) => f());
    };
  }, [month]);
  return data;
}

export function fmtDur(sec: number) {
  const m = Math.floor(sec / 60);
  const h = Math.floor(m / 60);
  if (h === 0) return `${m}m`;
  return m % 60 === 0 ? `${h}h` : `${h}h${m % 60}m`;
}

export function fmtClock(sec: number) {
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  const p = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${p(m)}:${p(s)}` : `${p(m)}:${p(s)}`;
}

export function fmtTime(ms: number) {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** 解析时长："25" / "25m" / "1h20m" / "1.5h" / "1:20" → 秒 */
export function parseDuration(input: string): number | null {
  const s = input.trim().toLowerCase().replace(/\s+/g, "");
  if (!s) return null;
  let m = s.match(/^(\d+):(\d{1,2})$/);
  if (m) return (Number(m[1]) * 60 + Number(m[2])) * 60;
  m = s.match(/^(\d+(?:\.\d+)?)$/);
  if (m) return Math.round(Number(m[1]) * 60);
  m = s.match(/^(?:(\d+(?:\.\d+)?)h)?(?:(\d+)m(?:in)?)?$/);
  if (m && (m[1] || m[2])) return Math.round((Number(m[1] ?? 0) * 60 + Number(m[2] ?? 0)) * 60);
  return null;
}

/** "HH:MM" 在归属日 day 中的时间戳；早于换日时刻的算到次日凌晨 */
export function timeOnDay(day: string, hhmm: string, dayStartHour: number): number | null {
  const m = hhmm.trim().match(/^(\d{1,2})[:：](\d{2})$/);
  if (!m) return null;
  const hh = Number(m[1]);
  const mm = Number(m[2]);
  if (hh > 23 || mm > 59) return null;
  const [y, mo, d] = day.split("-").map(Number);
  return new Date(y, mo - 1, d + (hh < dayStartHour ? 1 : 0), hh, mm).getTime();
}

export function dayStartMs(day: string, dayStartHour: number) {
  const [y, mo, d] = day.split("-").map(Number);
  return new Date(y, mo - 1, d, dayStartHour).getTime();
}

export function shiftDay(day: string, delta: number) {
  const [y, mo, d] = day.split("-").map(Number);
  const t = new Date(y, mo - 1, d + delta);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${t.getFullYear()}-${p(t.getMonth() + 1)}-${p(t.getDate())}`;
}
