# TimeTrace

自用「每日时间记录 + 晚间复盘」Windows 桌面小组件。Tauri 2 + React + TypeScript + SQLite。设计见 [DESIGN.md](DESIGN.md)。

## 开发

```powershell
pnpm install
pnpm tauri dev      # 启动开发版（热更新）
pnpm tauri build    # 打包安装程序 → src-tauri/target/release/bundle/nsis/
```

数据层测试：

```powershell
cd src-tauri; cargo test --lib
```

## 使用

- **小组件**：每项一条时间条；▶ 开始/■ 停止计时（同时只计一项），＋ 手动录入时长（`25` / `25m` / `1h20m` / `1:20`，开始时间可空 = 刚结束）。
- 标题栏可拖动；月亮图标打开晚间复盘，图钉 = 置顶开关（高亮即已置顶），折叠，最小化，退出。
- 点「/1h」直接改每日目标或上限；按住 Ctrl 滚动滚轮缩放小组件。
- **晚间复盘**：点日期弹出小日历（有益时长 + 已复盘标记）；24h 时间轴、各项与昨日/7 日均值对比、记录明细与补记、一分钟复盘；「项目设置」里增删改项目、目标与上限。

## 数据

全部数据在项目根目录 `data/`（已加入 .gitignore）：`timetrace.db` 为记录，`theme/` 为背景图。备份复制整个文件夹即可。
