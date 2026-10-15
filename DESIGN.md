# TimeTrace 设计草案

自用「每日时间记录 + 晚间复盘」桌面小组件。只记录**已花掉的时间**，不做待办。

## 已确定的决策

| 项 | 决定 |
|---|---|
| 技术栈 | Tauri 2 + React + TypeScript + SQLite；后续同项目出 Android |
| 自动追踪 | 不做（不接 ActivityWatch），全部手动 / 外部写入 |
| 背单词（Codex 学习） | Codex 在学习结束时调用 CLI：`tt log 背单词 35m --start 20:10 --note "..."`，通过学习目录下的 `AGENTS.md` 约定 |
| 英语跟读（Englishpob） | 小组件内手动输入时长（默认结束时间=现在） |
| 面试学习 / 刷视频等 | 小组件计时器开始/停止，或补记 |
| 同步 | 暂不实现；数据模型预留（UUID、updated_at、软删除） |
| 防拖延决策 | 后期接 Jev（TypeSafe AI），模型调用层做成可替换适配器 |

## 界面

**小组件**（置顶、无边框、可折叠）：每项一条时间条。
- 有益类显示「已用 / 目标」，消耗类显示「已用 / 上限」，超限部分变红。
- 同一时刻只有一项计时，点另一项自动切换。
- 每项提供「+」手动录入时长。

**晚间复盘**（定时提醒打开）：
1. 24h 色块时间轴（无开始时间的记录单独列在「未定位」区）
2. 各项时间条 + 与昨日 / 近 7 日均值对比
3. 一分钟复盘：最满意的一段 / 最浪费的一段 / 明天改一件事
4. 周趋势

## 数据模型（SQLite）

表结构见 `src-tauri/src/db.rs` 中的 `SCHEMA_V1`，要点：

- `activity`：名称、颜色、`kind`（gain 有益 / drain 消耗）、`daily_minutes`（目标或上限，0 = 不设）、排序
- `entry`：归属日 `day`、可空的 `start_at`/`end_at`、`seconds`、`source`（timer / manual / cli）、备注
- `review`：每日一行，最满意 / 最浪费 / 明天改一件事 / 打分
- `running`：当前计时（最多一行），应用重启后计时不丢
- 所有业务表用 UUID 主键 + `updated_at` + 软删除 `deleted`，为同步预留
- 一天从 **凌晨 4:00** 开始算（`DAY_START_HOUR`），熬夜到 1 点仍算前一天
- 计时不足 30 秒视为误触，不落库
- 数据目录：项目根目录 `data/`（`timetrace.db` + `theme/background.jpg`），可用环境变量 `TIMETRACE_DATA` 覆盖
- `setting` 表（v2）：`theme`（遮罩、模糊、焦点、主题色、小组件缩放）、`pinned`，值为 JSON

## 外部写入：`tt` CLI

与小组件共用同一 SQLite（WAL 模式），小组件不开也能写；小组件监听变更刷新。

```
tt log <活动名> <时长> [--start HH:MM] [--day YYYY-MM-DD] [--note 文本]
tt today            # 输出今日汇总（JSON，可供模型读取）
```

## 复盘提醒

- 后台线程（`src-tauri/src/reminder.rs`）每 15 秒检查；到点且当天未复盘 → 恢复小组件窗口、播放内嵌 WAV（Win32 `PlaySoundW`）、发 `reminder` 事件
- 每个归属日最多自动提醒一次；可稍后提醒（snooze）；修改提醒时间后当天按新时间重新计算
- 设置项 `reminder`：`{ enabled, time: "HH:MM", sound }`

## 防拖延决策助手（后期）

- Jev（TypeSafe AI）文档：https://docs.typesafe.ai/introduction/quickstart
- 接口：`POST https://api.typesafe.ai/v1/systemone`，模型 `jev-latest`；Jev 是判断型模型（state + 选择题 → 选项与概率），不生成解释，天然满足「只要做/不做」
- 实现：`src-tauri/src/jev.rs`。两种问法，都只返回一个答案、不绑定任何操作：
  - 单件事（「刷视频」）→ `do` / `dont`，显示「做 / 不做」
  - 多个选项（用 还是/或者/与/vs/、/／ 分隔，如「睡觉与学习」）→ 选项本身作为 choice 的候选，直接回答其一；拆出的每段至少 2 个字，避免误拆「参与」等词
- 决策记录 `decision` 表 v4：`kind`（yesno/choice）、`decision`（do/dont 或选项）、`probability`（选中答案的概率）
- state（英文字段名）：当前时间、请求原文、个人目标档案、判断准则、今日各项进度与状态、正在计时、今日已问过的决策（防止反复问）
- Jev 以英文为主要训练语言，题干与选项描述用英文；用户档案/准则可中文
- API Key：用户环境变量 `TYPESAFE_API_KEY`（兼容 `JEV_API_KEY`），只在 Rust 后端读取
- 决策入口是屏幕中央的弹窗（`decide` 窗口，启动时隐藏创建、唤出即显示，失焦/Esc 收起）；全局快捷键默认 `Ctrl+Alt+D`，可在「决策」页更换，被占用时不影响启动

输入「现在想做 X / 应该做 Y」→ 提供给模型：
- 个人目标档案（长期目标，如面试准备、口语提升）
- 今日结构化状态（`tt today` 的 JSON）
- 近 7 日趋势

输出：建议 + 理由 + 一个最小起步动作（如「先做 10 分钟」）。

## 分期

1. **MVP**：小组件、计时器、手动录入、时间条、SQLite、晚间复盘页
2. `tt` CLI + Codex `AGENTS.md` 约定
3. 周趋势、复盘提醒
4. 防拖延决策助手（Jev）
5. Android 端 + 同步

## Android 端（进行中）

已定方向：App + 原生桌面小组件；手机单独存数据（以后再做同步）；全部手动记录，不做自动统计。

已完成（第 1 步，跨平台改造，尚未在真机编译）：
- Rust：平台差异集中在 `src-tauri/src/desktop.rs` / `mobile.rs`，`lib.rs` 通过 `platform::` 调用同名命令；桌面专用插件（window-state / autostart / global-shortcut）已移到 `cfg(not(android/ios))` 依赖
- 手机数据目录：`app_data_dir()`；Jev API Key 在手机上于「决策」页填写（存 `setting.jevApiKey`），电脑仍优先读 `TYPESAFE_API_KEY`
- 前端：`src/platform.ts` 的 `isMobile`（按 UA 判断）→ `src/MobileApp.tsx`（底部标签：今日/复盘/趋势/决策/设置，「做不做」为浮层）；`Review` 支持 `section` 属性嵌入

待做：
2. 生成工程并真机运行：设好 `NDK_HOME`、`rustup target add aarch64-linux-android`，手机开 USB 调试，然后 `pnpm tauri android init`、`pnpm tauri android dev`
3. 复盘提醒改为系统定时通知（手机进程会被回收，不能用后台线程）
4. Kotlin 原生桌面小组件（时间条 + 点按开始/停止计时）
5. `pnpm tauri android build` 打包 APK

## 多设备同步（WebDAV，端到端加密）

- 实现：`src-tauri/src/sync.rs`（同步流程）、`src-tauri/src/e2e.rs`（加密）
- 远端 `TimeTrace/devices/<设备ID>.json`，每台设备只写自己的文件；读取他人文件逐条合并，`updated_at` 较新者胜，软删除随之同步；同名项目归并为 ID 较小者
- 同步：记录、复盘、项目、决策记录、目标档案与准则；不同步：计时中状态、API Key、同步账号/口令、背景与设备专属设置
- 加密：Argon2id(同步口令, 盐) → XChaCha20-Poly1305，附加数据绑定文件名；云端只有密文。口令只存本机，所有设备须一致
- 流程：先拉取并解密其他设备（解不开的跳过并提示），再上传；本机口令一个都解不开时不上传，避免污染云端
- 节流：内容不变不上传（明文指纹）、对方 ETag 不变不下载；空闲时每 10 分钟 1 个 PROPFIND
- 端到端测试：`cargo test --lib webdav_round_trip -- --ignored`（需本地 WebDAV 模拟服务器）
