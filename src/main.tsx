import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ThemeRoot } from "./theme";
import Widget from "./Widget";
import Review from "./Review";
import DecideWindow from "./DecideWindow";
import "./styles.css";

// 同一个前端，按窗口 label 区分：main = 小组件，review = 复盘页，decide = 决策弹窗
const label = getCurrentWindow().label;
document.body.classList.add(label === "review" ? "page-review" : "page-widget");
if (label !== "review") document.documentElement.classList.add("transparent-window");
const Page = label === "review" ? Review : label === "decide" ? DecideWindow : Widget;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeRoot>
      <Page />
    </ThemeRoot>
  </React.StrictMode>,
);
