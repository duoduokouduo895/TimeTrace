import { useEffect, useRef, useState } from "react";
import {
  Activity,
  DaySummary,
  Kind,
  Review as ReviewData,
  api,
  dayStartMs,
  fmtDur,
  fmtTime,
  parseDuration,
  secondsOf,
  shiftDay,
  timeOnDay,
  useDaySummary,
  useMonthOverview,
  useNow,
} from "./api";
import { Icon } from "./icons";
import { Backdrop, SCALE_MAX, SCALE_MIN, prepareImage, useTheme } from "./theme";
import { Bar } from "./Widget";

const SOURCE_LABEL = { timer: "计时", manual: "手动", cli: "Codex" } as const;
const WEEKDAY = "日一二三四五六";

type Tab = "review" | "activities" | "appearance";

export default function Review() {
  const [day, setDay] = useState<string | undefined>(undefined);
  const { data, error } = useDaySummary(day);
  const now = useNow(!!data?.running);
  const [tab, setTab] = useState<Tab>("review");

  return (
    <>
      <Backdrop extraBlur={14} />
      <div className="r-page">
        {!data ? (
          <div className="muted">{error ?? "加载中…"}</div>
        ) : (
          <>
            <header className="r-head">
              <DateNav data={data} onPick={(d) => setDay(d === data.today ? undefined : d)} />
              <nav className="seg">
                {(
                  [
                    ["review", "复盘"],
                    ["activities", "项目"],
                    ["appearance", "外观"],
                  ] as const
                ).map(([k, label]) => (
                  <button key={k} className={tab === k ? "on" : ""} onClick={() => setTab(k)}>
                    {label}
                  </button>
                ))}
              </nav>
            </header>
            {error && <div className="r-err">{error}</div>}

            {tab === "review" && (
              <>
                <Stats data={data} now={now} />
                <Card title="时间轴">
                  <Timeline data={data} now={now} />
                </Card>
                <Card title="各项用时">
                  <Compare data={data} now={now} />
                </Card>
                <Card title="记录明细">
                  <Entries data={data} />
                </Card>
                <Card title="一分钟复盘">
                  <ReviewForm data={data} />
                </Card>
              </>
            )}
            {tab === "activities" && (
              <Card title="项目" hint="有益类设「每日目标」，消耗类设「每日上限」，0 表示不设。小组件上点「/1h」也能直接改。">
                <Settings activities={data.activities} />
              </Card>
            )}
            {tab === "appearance" && <Appearance />}
          </>
        )}
      </div>
    </>
  );
}

function Card(props: { title: string; hint?: string; children: React.ReactNode }) {
  return (
    <section className="card">
      <h2>
        {props.title}
        {props.hint && <span className="hint">{props.hint}</span>}
      </h2>
      {props.children}
    </section>
  );
}

// ---------- 日期导航 + 小日历 ----------

function DateNav({ data, onPick }: { data: DaySummary; onPick: (day: string) => void }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const isToday = data.day === data.today;
  const [y, m, d] = data.day.split("-").map(Number);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);

  return (
    <div className="date-nav" ref={ref}>
      <button className="icon-btn" title="前一天" onClick={() => onPick(shiftDay(data.day, -1))}>
        <Icon name="chevronLeft" />
      </button>
      <button className={`date-btn ${open ? "open" : ""}`} onClick={() => setOpen(!open)}>
        <Icon name="calendar" />
        <span className="date-main">
          {m}月{d}日
        </span>
        <span className="date-sub">
          {y} · 周{WEEKDAY[new Date(y, m - 1, d).getDay()]}
        </span>
        {isToday && <span className="tag">今天</span>}
      </button>
      <button className="icon-btn" title="后一天" disabled={isToday} onClick={() => onPick(shiftDay(data.day, 1))}>
        <Icon name="chevronRight" />
      </button>
      {!isToday && (
        <button className="ghost" onClick={() => onPick(data.today)}>
          回到今天
        </button>
      )}
      {open && (
        <Calendar
          selected={data.day}
          today={data.today}
          onPick={(day) => {
            onPick(day);
            setOpen(false);
          }}
        />
      )}
    </div>
  );
}

function Calendar(props: { selected: string; today: string; onPick: (day: string) => void }) {
  const [month, setMonth] = useState(props.selected.slice(0, 7));
  const stats = useMonthOverview(month);
  const [y, m] = month.split("-").map(Number);
  const first = new Date(y, m - 1, 1);
  const daysInMonth = new Date(y, m, 0).getDate();
  const lead = (first.getDay() + 6) % 7; // 周一开头
  const p = (n: number) => String(n).padStart(2, "0");
  const statOf = (day: string) => stats.find((s) => s.day === day);
  const maxGain = Math.max(3600, ...stats.map((s) => s.gain));
  const shiftMonth = (k: number) => {
    const t = new Date(y, m - 1 + k, 1);
    setMonth(`${t.getFullYear()}-${p(t.getMonth() + 1)}`);
  };
  const thisMonth = props.today.slice(0, 7);

  return (
    <div className="cal">
      <div className="cal-head">
        <button className="icon-btn" onClick={() => shiftMonth(-1)}>
          <Icon name="chevronLeft" />
        </button>
        <span>
          {y}年{m}月
        </span>
        <button className="icon-btn" onClick={() => shiftMonth(1)} disabled={month >= thisMonth}>
          <Icon name="chevronRight" />
        </button>
      </div>
      <div className="cal-grid">
        {"一二三四五六日".split("").map((w) => (
          <span key={w} className="cal-w">
            {w}
          </span>
        ))}
        {Array.from({ length: lead }, (_, i) => (
          <span key={`l${i}`} />
        ))}
        {Array.from({ length: daysInMonth }, (_, i) => {
          const day = `${month}-${p(i + 1)}`;
          const s = statOf(day);
          const future = day > props.today;
          const cls = [
            "cal-d",
            day === props.selected && "sel",
            day === props.today && "today",
            s?.reviewed && "reviewed",
          ]
            .filter(Boolean)
            .join(" ");
          return (
            <button
              key={day}
              className={cls}
              disabled={future}
              onClick={() => props.onPick(day)}
              title={s ? `有益 ${fmtDur(s.gain)} · 消耗 ${fmtDur(s.drain)}${s.reviewed ? " · 已复盘" : ""}` : undefined}
            >
              {i + 1}
              {s && s.gain > 0 && <i className="cal-heat" style={{ opacity: 0.35 + 0.65 * Math.min(1, s.gain / maxGain) }} />}
            </button>
          );
        })}
      </div>
      <div className="cal-legend">
        <span>
          <i className="cal-heat demo" /> 有益时长
        </span>
        <span>
          <i className="ring" /> 已复盘
        </span>
        {month !== thisMonth || props.selected !== props.today ? (
          <button className="ghost small" onClick={() => props.onPick(props.today)}>
            今天
          </button>
        ) : null}
      </div>
    </div>
  );
}

// ---------- 概览 ----------

function Stats({ data, now }: { data: DaySummary; now: number }) {
  const gains = data.activities.filter((a) => a.kind === "gain");
  const drains = data.activities.filter((a) => a.kind === "drain");
  const sum = (list: Activity[]) => list.reduce((s, a) => s + secondsOf(data, a.id, now), 0);
  const gainTarget = gains.reduce((s, a) => s + a.dailyMinutes * 60, 0);
  const drainLimit = drains.reduce((s, a) => s + a.dailyMinutes * 60, 0);
  const done = gains.filter((a) => a.dailyMinutes > 0 && secondsOf(data, a.id, now) >= a.dailyMinutes * 60).length;
  const withTarget = gains.filter((a) => a.dailyMinutes > 0).length;
  const g = sum(gains);
  const dr = sum(drains);

  return (
    <div className="stats">
      <div className="stat">
        <span className="stat-k">有益</span>
        <span className="stat-v gain">{fmtDur(g)}</span>
        {gainTarget > 0 && <span className="stat-s">目标 {Math.round((g / gainTarget) * 100)}%</span>}
      </div>
      <div className="stat">
        <span className="stat-k">消耗</span>
        <span className={`stat-v ${drainLimit > 0 && dr > drainLimit ? "bad" : "drain"}`}>{fmtDur(dr)}</span>
        {drainLimit > 0 && <span className="stat-s">上限 {fmtDur(drainLimit)}</span>}
      </div>
      <div className="stat">
        <span className="stat-k">达标</span>
        <span className="stat-v">
          {done}
          <small>/{withTarget}</small>
        </span>
        <span className="stat-s">有目标的有益项</span>
      </div>
      <div className="stat">
        <span className="stat-k">自评</span>
        <span className="stat-v">{data.review.score ?? "–"}</span>
        <span className="stat-s">{data.review.score ? "满分 5" : "还没复盘"}</span>
      </div>
    </div>
  );
}

const DAY_MS = 24 * 3600 * 1000;

function Timeline({ data, now }: { data: DaySummary; now: number }) {
  const start = dayStartMs(data.day, data.dayStartHour);
  const actOf = (id: string) => data.activities.find((a) => a.id === id);
  const blocks = data.entries
    .filter((e) => e.startAt !== null)
    .map((e) => ({ key: e.id, act: actOf(e.activityId), from: e.startAt!, secs: e.seconds, note: e.note }));
  if (data.running && data.day === data.today) {
    blocks.push({
      key: "running",
      act: actOf(data.running.activityId),
      from: data.running.startAt,
      secs: Math.floor((now - data.running.startAt) / 1000),
      note: "计时中",
    });
  }
  const unplaced = data.entries.filter((e) => e.startAt === null);
  const ticks = Array.from({ length: 13 }, (_, i) => i * 2);
  const pct = (ms: number) => Math.min(100, Math.max(0, ((ms - start) / DAY_MS) * 100));

  return (
    <div>
      <div className="tl">
        {ticks.map((h) => (
          <span key={h} className="tl-tick" style={{ left: `${(h / 24) * 100}%` }}>
            <span>{String((data.dayStartHour + h) % 24).padStart(2, "0")}</span>
          </span>
        ))}
        {data.day === data.today && <span className="tl-now" style={{ left: `${pct(now)}%` }} />}
        {blocks.map((b) => {
          const left = pct(b.from);
          const width = Math.max(pct(b.from + b.secs * 1000) - left, 0.3);
          return (
            <span
              key={b.key}
              className="tl-block"
              style={{ left: `${left}%`, width: `${width}%`, background: b.act?.color ?? "#888" }}
              title={`${b.act?.name ?? "已删除项目"}  ${fmtTime(b.from)}–${fmtTime(b.from + b.secs * 1000)}  ${fmtDur(b.secs)}${b.note ? "\n" + b.note : ""}`}
            />
          );
        })}
      </div>
      <div className="tl-legend">
        {data.activities.map((a) => (
          <span key={a.id}>
            <i className="dot" style={{ background: a.color }} />
            {a.name}
          </span>
        ))}
        {unplaced.length > 0 && (
          <span className="muted">
            · 未定位：
            {unplaced.map((e) => `${actOf(e.activityId)?.name ?? "已删除"} ${fmtDur(e.seconds)}`).join("、")}
          </span>
        )}
      </div>
    </div>
  );
}

function Compare({ data, now }: { data: DaySummary; now: number }) {
  return (
    <div className="cmp">
      <div className="cmp-row cmp-h">
        <span>项目</span>
        <span />
        <span>今日</span>
        <span>昨日</span>
        <span>7 日均</span>
        <span>较均值</span>
      </div>
      {data.activities.map((a) => {
        const sec = secondsOf(data, a.id, now);
        const avg = data.avg7[a.id] ?? 0;
        const diff = sec - avg;
        const good = a.kind === "gain" ? diff >= 0 : diff <= 0;
        const flat = Math.abs(diff) < 60;
        return (
          <div className="cmp-row" key={a.id}>
            <span className="cmp-name">
              <i className="dot" style={{ background: a.color }} />
              {a.name}
            </span>
            <span className="cmp-bar">
              <Bar a={a} seconds={sec} />
              <span className="cmp-target">
                {a.dailyMinutes > 0 ? `${a.kind === "gain" ? "目标" : "上限"} ${fmtDur(a.dailyMinutes * 60)}` : ""}
              </span>
            </span>
            <b>{fmtDur(sec)}</b>
            <span className="muted">{fmtDur(data.yesterday[a.id] ?? 0)}</span>
            <span className="muted">{fmtDur(avg)}</span>
            <span className={`delta ${flat ? "" : good ? "good" : "bad"}`}>
              {flat ? "持平" : `${diff > 0 ? "↑" : "↓"} ${fmtDur(Math.abs(diff))}`}
            </span>
          </div>
        );
      })}
    </div>
  );
}

function Entries({ data }: { data: DaySummary }) {
  const [confirm, setConfirm] = useState<string | null>(null);
  const actOf = (id: string) => data.activities.find((a) => a.id === id);

  return (
    <div>
      {data.entries.length === 0 && <p className="muted empty">这一天还没有记录，可以在下面补记。</p>}
      <ul className="entries">
        {data.entries.map((e) => (
          <li key={e.id}>
            <span className="e-time">
              {e.startAt !== null && e.endAt !== null ? `${fmtTime(e.startAt)} – ${fmtTime(e.endAt)}` : "未定位"}
            </span>
            <span className="e-act">
              <i className="dot" style={{ background: actOf(e.activityId)?.color ?? "#888" }} />
              {actOf(e.activityId)?.name ?? "已删除项目"}
            </span>
            <span className="e-dur">{fmtDur(e.seconds)}</span>
            <span className={`e-src src-${e.source}`}>{SOURCE_LABEL[e.source] ?? e.source}</span>
            <span className="e-note">{e.note}</span>
            <button
              className={`icon-btn ${confirm === e.id ? "danger-on" : ""}`}
              title={confirm === e.id ? "再点一次确认删除" : "删除"}
              onClick={() => (confirm === e.id ? api.deleteEntry(e.id) : setConfirm(e.id))}
              onBlur={() => setConfirm(null)}
            >
              {confirm === e.id ? "确认" : <Icon name="trash" />}
            </button>
          </li>
        ))}
      </ul>
      <AddEntry data={data} />
    </div>
  );
}

function AddEntry({ data }: { data: DaySummary }) {
  const [activityId, setActivityId] = useState(data.activities[0]?.id ?? "");
  const [dur, setDur] = useState("");
  const [start, setStart] = useState("");
  const [note, setNote] = useState("");
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    if (!data.activities.some((a) => a.id === activityId)) setActivityId(data.activities[0]?.id ?? "");
  }, [data.activities, activityId]);

  const submit = () => {
    const seconds = parseDuration(dur);
    if (!activityId) return setErr("先在「项目」里添加项目");
    if (!seconds) return setErr("时长格式：25 / 25m / 1h20m / 1:20");
    let startAt: number | undefined;
    if (start.trim()) {
      const t = timeOnDay(data.day, start, data.dayStartHour);
      if (t === null) return setErr("开始时间格式：HH:MM");
      startAt = t;
    }
    api.addEntry({ activityId, seconds, startAt, day: data.day, note }).then(
      () => {
        setDur("");
        setStart("");
        setNote("");
        setErr(null);
      },
      (e) => setErr(String(e)),
    );
  };

  return (
    <form
      className="add-entry"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <select value={activityId} onChange={(e) => setActivityId(e.target.value)}>
        {data.activities.map((a) => (
          <option key={a.id} value={a.id}>
            {a.name}
          </option>
        ))}
      </select>
      <input className="w-s" placeholder="时长 25m" value={dur} onChange={(e) => setDur(e.target.value)} />
      <input className="w-s" placeholder="开始 HH:MM" value={start} onChange={(e) => setStart(e.target.value)} />
      <input className="w-l" placeholder="备注（可空）" value={note} onChange={(e) => setNote(e.target.value)} />
      <button type="submit" className="primary">
        <Icon name="plus" size={12} /> 补记
      </button>
      {err && <span className="r-err inline">{err}</span>}
    </form>
  );
}

function ReviewForm({ data }: { data: DaySummary }) {
  const [form, setForm] = useState<ReviewData>(data.review);
  const [saved, setSaved] = useState(true);
  const loadedDay = useRef(data.day);

  // 只在切换日期时用库里的数据覆盖表单，避免刷新冲掉正在输入的内容
  useEffect(() => {
    if (loadedDay.current !== data.day) {
      loadedDay.current = data.day;
      setForm(data.review);
      setSaved(true);
    }
  }, [data.day, data.review]);

  const set = (patch: Partial<ReviewData>) => {
    setForm({ ...form, ...patch });
    setSaved(false);
  };

  const save = () => api.saveReview({ ...form, day: data.day }).then(() => setSaved(true));

  const fields: [keyof ReviewData, string, string][] = [
    ["best", "今天最满意的一段时间", "做了什么、为什么顺利"],
    ["worst", "最浪费的一段时间", "当时在逃避什么？"],
    ["tomorrow", "明天只改一件事", "具体到时间和动作"],
  ];

  return (
    <div className="review-form">
      <div className="rf-grid">
        {fields.map(([k, label, ph]) => (
          <label key={k}>
            <span>{label}</span>
            <textarea rows={3} placeholder={ph} value={form[k] as string} onChange={(e) => set({ [k]: e.target.value })} />
          </label>
        ))}
      </div>
      <div className="score">
        <span className="muted">今天给自己打几分</span>
        <div className="score-dots">
          {[1, 2, 3, 4, 5].map((n) => (
            <button
              key={n}
              className={form.score !== null && n <= form.score ? "on" : ""}
              onClick={() => set({ score: form.score === n ? null : n })}
            >
              {n}
            </button>
          ))}
        </div>
        <span className="spacer" />
        <button className="primary" onClick={save} disabled={saved}>
          {saved ? (
            <>
              <Icon name="check" size={12} /> 已保存
            </>
          ) : (
            "保存复盘"
          )}
        </button>
      </div>
    </div>
  );
}

// ---------- 项目设置 ----------

function Settings({ activities }: { activities: Activity[] }) {
  return (
    <ul className="settings">
      {activities.map((a, i) => (
        <ActivityRow key={a.id} a={a} first={i === 0} last={i === activities.length - 1} />
      ))}
      <ActivityRow key={`new-${activities.length}`} />
    </ul>
  );
}

function ActivityRow({ a, first, last }: { a?: Activity; first?: boolean; last?: boolean }) {
  const [name, setName] = useState(a?.name ?? "");
  const [color, setColor] = useState(a?.color ?? "#8a7cff");
  const [kind, setKind] = useState<Kind>(a?.kind ?? "gain");
  const [minutes, setMinutes] = useState(String(a?.dailyMinutes ?? 30));
  const [confirm, setConfirm] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  // 外部（如小组件）改了目标后同步
  useEffect(() => {
    if (a) setMinutes(String(a.dailyMinutes));
  }, [a?.dailyMinutes]); // eslint-disable-line react-hooks/exhaustive-deps

  const dirty =
    !a || name !== a.name || color !== a.color || kind !== a.kind || minutes !== String(a.dailyMinutes);

  const save = () => {
    const n = Number(minutes);
    if (!Number.isFinite(n) || n < 0) return setErr("分钟数无效");
    api.saveActivity({ id: a?.id, name, color, kind, dailyMinutes: Math.round(n) }).then(
      () => setErr(null),
      (e) => setErr(String(e)),
    );
  };

  return (
    <li className={a ? "" : "new"}>
      <label className="swatch" style={{ background: color }}>
        <input type="color" value={color} onChange={(e) => setColor(e.target.value)} />
      </label>
      <input
        className="w-m"
        placeholder={a ? "" : "新项目名称"}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && dirty && name.trim() && save()}
      />
      <div className="seg small">
        <button className={kind === "gain" ? "on" : ""} onClick={() => setKind("gain")}>
          有益
        </button>
        <button className={kind === "drain" ? "on" : ""} onClick={() => setKind("drain")}>
          消耗
        </button>
      </div>
      <span className="muted">{kind === "gain" ? "每日目标" : "每日上限"}</span>
      <input
        className="w-xs"
        inputMode="numeric"
        value={minutes}
        onChange={(e) => setMinutes(e.target.value.replace(/[^\d]/g, ""))}
        onKeyDown={(e) => e.key === "Enter" && dirty && name.trim() && save()}
      />
      <span className="muted">分钟</span>
      {(dirty || !a) && (
        <button className="primary" onClick={save} disabled={!name.trim()}>
          {a ? "保存" : "添加"}
        </button>
      )}
      {a && (
        <span className="row-tools">
          <button className="icon-btn" disabled={first} title="上移" onClick={() => api.moveActivity(a.id, -1)}>
            <Icon name="chevronUp" />
          </button>
          <button className="icon-btn" disabled={last} title="下移" onClick={() => api.moveActivity(a.id, 1)}>
            <Icon name="chevronDown" />
          </button>
          <button
            className={`icon-btn ${confirm ? "danger-on" : ""}`}
            title={confirm ? "再点一次确认删除" : "删除"}
            onClick={() => (confirm ? api.deleteActivity(a.id) : setConfirm(true))}
            onBlur={() => setConfirm(false)}
          >
            {confirm ? "确认" : <Icon name="trash" />}
          </button>
        </span>
      )}
      {err && <span className="r-err inline">{err}</span>}
    </li>
  );
}

// ---------- 外观 ----------

function Appearance() {
  const { theme, background, accent, saveTheme } = useTheme();
  const [dir, setDir] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    api.dataDir().then(setDir);
  }, []);

  const pick = async (file: File | undefined) => {
    if (!file) return;
    setBusy(true);
    try {
      await api.setBackground(await prepareImage(file));
      setErr(null);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
      if (fileRef.current) fileRef.current.value = "";
    }
  };

  return (
    <>
      <Card title="背景">
        <div className="appear">
          <div className="preview">
            <div
              className="preview-img"
              style={{
                backgroundImage: background ? `url("${background}")` : undefined,
                backgroundPosition: `center ${theme.focusY}%`,
                filter: `blur(${theme.blur}px)`,
              }}
            />
            <div className="preview-shade" style={{ opacity: background ? theme.overlay : 1 }} />
            <div className="preview-widget">
              <span className="pw-title">小组件预览</span>
              <span className="pw-bar" style={{ width: "72%" }} />
              <span className="pw-bar" style={{ width: "40%", background: "#22b07d" }} />
              <span className="pw-bar" style={{ width: "88%", background: "#f0a020" }} />
            </div>
          </div>
          <div className="appear-ctl">
            <div className="row">
              <button className="primary" disabled={busy} onClick={() => fileRef.current?.click()}>
                <Icon name="image" size={13} /> {busy ? "处理中…" : background ? "更换图片" : "选择图片"}
              </button>
              {background && (
                <button className="ghost" onClick={() => api.clearBackground()}>
                  移除
                </button>
              )}
              <input ref={fileRef} type="file" accept="image/*" hidden onChange={(e) => pick(e.target.files?.[0])} />
            </div>
            {err && <div className="r-err">{err}</div>}
            <Slider label="遮罩浓度" value={theme.overlay} min={0} max={0.9} step={0.05}
              fmt={(v) => `${Math.round(v * 100)}%`} onChange={(v) => saveTheme({ overlay: v })} />
            <Slider label="背景模糊" value={theme.blur} min={0} max={20} step={1}
              fmt={(v) => `${v}px`} onChange={(v) => saveTheme({ blur: v })} />
            <Slider label="图片焦点（上下）" value={theme.focusY} min={0} max={100} step={1}
              fmt={(v) => `${v}%`} onChange={(v) => saveTheme({ focusY: v })} />
            <p className="muted small">图片会复制到数据目录，原图可以随意移动或删除。</p>
          </div>
        </div>
      </Card>

      <Card title="主题色">
        <div className="row">
          <label className="swatch big" style={{ background: accent }}>
            <input type="color" value={accent} onChange={(e) => saveTheme({ accent: e.target.value })} />
          </label>
          <div className="seg small">
            <button className={theme.accent === null ? "on" : ""} onClick={() => saveTheme({ accent: null })}>
              从背景自动取色
            </button>
            <button className={theme.accent !== null ? "on" : ""} onClick={() => saveTheme({ accent })}>
              自定义
            </button>
          </div>
          <span className="muted small">用于计时状态、按钮和日历高亮</span>
        </div>
      </Card>

      <Card title="小组件缩放" hint="在小组件上按住 Ctrl 滚动滚轮也能缩放">
        <div className="row">
          {[0.85, 1, 1.15, 1.3].map((s) => (
            <button key={s} className={`ghost ${theme.widgetScale === s ? "on" : ""}`} onClick={() => saveTheme({ widgetScale: s })}>
              {s === 0.85 ? "小" : s === 1 ? "标准" : s === 1.15 ? "大" : "特大"}
            </button>
          ))}
          <Slider label="" value={theme.widgetScale} min={SCALE_MIN} max={SCALE_MAX} step={0.05}
            fmt={(v) => `${Math.round(v * 100)}%`} onChange={(v) => saveTheme({ widgetScale: v })} />
        </div>
      </Card>

      <Card title="数据">
        <div className="row">
          <code className="path">{dir}</code>
          <button className="ghost" onClick={() => api.openDataDir()}>
            <Icon name="folder" size={13} /> 打开文件夹
          </button>
        </div>
        <p className="muted small">timetrace.db 是全部记录，theme/ 下是背景图。备份时复制整个文件夹即可。</p>
      </Card>
    </>
  );
}

function Slider(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  fmt: (v: number) => string;
  onChange: (v: number) => void;
}) {
  // 拖动时本地即时更新，松手再保存
  const [v, setV] = useState(props.value);
  useEffect(() => setV(props.value), [props.value]);
  return (
    <label className="slider">
      {props.label && <span>{props.label}</span>}
      <input
        type="range"
        min={props.min}
        max={props.max}
        step={props.step}
        value={v}
        onChange={(e) => setV(Number(e.target.value))}
        onPointerUp={() => props.onChange(v)}
        onKeyUp={() => props.onChange(v)}
      />
      <span className="slider-v">{props.fmt(v)}</span>
    </label>
  );
}
