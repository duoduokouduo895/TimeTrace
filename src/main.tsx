import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ThemeRoot } from "./theme";
import Widget from "./Widget";
import Review from "./Review";
import "./styles.css";

// 同一个前端，按窗口 label 区分：main = 小组件，review = 复盘页
const isReview = getCurrentWindow().label === "review";
document.body.classList.add(isReview ? "page-review" : "page-widget");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeRoot>{isReview ? <Review /> : <Widget />}</ThemeRoot>
  </React.StrictMode>,
);
