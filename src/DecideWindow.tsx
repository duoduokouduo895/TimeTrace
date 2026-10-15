import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { Verdict, api } from "./api";
import { Icon } from "./icons";
import { Backdrop } from "./theme";
import { isMobile } from "./platform";

const win = getCurrentWindow();

type Phase = { k: "input" } | { k: "loading" } | { k: "result"; v: Verdict } | { k: "error"; msg: string };

/**
 * 决策弹窗：只给答案——「做 / 不做」，或多个选项中的一个。不绑定任何操作。
 * 电脑上是独立的透明窗口；传入 onClose 时作为 App 内浮层使用（手机）。
 */
export default function DecideWindow({ onClose }: { onClose?: () => void } = {}) {
  const overlay = onClose !== undefined;
  const [text, setText] = useState("");
  const [phase, setPhase] = useState<Phase>({ k: "input" });
  const inputRef = useRef<HTMLInputElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const phaseRef = useRef(phase);
  phaseRef.current = phase;

  const hide = () => {
    if (overlay) onClose();
    else win.hide();
    setPhase({ k: "input" });
    setText("");
  };

  useEffect(() => {
    // Esc 随时关闭；看到答案后 Enter 也关闭
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" || (e.key === "Enter" && phaseRef.current.k === "result")) {
        e.preventDefault();
        hide();
      }
    };
    window.addEventListener("keydown", onKey);
    if (overlay) return () => window.removeEventListener("keydown", onKey);

    // 以下仅独立窗口：每次唤出时清空并聚焦输入框
    const un = listen("open-decision", () => {
      setText("");
      setPhase({ k: "input" });
      setTimeout(() => inputRef.current?.focus(), 30);
    });
    // 点到别处就收起（判断中除外）
    const unFocus = win.onFocusChanged(({ payload: focused }) => {
      if (!focused && phaseRef.current.k !== "loading") hide();
    });
    return () => {
      un.then((f) => f());
      unFocus.then((f) => f());
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  // 窗口尺寸跟随内容（含阴影留白），并保持在屏幕中央
  useLayoutEffect(() => {
    const el = stageRef.current;
    if (!el || overlay) return;
    const ro = new ResizeObserver(() => {
      const r = el.getBoundingClientRect();
      win.setSize(new LogicalSize(Math.ceil(r.width), Math.ceil(r.height))).then(() => win.center());
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const ask = () => {
    if (!text.trim() || phase.k === "loading") return;
    setPhase({ k: "loading" });
    api.decide(text).then(
      (v) => setPhase({ k: "result", v }),
      (e) =>
        setPhase({
          k: "error",
          msg:
            String(e) === "NO_KEY"
              ? overlay
                ? "还没有 Jev API Key，去「决策」页里填写"
                : "没检测到 TYPESAFE_API_KEY，设置后需要重启 TimeTrace"
              : String(e),
        }),
    );
  };

  const card = (
    <div className="dw-stage" ref={stageRef}>
      <div className={`dw-card ${phase.k}`} onClick={() => phase.k === "result" && overlay && hide()}>
        <Backdrop extraBlur={28} />
        <div className="dw-content">{phase.k === "result" ? <Result text={text} v={phase.v} /> : renderInput()}</div>
        {phase.k === "loading" && <div className="dw-progress" />}
      </div>
    </div>
  );
  // 浮层：点暗色背景关闭（判断中除外）
  return overlay ? (
    <div className="dw-overlay" onClick={(e) => e.target === e.currentTarget && phase.k !== "loading" && hide()}>
      {card}
    </div>
  ) : (
    card
  );

  function renderInput() {
    return (
      <>
        <form
          className="dw-input"
          onSubmit={(e) => {
            e.preventDefault();
            ask();
          }}
        >
          <span className="dw-orb">
            <Icon name="bolt" size={16} />
          </span>
          <input
            ref={inputRef}
            autoFocus
            placeholder="现在想做什么？"
            value={text}
            readOnly={phase.k === "loading"}
            onChange={(e) => {
              setText(e.target.value);
              if (phase.k === "error") setPhase({ k: "input" });
            }}
          />
          {isMobile ? (
            <button type="submit" className="dw-go" aria-label="让 Jev 决定">
              <Icon name="chevronRight" size={16} />
            </button>
          ) : (
            <kbd className="dw-enter">↵</kbd>
          )}
        </form>
        {phase.k === "error" ? (
          <div className="dw-tips err">{phase.msg}</div>
        ) : (
          <div className="dw-tips">
            <span>
              <b>刷视频</b> → 做 / 不做
            </span>
            <span>
              <b>睡觉与学习</b> → 二选一
            </span>
            <span className="dw-tips-end">{isMobile ? "点空白处关闭" : "Esc 关闭"}</span>
          </div>
        )}
      </>
    );
  }
}

function Result({ text, v }: { text: string; v: Verdict }) {
  const label = v.kind === "yesno" ? (v.answer === "do" ? "做" : "不做") : v.answer;
  const tone = v.kind === "yesno" ? (v.answer === "do" ? "do" : "dont") : "pick";
  const len = [...label].length;
  const size = len <= 2 ? 64 : len <= 4 ? 52 : len <= 7 ? 38 : 28;
  return (
    <div className={`dw-result ${tone}`}>
      <span className="dw-q">{text}</span>
      <span className="dw-answer" style={{ fontSize: size }}>
        {label}
      </span>
      <span className="dw-close">{isMobile ? "轻点关闭" : "Enter / Esc 关闭"}</span>
    </div>
  );
}
