import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import {
  Activity,
  DaySummary,
  api,
  fmtClock,
  fmtDur,
  parseDuration,
  secondsOf,
  timeOnDay,
  useDaySummary,
  useNow,
} from "./api";
import { Icon } from "./icons";
import { Backdrop, SCALE_MAX, SCALE_MIN, useTheme } from "./theme";

const BASE_WIDTH = 320;
const WEEKDAY = "日一二三四五六";
const win = getCurrentWindow();

export default function Widget() {
  const { data, error } = useDaySummary();
  const { theme, pinned, saveTheme } = useTheme();
  const now = useNow(!!data?.running);
  const [collapsed, setCollapsed] = useState(false);
  const [adding, setAdding] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [scale, setScale] = useState(theme.widgetScale);
  const [remind, setRemind] = useState(false);
  const outerRef = useRef<HTMLDivElement>(null);

  useEffect(() => setScale(theme.widgetScale), [theme.widgetScale]);

  // 复盘提醒：启动时查一次，之后听后端事件
  useEffect(() => {
    api.reminderPending().then(setRemind);
    const unlisten = listen<boolean>("reminder", (e) => setRemind(e.payload));
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  // 窗口尺寸跟随内容（含缩放）
  useLayoutEffect(() => {
    const el = outerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const r = el.getBoundingClientRect();
      win.setSize(new LogicalSize(Math.ceil(r.width), Math.ceil(r.height)));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Ctrl + 滚轮缩放，停止滚动 400ms 后保存
  useEffect(() => {
    const el = outerRef.current;
    if (!el) return;
    let timer: number | undefined;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      setScale((s) => {
        const next = Math.round(Math.min(SCALE_MAX, Math.max(SCALE_MIN, s + (e.deltaY < 0 ? 0.05 : -0.05))) * 100) / 100;
        clearTimeout(timer);
        timer = window.setTimeout(() => saveTheme({ widgetScale: next }), 400);
        return next;
      });
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [saveTheme]);

  useEffect(() => {
    if (!msg) return;
    const t = setTimeout(() => setMsg(null), 3000);
    return () => clearTimeout(t);
  }, [msg]);

  const fail = (e: unknown) => setMsg(String(e));

  const running = data?.running ?? null;
  const runningAct = data?.activities.find((a) => a.id === running?.activityId);
  const sumOf = (kind: string) =>
    data ? data.activities.filter((a) => a.kind === kind).reduce((s, a) => s + secondsOf(data, a.id, now), 0) : 0;
  const dateLabel = data
    ? (() => {
        const [y, m, d] = data.today.split("-").map(Number);
        return `${m}月${d}日 周${WEEKDAY[new Date(y, m - 1, d).getDay()]}`;
      })()
    : "";

  return (
    <div className="w-outer" ref={outerRef}>
      <div className="widget" style={{ zoom: scale, width: BASE_WIDTH }}>
        <Backdrop />
        <div className="w-content">
          <header className="w-head" data-tauri-drag-region>
            <div className="w-title" data-tauri-drag-region>
              <span className="w-date" data-tauri-drag-region>
                {dateLabel}
              </span>
              {runningAct && running && (
                <span className="w-live" data-tauri-drag-region>
                  <i className="pulse" style={{ background: runningAct.color }} />
                  {runningAct.name}
                  <b>{fmtClock(Math.floor((now - running.startAt) / 1000))}</b>
                </span>
              )}
            </div>
            <div className="w-tools">
              <button className="icon" title="该不该做？让 Jev 决定" onClick={() => api.openDecide().catch(fail)}>
                <Icon name="bolt" />
              </button>
              <button className="icon" title="晚间复盘" onClick={() => api.openReview().catch(fail)}>
                <Icon name="moon" />
              </button>
              <button
                className={`icon ${pinned ? "on" : ""}`}
                title={pinned ? "已置顶（点击取消）" : "未置顶（点击置顶）"}
                onClick={() => api.setPinned(!pinned).catch(fail)}
              >
                <Icon name="pin" />
              </button>
              <button className="icon" title={collapsed ? "展开" : "折叠"} onClick={() => setCollapsed(!collapsed)}>
                <Icon name={collapsed ? "chevronDown" : "chevronUp"} />
              </button>
              <button className="icon" title="最小化" onClick={() => win.minimize()}>
                <Icon name="minus" />
              </button>
              <button className="icon danger" title="退出" onClick={() => win.close()}>
                <Icon name="close" />
              </button>
            </div>
          </header>

          {remind && (
            <div className="w-remind">
              <span className="w-remind-icon">
                <Icon name="moon" size={16} />
              </span>
              <span className="w-remind-text">
                <b>该复盘啦</b>
                <span>花一分钟回顾今天</span>
              </span>
              <button
                className="primary small"
                onClick={() => {
                  api.openReview().catch(fail);
                  api.dismissReminder();
                }}
              >
                去复盘
              </button>
              <button className="icon" title="10 分钟后再提醒" onClick={() => api.snoozeReminder(10)}>
                <span className="snooze">10′</span>
              </button>
              <button className="icon" title="今天不再提醒" onClick={() => api.dismissReminder()}>
                <Icon name="close" size={12} />
              </button>
            </div>
          )}

          {(error || msg) && <div className="w-msg">{error ?? msg}</div>}

          {data && !collapsed && (
            <ul className="w-list">
              {data.activities.map((a) => (
                <Row
                  key={a.id}
                  a={a}
                  data={data}
                  now={now}
                  adding={adding === a.id}
                  onToggleAdd={() => setAdding(adding === a.id ? null : a.id)}
                  onError={fail}
                />
              ))}
              {data.activities.length === 0 && <li className="w-empty">还没有项目，去复盘页 → 项目 里添加</li>}
            </ul>
          )}

          {data && (
            <footer className="w-foot" data-tauri-drag-region>
              <span data-tauri-drag-region>
                有益 <b className="gain">{fmtDur(sumOf("gain"))}</b>
              </span>
              <span data-tauri-drag-region>
                消耗 <b className="drain">{fmtDur(sumOf("drain"))}</b>
              </span>
              {collapsed && running && (
                <button className="icon" title="停止计时" onClick={() => api.stopTimer().catch(fail)}>
                  <Icon name="stop" size={12} />
                </button>
              )}
            </footer>
          )}
        </div>
      </div>
    </div>
  );
}

function Row(props: {
  a: Activity;
  data: DaySummary;
  now: number;
  adding: boolean;
  onToggleAdd: () => void;
  onError: (e: unknown) => void;
}) {
  const { a, data, now } = props;
  const sec = secondsOf(data, a.id, now);
  const isRunning = data.running?.activityId === a.id;
  const over = a.kind === "drain" && a.dailyMinutes > 0 && sec > a.dailyMinutes * 60;

  return (
    <li className={`w-row ${isRunning ? "running" : ""} ${over ? "over" : ""}`}>
      <div className="w-line">
        <span className="w-name" title={a.name}>
          {a.name}
        </span>
        <span className="w-val">
          <b>{fmtDur(sec)}</b>
          <TargetEditor a={a} onError={props.onError} />
        </span>
        <span className="w-actions">
          <button
            className={`icon play ${isRunning ? "on" : ""}`}
            title={isRunning ? "停止" : "开始计时"}
            onClick={() => (isRunning ? api.stopTimer() : api.startTimer(a.id)).catch(props.onError)}
          >
            <Icon name={isRunning ? "stop" : "play"} size={12} />
          </button>
          <button className={`icon ${props.adding ? "on" : ""}`} title="手动录入时长" onClick={props.onToggleAdd}>
            <Icon name="plus" size={12} />
          </button>
        </span>
      </div>
      <Bar a={a} seconds={sec} running={isRunning} />
      {props.adding && <QuickAdd a={a} data={data} onDone={props.onToggleAdd} onError={props.onError} />}
    </li>
  );
}

/** 点击「/1h」直接修改每日目标或上限（分钟） */
function TargetEditor({ a, onError }: { a: Activity; onError: (e: unknown) => void }) {
  const [editing, setEditing] = useState(false);
  const [val, setVal] = useState("");
  const label = a.kind === "gain" ? "目标" : "上限";

  if (!editing) {
    return (
      <button
        className="w-target"
        title={`点击修改每日${label}`}
        onClick={() => {
          setVal(String(a.dailyMinutes || ""));
          setEditing(true);
        }}
      >
        {a.dailyMinutes > 0 ? `/${fmtDur(a.dailyMinutes * 60)}` : `设${label}`}
      </button>
    );
  }

  const save = () => {
    const n = Math.round(Number(val || 0));
    setEditing(false);
    if (!Number.isFinite(n) || n < 0 || n === a.dailyMinutes) return;
    api.saveActivity({ id: a.id, name: a.name, color: a.color, kind: a.kind, dailyMinutes: n }).catch(onError);
  };

  return (
    <span className="w-target-edit">
      /
      <input
        autoFocus
        inputMode="numeric"
        value={val}
        onChange={(e) => setVal(e.target.value.replace(/[^\d]/g, ""))}
        onBlur={save}
        onKeyDown={(e) => {
          if (e.key === "Enter") save();
          if (e.key === "Escape") setEditing(false);
        }}
      />
      分
    </span>
  );
}

export function Bar({ a, seconds, running }: { a: Activity; seconds: number; running?: boolean }) {
  const limit = a.dailyMinutes * 60;
  // 没设目标时按 2 小时为满格
  const scale = Math.max(limit || 7200, seconds);
  const main = Math.min(seconds, limit || seconds);
  const over = limit > 0 ? Math.max(0, seconds - limit) : 0;
  return (
    <span className={`bar ${running ? "running" : ""}`} style={{ ["--c" as string]: a.color }}>
      <span className="bar-fill" style={{ width: `${(main / scale) * 100}%` }} />
      {over > 0 && (
        <span className={`bar-over ${a.kind === "drain" ? "bad" : "good"}`} style={{ width: `${(over / scale) * 100}%` }} />
      )}
      {limit > 0 && seconds > limit && <span className="bar-mark" style={{ left: `${(limit / scale) * 100}%` }} />}
    </span>
  );
}

function QuickAdd(props: { a: Activity; data: DaySummary; onDone: () => void; onError: (e: unknown) => void }) {
  const [dur, setDur] = useState("");
  const [start, setStart] = useState("");
  const [err, setErr] = useState<string | null>(null);

  const submit = () => {
    const seconds = parseDuration(dur);
    if (!seconds) return setErr("时长格式：25 / 25m / 1h20m / 1:20");
    let startAt: number | undefined;
    if (start.trim()) {
      const t = timeOnDay(props.data.today, start, props.data.dayStartHour);
      if (t === null) return setErr("开始时间格式：HH:MM");
      startAt = t;
    }
    api.addEntry({ activityId: props.a.id, seconds, startAt }).then(props.onDone, props.onError);
  };

  return (
    <form
      className="w-add"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <input autoFocus placeholder="时长 25m" value={dur} onChange={(e) => setDur(e.target.value)} />
      <input placeholder="开始 HH:MM 可空" value={start} onChange={(e) => setStart(e.target.value)} />
      <button type="submit" className="primary">
        记录
      </button>
      {err && <div className="w-add-err">{err}</div>}
    </form>
  );
}
