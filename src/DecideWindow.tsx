import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { Verdict, api, useDaySummary } from "./api";
import { Icon } from "./icons";
import { Backdrop } from "./theme";

const WIDTH = 460;
const win = getCurrentWindow();

type Phase = { k: "input" } | { k: "loading" } | { k: "result"; v: Verdict } | { k: "error"; msg: string };

/** 屏幕中央的决策弹窗：写下想做的事 → Jev 只回答「做 / 不做」+ 一个立刻执行的按钮 */
export default function DecideWindow() {
  const { data } = useDaySummary();
  const [text, setText] = useState("");
  const [phase, setPhase] = useState<Phase>({ k: "input" });
  const inputRef = useRef<HTMLInputElement>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const phaseRef = useRef(phase);
  phaseRef.current = phase;

  const hide = () => {
    win.hide();
    setPhase({ k: "input" });
    setText("");
  };

  // 每次唤出：清空并聚焦输入框
  useEffect(() => {
    const un = listen("open-decision", () => {
      setText("");
      setPhase({ k: "input" });
      setTimeout(() => inputRef.current?.focus(), 30);
    });
    // 点到别处就收起（判断中除外）
    const unFocus = win.onFocusChanged(({ payload: focused }) => {
      if (!focused && phaseRef.current.k !== "loading") hide();
    });
    const esc = (e: KeyboardEvent) => e.key === "Escape" && hide();
    window.addEventListener("keydown", esc);
    return () => {
      un.then((f) => f());
      unFocus.then((f) => f());
      window.removeEventListener("keydown", esc);
    };
  }, []);

  // 窗口高度跟随卡片，并保持在屏幕中央
  useLayoutEffect(() => {
    const el = cardRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const h = Math.ceil(el.getBoundingClientRect().height);
      win.setSize(new LogicalSize(WIDTH, h)).then(() => win.center());
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const ask = () => {
    if (!text.trim()) return;
    setPhase({ k: "loading" });
    api.decide(text).then(
      (v) => setPhase({ k: "result", v }),
      (e) =>
        setPhase({
          k: "error",
          msg: String(e) === "NO_KEY" ? "没检测到 TYPESAFE_API_KEY。设置环境变量后需要重启 TimeTrace。" : String(e),
        }),
    );
  };

  const actOf = (id: string | null) => data?.activities.find((a) => a.id === id);

  let body: React.ReactNode;
  if (phase.k === "result") {
    const { v } = phase;
    const yes = v.decision === "do";
    // 做 → 若请求本身是已记录项目，直接开始计时；不做 → 开始 Jev 建议的有益项目
    const target = actOf(yes ? v.categoryId : v.insteadId);
    const go = () => {
      if (target) {
        api.startTimer(target.id);
        api.setDecisionAction(v.id, `开始计时:${target.name}`);
      }
      hide();
    };
    body = (
      <div className={`dw-result ${yes ? "do" : "dont"}`}>
        <span className="dw-q">{text}</span>
        <span className="dw-verdict">{yes ? "做" : "不做"}</span>
        <div className="dw-actions">
          <button className="primary" autoFocus onClick={go}>
            {target ? (
              <>
                <Icon name="play" size={11} /> {yes ? "开始计时" : "现在开始"} · {target.name}
              </>
            ) : yes ? (
              "去做吧"
            ) : (
              "好的"
            )}
          </button>
        </div>
      </div>
    );
  } else {
    body = (
      <>
        <form
          className="dw-input"
          onSubmit={(e) => {
            e.preventDefault();
            ask();
          }}
        >
          <Icon name="bolt" size={18} />
          <input
            ref={inputRef}
            autoFocus
            placeholder="现在想做什么？"
            value={text}
            disabled={phase.k === "loading"}
            onChange={(e) => {
              setText(e.target.value);
              if (phase.k === "error") setPhase({ k: "input" });
            }}
          />
        </form>
        {phase.k === "loading" && <div className="decide-loading" />}
        {phase.k === "error" ? (
          <div className="dw-hint err">{phase.msg}</div>
        ) : (
          <div className="dw-hint">
            <span>
              <kbd>Enter</kbd> 让 Jev 决定
            </span>
            <span>
              <kbd>Esc</kbd> 关闭
            </span>
          </div>
        )}
      </>
    );
  }

  return (
    <div className="dw-card" ref={cardRef} style={{ width: WIDTH }}>
      <Backdrop extraBlur={16} />
      <div className="dw-content">{body}</div>
    </div>
  );
}
