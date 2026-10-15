import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ThemeRoot } from "./theme";
import Widget from "./Widget";
import Review from "./Review";
import DecideWindow from "./DecideWindow";
import MobileApp from "./MobileApp";
import { isMobile } from "./platform";
import "./styles.css";

// 同一个前端：手机上是单页 App；电脑上按窗口 label 区分 main = 小组件，review = 复盘页，decide = 决策弹窗
const label = getCurrentWindow().label;
let Page: React.ComponentType;
if (isMobile) {
  document.body.classList.add("page-review", "page-mobile");
  Page = MobileApp;
} else {
  document.body.classList.add(label === "review" ? "page-review" : "page-widget");
  if (label !== "review") document.documentElement.classList.add("transparent-window");
  Page = label === "review" ? Review : label === "decide" ? DecideWindow : Widget;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeRoot>
      <Page />
    </ThemeRoot>
  </React.StrictMode>,
);
