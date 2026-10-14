import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { DecisionProfile, DecisionRecord, api, fmtTime } from "./api";
import { Icon } from "./icons";

/** 复盘页「决策」：用法、个人目标档案、判断准则、最近的决策 */
const HOTKEYS = ["ctrl+alt+d", "ctrl+alt+j", "ctrl+alt+space", "ctrl+shift+space", "alt+shift+d"];
const hkLabel = (hk: string) =>
  hk
    .split("+")
    .map((k) => (k === "ctrl" ? "Ctrl" : k === "alt" ? "Alt" : k === "shift" ? "Shift" : k === "space" ? "Space" : k.toUpperCase()))
    .join(" + ");

function HotkeyRow() {
  const [st, setSt] = useState<{ hotkey: string; ok: boolean } | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    api.hotkeyStatus().then(setSt);
  }, []);
  if (!st) return null;
  return (
    <div className="row hotkey-row">
      <span>唤出快捷键</span>
      <select
        value={st.hotkey}
        onChange={(e) =>
          api.setHotkey(e.target.value).then(
            (s) => (setSt(s), setErr(null)),
            (er) => setErr(String(er)),
          )
        }
      >
        {(HOTKEYS.includes(st.hotkey) ? HOTKEYS : [st.hotkey, ...HOTKEYS]).map((h) => (
          <option key={h} value={h}>
            {hkLabel(h)}
          </option>
        ))}
      </select>
      <span className={`key-pill ${st.ok ? "ok" : "bad"}`}>{st.ok ? "可用" : "被其他程序占用，请换一个"}</span>
      {err && <span className="r-err inline">{err}</span>}
    </div>
  );
}

export default function DecisionPage() {
  const [p, setP] = useState<DecisionProfile | null>(null);
  const [goals, setGoals] = useState("");
  const [rules, setRules] = useState("");
  const [saved, setSaved] = useState(true);
  const [log, setLog] = useState<DecisionRecord[]>([]);

  useEffect(() => {
    api.decisionProfile().then((d) => {
      setP(d);
      setGoals(d.goals);
      setRules(d.rules);
    });
    const load = () => api.listDecisions(30).then(setLog);
    load();
    const un = listen("decisions-changed", load);
    return () => {
      un.then((f) => f());
    };
  }, []);

  if (!p) return <div className="muted">加载中…</div>;

  const save = () =>
    api.setSetting("decisionProfile", { goals, rules }).then(() => setSaved(true));

  const edit = (setter: (v: string) => void) => (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setter(e.target.value);
    setSaved(false);
  };

  return (
    <>
      <section className="card">
        <h2>
          怎么用
          <span className={`key-pill ${p.hasKey ? "ok" : "bad"}`}>
            {p.hasKey ? "已检测到 API Key" : "未检测到 TYPESAFE_API_KEY"}
          </span>
        </h2>
        <ol className="howto">
          <li>
            想摸鱼、犹豫要不要做某件事时，按下面设置的快捷键（或点小组件上的
            <Icon name="bolt" size={12} />），屏幕中央会弹出输入框
          </li>
          <li>写下「现在想做什么」，比如「打一把游戏」「刷 20 分钟 B 站」「先睡个午觉」，回车</li>
          <li>
            Jev 只回答 <b className="good">做</b> 或 <b className="bad">不做</b>，不给理由——下面的按钮点了就开始计时，不再犹豫
          </li>
        </ol>
        <HotkeyRow />
        <p className="muted small">
          Jev 会参考：下面的目标档案和判断准则、现在几点、今天各项进度、今天已经问过什么（同一件事被拒后再问，仍然不做）。
          Jev 以英文为主要训练语言，中文也能用；判断不准时，把准则写得更具体最有效。
        </p>
      </section>

      <section className="card">
        <h2>
          个人目标档案<span className="hint">你想成为什么样、近期最重要的事</span>
          <button className="ghost small push" onClick={() => (setGoals(p.defaultGoals), setSaved(false))}>
            恢复示例
          </button>
        </h2>
        <textarea className="profile" rows={6} value={goals} onChange={edit(setGoals)} />
      </section>

      <section className="card">
        <h2>
          判断准则<span className="hint">什么算学习、娱乐的边界、时间规矩——Jev 按这里判断</span>
          <button className="ghost small push" onClick={() => (setRules(p.defaultRules), setSaved(false))}>
            恢复示例
          </button>
        </h2>
        <textarea className="profile" rows={9} value={rules} onChange={edit(setRules)} />
        <div className="row save-row">
          <span className="muted small">保存后下一次决策立即生效</span>
          <span className="spacer" />
          <button className="primary" disabled={saved} onClick={save}>
            {saved ? (
              <>
                <Icon name="check" size={12} /> 已保存
              </>
            ) : (
              "保存档案与准则"
            )}
          </button>
        </div>
      </section>

      <section className="card">
        <h2>最近的决策</h2>
        {log.length === 0 ? (
          <p className="muted empty">还没有决策记录。</p>
        ) : (
          <ul className="dlog">
            {log.map((d) => (
              <li key={d.id}>
                <span className="dlog-time">
                  {new Date(d.askedAt).getMonth() + 1}/{new Date(d.askedAt).getDate()} {fmtTime(d.askedAt)}
                </span>
                <span className={`dlog-v ${d.decision}`}>{d.decision === "do" ? "做" : "不做"}</span>
                <span className="dlog-q">{d.question}</span>
                <span className="dlog-a">{d.action ? d.action.replace(":", " · ") : ""}</span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </>
  );
}
