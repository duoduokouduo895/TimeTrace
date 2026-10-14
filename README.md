# TimeTrace

自用「每日时间记录 + 晚间复盘」Windows 桌面小组件。Tauri 2 + React + TypeScript + SQLite。设计见 [DESIGN.md](DESIGN.md)。

## 开发

```powershell
pnpm install
pnpm tauri dev      # 启动开发版（热更新）
pnpm tauri build    # 打包安装程序 → src-tauri/target/release/bundle/nsis/TimeTrace_<版本>_x64-setup.exe
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

- **复盘提醒**：每晚定时（默认 22:30，复盘页 → 设置里改），当天没写复盘时小组件弹出横幅并播放提示音，可「10′」稍后提醒。需要程序在运行，建议在安装版里开启「开机自动启动」。

- **趋势**：复盘页「趋势」，近 7 / 30 天的日均有益/消耗（较前期变化）、达标与复盘天数、每日分向堆叠柱（有益向上、消耗向下）、各项小图，可切换表格。
- **防拖延决策**：按快捷键（默认 `Ctrl+Alt+D`，可在「决策」页更换）或小组件上的闪电图标，屏幕中央弹出输入框，写下「现在想做什么」→ Jev 只回答「做 / 不做」，附一个立刻执行的按钮（不做 → 开始建议的有益项目；做 → 若是已记录项目则直接计时）。目标档案与判断准则在复盘页「决策」里编辑。需要用户环境变量 `TYPESAFE_API_KEY`（设置后重启 TimeTrace）。

## 资源生成

- 提示音：`python scripts/gen-chime.py` → `src-tauri/sounds/review-chime.wav`（编译时内嵌进程序）
- 图标：`python scripts/gen-icon.py && pnpm tauri icon scripts/app-icon.png`

## 数据

全部数据在项目根目录 `data/`（已加入 .gitignore）：`timetrace.db` 为记录，`theme/` 为背景图。备份复制整个文件夹即可。
