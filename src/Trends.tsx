import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Activity, RangeStats, api, fmtDur, shiftDay } from "./api";

const WEEKDAY = "日一二三四五六";

/** 近 7 / 30 天趋势：概览数字 + 每日有益/消耗分向堆叠柱 + 各项小图 */
export default function Trends({ today }: { today: string }) {
  const [n, setN] = useState<7 | 30>(7);
  const [view, setView] = useState<"chart" | "table">("chart");
  const [stats, setStats] = useState<RangeStats | null>(null);

  // 多取一个周期，用来算「较前 n 天」
  const from = shiftDay(today, -(2 * n - 1));
  useEffect(() => {
    const load = () => api.rangeStats(from, today).then(setStats);
    load();
    const un = listen("data-changed", load);
    return () => {
      un.then((f) => f());
    };
  }, [from, today]);

  const model = useMemo(() => (stats ? buildModel(stats, today, n) : null), [stats, today, n]);
  if (!model) return <div className="muted">加载中…</div>;

  return (
    <>
      <div className="trend-bar">
        <div className="seg">
          {([7, 30] as const).map((k) => (
            <button key={k} className={n === k ? "on" : ""} onClick={() => setN(k)}>
              近 {k} 天
            </button>
          ))}
        </div>
        <div className="seg small">
          <button className={view === "chart" ? "on" : ""} onClick={() => setView("chart")}>
            图表
          </button>
          <button className={view === "table" ? "on" : ""} onClick={() => setView("table")}>
            表格
          </button>
        </div>
      </div>

      <Tiles m={model} n={n} />

      {view === "chart" ? (
        <>
          <section className="card">
            <h2>
              每日用时<span className="hint">柱子向上为有益，向下为消耗；悬停查看明细</span>
            </h2>
            <Legend activities={model.activities} />
            <DailyChart m={model} n={n} />
          </section>
          <section className="card">
            <h2>
              各项走势<span className="hint">横线为每日目标 / 上限</span>
            </h2>
            <div className="multiples">
              {model.activities.map((a) => (
                <ActivityRow key={a.id} a={a} m={model} />
              ))}
            </div>
          </section>
        </>
      ) : (
        <section className="card">
          <h2>明细（分钟）</h2>
          <DataTable m={model} />
        </section>
      )}
    </>
  );
}

interface Model {
  days: string[];
  activities: Activity[];
  /** day -> activityId -> 秒 */
  cell: (day: string, id: string) => number;
  gainOf: (day: string) => number;
  drainOf: (day: string) => number;
  cur: Period;
  prev: Period;
}

interface Period {
  gain: number;
  drain: number;
  goalDays: number;
  reviewDays: number;
}

function buildModel(s: RangeStats, today: string, n: number): Model {
  const map = new Map<string, number>();
  for (const r of s.rows) map.set(`${r.day}|${r.activityId}`, r.seconds);
  const cell = (day: string, id: string) => map.get(`${day}|${id}`) ?? 0;
  const gains = s.activities.filter((a) => a.kind === "gain");
  const drains = s.activities.filter((a) => a.kind === "drain");
  const gainOf = (day: string) => gains.reduce((t, a) => t + cell(day, a.id), 0);
  const drainOf = (day: string) => drains.reduce((t, a) => t + cell(day, a.id), 0);
  const gainTarget = gains.reduce((t, a) => t + a.dailyMinutes * 60, 0);
  const reviewed = new Set(s.reviewed);
  const period = (end: string): Period => {
    const ds = Array.from({ length: n }, (_, i) => shiftDay(end, i - n + 1));
    return {
      gain: ds.reduce((t, d) => t + gainOf(d), 0) / n,
      drain: ds.reduce((t, d) => t + drainOf(d), 0) / n,
      goalDays: gainTarget > 0 ? ds.filter((d) => gainOf(d) >= gainTarget).length : 0,
      reviewDays: ds.filter((d) => reviewed.has(d)).length,
    };
  };
  return {
    days: Array.from({ length: n }, (_, i) => shiftDay(today, i - n + 1)),
    activities: [...gains, ...drains],
    cell,
    gainOf,
    drainOf,
    cur: period(today),
    prev: period(shiftDay(today, -n)),
  };
}

function Delta({ now, before, upIsGood }: { now: number; before: number; upIsGood: boolean }) {
  const d = now - before;
  if (Math.abs(d) < 60) return <span className="stat-s">与前期持平</span>;
  const good = d > 0 === upIsGood;
  return (
    <span className={`stat-s delta-inline ${good ? "good" : "bad"}`}>
      {d > 0 ? "↑" : "↓"} {fmtDur(Math.abs(d))} <span className="muted">较前期</span>
    </span>
  );
}

function Tiles({ m, n }: { m: Model; n: number }) {
  return (
    <div className="stats">
      <div className="stat">
        <span className="stat-k">日均有益</span>
        <span className="stat-v">{fmtDur(m.cur.gain)}</span>
        <Delta now={m.cur.gain} before={m.prev.gain} upIsGood />
      </div>
      <div className="stat">
        <span className="stat-k">日均消耗</span>
        <span className="stat-v">{fmtDur(m.cur.drain)}</span>
        <Delta now={m.cur.drain} before={m.prev.drain} upIsGood={false} />
      </div>
      <div className="stat">
        <span className="stat-k">有益达标天数</span>
        <span className="stat-v">
          {m.cur.goalDays}
          <small>/{n}</small>
        </span>
        <span className="stat-s">有益总时长 ≥ 目标总和</span>
      </div>
      <div className="stat">
        <span className="stat-k">复盘天数</span>
        <span className="stat-v">
          {m.cur.reviewDays}
          <small>/{n}</small>
        </span>
        <span className="stat-s">前期 {m.prev.reviewDays} 天</span>
      </div>
    </div>
  );
}

function Legend({ activities }: { activities: Activity[] }) {
  return (
    <div className="legend">
      {activities.map((a) => (
        <span key={a.id}>
          <i className="swatch-sm" style={{ background: a.color }} />
          {a.name}
          <span className="muted">{a.kind === "gain" ? "↑" : "↓"}</span>
        </span>
      ))}
    </div>
  );
}

/** 刻度：按最大值挑 30 分钟 / 1h / 2h / 4h 的整数步长 */
function tickStep(maxSec: number) {
  const h = maxSec / 3600;
  return (h <= 1.5 ? 0.5 : h <= 4 ? 1 : h <= 8 ? 2 : 4) * 3600;
}

const CHART_H = 240;
const GAP = 2;

function DailyChart({ m, n }: { m: Model; n: number }) {
  const [hover, setHover] = useState<number | null>(null);
  const gains = m.activities.filter((a) => a.kind === "gain");
  const drains = m.activities.filter((a) => a.kind === "drain");
  const up = Math.max(1800, ...m.days.map(m.gainOf));
  const down = Math.max(drains.length ? 1800 : 0, ...m.days.map(m.drainOf));
  const step = tickStep(Math.max(up, down));
  const upTop = Math.ceil(up / step) * step;
  const downTop = Math.ceil(down / step) * step;
  const k = CHART_H / (upTop + downTop);
  const base = upTop * k;
  const ticks: number[] = [];
  for (let v = -downTop; v <= upTop; v += step) ticks.push(v);
  const fmtTick = (v: number) => (v === 0 ? "0" : `${Math.abs(v) / 3600}h`);
  const labelEvery = n <= 7 ? 1 : 5;

  return (
    <div className="dchart">
      <div className="dchart-y">
        {ticks.map((v) => (
          <span key={v} style={{ top: base - v * k }}>
            {fmtTick(v)}
          </span>
        ))}
      </div>
      <div className="dchart-plot" style={{ height: CHART_H }} onMouseLeave={() => setHover(null)}>
        {ticks.map((v) => (
          <i key={v} className={`grid ${v === 0 ? "zero" : ""}`} style={{ top: base - v * k }} />
        ))}
        <div className="dchart-cols">
          {m.days.map((day, i) => (
            <div
              key={day}
              className={`dcol ${hover !== null && hover !== i ? "dim" : ""}`}
              onMouseEnter={() => setHover(i)}
            >
              <Stack list={gains} day={day} m={m} k={k} dir="up" base={base} />
              <Stack list={drains} day={day} m={m} k={k} dir="down" base={base} />
            </div>
          ))}
        </div>
        {hover !== null && <Tip m={m} day={m.days[hover]} index={hover} n={m.days.length} />}
      </div>
      <div className="dchart-x">
        {m.days.map((day, i) => {
          const show = i % labelEvery === 0 || i === m.days.length - 1;
          const [y, mo, d] = day.split("-").map(Number);
          return (
            <span key={day} className={i === m.days.length - 1 ? "today" : ""}>
              {show && (
                <>
                  {mo}/{d}
                  {n <= 7 && <small>周{WEEKDAY[new Date(y, mo - 1, d).getDay()]}</small>}
                </>
              )}
            </span>
          );
        })}
      </div>
    </div>
  );
}

function Stack(props: { list: Activity[]; day: string; m: Model; k: number; dir: "up" | "down"; base: number }) {
  const segs = props.list
    .map((a) => ({ a, h: props.m.cell(props.day, a.id) * props.k }))
    .filter((s) => s.h >= 1);
  if (segs.length === 0) return null;
  const up = props.dir === "up";
  return (
    <div className={`dstack ${props.dir}`} style={up ? { bottom: CHART_H - props.base + GAP / 2 } : { top: props.base + GAP / 2 }}>
      {segs.map((s, i) => (
        <span
          key={s.a.id}
          style={{
            height: Math.max(1, s.h - GAP),
            background: s.a.color,
            // 远离基线的一端圆角，基线一端方角
            borderRadius: i === segs.length - 1 ? (up ? "4px 4px 0 0" : "0 0 4px 4px") : 0,
          }}
        />
      ))}
    </div>
  );
}

function Tip({ m, day, index, n }: { m: Model; day: string; index: number; n: number }) {
  const [y, mo, d] = day.split("-").map(Number);
  const left = ((index + 0.5) / n) * 100;
  return (
    <div className={`dtip ${left > 60 ? "left" : ""}`} style={{ left: `${left}%` }}>
      <b>
        {mo}月{d}日 周{WEEKDAY[new Date(y, mo - 1, d).getDay()]}
      </b>
      {m.activities.map((a) => (
        <div key={a.id} className="dtip-row">
          <i className="swatch-sm" style={{ background: a.color }} />
          <span>{a.name}</span>
          <span className="dtip-v">{fmtDur(m.cell(day, a.id))}</span>
        </div>
      ))}
      <div className="dtip-row total">
        <span>有益 {fmtDur(m.gainOf(day))}</span>
        <span>消耗 {fmtDur(m.drainOf(day))}</span>
      </div>
    </div>
  );
}

function ActivityRow({ a, m }: { a: Activity; m: Model }) {
  const vals = m.days.map((d) => m.cell(d, a.id));
  const total = vals.reduce((t, v) => t + v, 0);
  const limit = a.dailyMinutes * 60;
  const max = Math.max(limit, ...vals, 1800);
  const H = 40;
  const hit = limit > 0 ? vals.filter((v) => (a.kind === "gain" ? v >= limit : v > limit)).length : null;
  return (
    <div className="mrow">
      <div className="mrow-name">
        <i className="swatch-sm" style={{ background: a.color }} />
        {a.name}
      </div>
      <div className="mrow-nums">
        <span>
          合计 <b>{fmtDur(total)}</b>
        </span>
        <span>
          日均 <b>{fmtDur(total / m.days.length)}</b>
        </span>
        {hit !== null && (
          <span className={a.kind === "drain" && hit > 0 ? "bad" : ""}>
            {a.kind === "gain" ? "达标" : "超限"} <b>{hit}</b> 天
          </span>
        )}
      </div>
      <div className="mini" style={{ height: H }}>
        {limit > 0 && <i className="mini-line" style={{ bottom: (limit / max) * H }} />}
        {vals.map((v, i) => (
          <span key={m.days[i]} className="mini-col" title={`${m.days[i]}  ${fmtDur(v)}`}>
            <span style={{ height: v > 0 ? Math.max(2, (v / max) * H) : 0, background: a.color }} />
          </span>
        ))}
      </div>
    </div>
  );
}

function DataTable({ m }: { m: Model }) {
  const mins = (s: number) => (s ? Math.round(s / 60) : "–");
  return (
    <div className="dtable-wrap">
      <table className="dtable">
        <thead>
          <tr>
            <th>日期</th>
            {m.activities.map((a) => (
              <th key={a.id}>
                <i className="swatch-sm" style={{ background: a.color }} />
                {a.name}
              </th>
            ))}
            <th>有益</th>
            <th>消耗</th>
          </tr>
        </thead>
        <tbody>
          {[...m.days].reverse().map((d) => (
            <tr key={d}>
              <td>{d.slice(5)}</td>
              {m.activities.map((a) => (
                <td key={a.id}>{mins(m.cell(d, a.id))}</td>
              ))}
              <td>
                <b>{mins(m.gainOf(d))}</b>
              </td>
              <td>{mins(m.drainOf(d))}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
