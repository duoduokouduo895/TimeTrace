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

## 防拖延决策助手（后期）

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
