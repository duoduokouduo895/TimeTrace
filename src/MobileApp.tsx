import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { fmtClock, fmtDur, secondsOf, useDaySummary, useNow } from "./api";
import { Icon, IconName } from "./icons";
import { Backdrop } from "./theme";
import { Row } from "./Widget";
import Review, { Section } from "./Review";
import DecideWindow from "./DecideWindow";

type MTab = "today" | Section;

const TABS: [MTab, string, IconName][] = [
  ["today", "今日", "clock"],
  ["review", "复盘", "moon"],
  ["trends", "趋势", "chart"],
  ["decision", "决策", "bolt"],
  ["settings", "设置", "gear"],
];

/** 手机端外壳：底部标签栏 + 各页面；「做不做」是浮层 */
export default function MobileApp() {
  const [tab, setTab] = useState<MTab>("today");
  const [deciding, setDeciding] = useState(false);

  useEffect(() => {
    const a = listen("open-decision", () => setDeciding(true));
    const b = listen<MTab>("navigate", (e) => setTab(e.payload));
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
  }, []);

  return (
    <>
      <Backdrop extraBlur={14} />
      <main className="m-main">{tab === "today" ? <Today /> : <Review key={tab} section={tab} />}</main>
      {tab === "today" && (
        <button className="m-fab" onClick={() => setDeciding(true)}>
          <Icon name="bolt" size={16} /> 做不做
        </button>
      )}
      <nav className="m-nav">
        {TABS.map(([k, label, icon]) => (
          <button key={k} className={tab === k ? "on" : ""} onClick={() => setTab(k)}>
            <Icon name={icon} size={20} />
            <span>{label}</span>
          </button>
        ))}
      </nav>
      {deciding && <DecideWindow onClose={() => setDeciding(false)} />}
    </>
  );
}

const WEEKDAY = "日一二三四五六";

/** 今日：各项时间条，点 ▶ 计时，点 ＋ 手动录入，点「/1h」改目标 */
function Today() {
  const { data, error } = useDaySummary();
  const now = useNow(!!data?.running);
  const [adding, setAdding] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

  useEffect(() => {
    if (!msg) return;
    const t = setTimeout(() => setMsg(null), 3000);
    return () => clearTimeout(t);
  }, [msg]);

  if (!data) return <div className="muted">{error ?? "加载中…"}</div>;

  const [y, m, d] = data.today.split("-").map(Number);
  const sum = (kind: string) =>
    data.activities.filter((a) => a.kind === kind).reduce((s, a) => s + secondsOf(data, a.id, now), 0);
  const runningAct = data.activities.find((a) => a.id === data.running?.activityId);

  return (
    <>
      <header className="m-today-head">
        <div>
          <h1>
            {m}月{d}日
          </h1>
          <span className="muted">周{WEEKDAY[new Date(y, m - 1, d).getDay()]}</span>
        </div>
        <div className="m-sums">
          <span>
            有益<b className="gain">{fmtDur(sum("gain"))}</b>
          </span>
          <span>
            消耗<b>{fmtDur(sum("drain"))}</b>
          </span>
        </div>
      </header>

      {runningAct && data.running && (
        <div className="m-live">
          <i className="pulse" style={{ background: runningAct.color }} />
          <span>{runningAct.name}</span>
          <b>{fmtClock(Math.floor((now - data.running.startAt) / 1000))}</b>
        </div>
      )}

      {(error || msg) && <div className="w-msg">{error ?? msg}</div>}

      <section className="card m-list">
        <ul className="w-list">
          {data.activities.map((a) => (
            <Row
              key={a.id}
              a={a}
              data={data}
              now={now}
              adding={adding === a.id}
              onToggleAdd={() => setAdding(adding === a.id ? null : a.id)}
              onError={(e) => setMsg(String(e))}
            />
          ))}
          {data.activities.length === 0 && <li className="w-empty">还没有项目，去「设置」里添加</li>}
        </ul>
      </section>
      <p className="muted small m-hint">点 ▶ 开始计时，＋ 手动录入时长，点「/1h」修改每日目标或上限</p>
    </>
  );
}
