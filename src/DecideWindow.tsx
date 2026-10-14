import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { Verdict, api } from "./api";
import { Icon } from "./icons";
import { Backdrop } from "./theme";

const win = getCurrentWindow();

type Phase = { k: "input" } | { k: "loading" } | { k: "result"; v: Verdict } | { k: "error"; msg: string };

/** 屏幕中央的决策弹窗：只给答案——「做 / 不做」，或多个选项中的一个。不绑定任何操作。 */
export default function DecideWindow() {
  const [text, setText] = useState("");
  const [phase, setPhase] = useState<Phase>({ k: "input" });
  const inputRef = useRef<HTMLInputElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const phaseRef = useRef(phase);
  phaseRef.current = phase;

  const hide = () => {
    win.hide();
    setPhase({ k: "input" });
    setText("");
  };

  useEffect(() => {
    // 每次唤出：清空并聚焦输入框
    const un = listen("open-decision", () => {
      setText("");
      setPhase({ k: "input" });
      setTimeout(() => inputRef.current?.focus(), 30);
    });
    // 点到别处就收起（判断中除外）
    const unFocus = win.onFocusChanged(({ payload: focused }) => {
      if (!focused && phaseRef.current.k !== "loading") hide();
    });
    // Esc 随时关闭；看到答案后 Enter 也关闭
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" || (e.key === "Enter" && phaseRef.current.k === "result")) {
        e.preventDefault();
        hide();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      un.then((f) => f());
      unFocus.then((f) => f());
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  // 窗口尺寸跟随内容（含阴影留白），并保持在屏幕中央
  useLayoutEffect(() => {
    const el = stageRef.current;
    if (!el) return;
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
          msg: String(e) === "NO_KEY" ? "没检测到 TYPESAFE_API_KEY，设置后需要重启 TimeTrace" : String(e),
        }),
    );
  };

  return (
    <div className="dw-stage" ref={stageRef}>
      <div className={`dw-card ${phase.k}`}>
        <Backdrop extraBlur={28} />
        <div className="dw-content">{phase.k === "result" ? <Result text={text} v={phase.v} /> : renderInput()}</div>
        {phase.k === "loading" && <div className="dw-progress" />}
      </div>
    </div>
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
          <kbd className="dw-enter">↵</kbd>
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
            <span className="dw-tips-end">Esc 关闭</span>
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
      <span className="dw-close">Enter / Esc 关闭</span>
    </div>
  );
}
