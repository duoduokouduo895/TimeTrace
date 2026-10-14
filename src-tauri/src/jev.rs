//! 防拖延决策助手：把「此刻的情境」交给 Jev（TypeSafe AI System One），只要「做 / 不做」。
//! 文档：https://docs.typesafe.ai/introduction/quickstart
//!
//! Jev 是判断型模型：state（情境）+ questions（选择题）→ 每题的选项与概率，不生成解释。
//! 官方说明其主要训练语言为英文，因此题干与选项描述用英文；用户的档案/准则可以是中文。

use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use chrono::{Datelike, Local, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::db::{self, DaySummary};

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const MODEL: &str = "jev-latest";

pub const DEFAULT_GOALS: &str = "\
- 近期最重要：准备求职面试，每天面试学习至少 2 小时
- 英语：每天背单词 1 小时、英语跟读 45 分钟，目标是能流利口语交流
- 作息：24:00 前睡觉，保证第二天精力
- 我有拖延症，常用刷视频、打游戏来逃避该做的事";

pub const DEFAULT_RULES: &str = "\
【什么算学习】背单词、英语跟读、面试刷题与八股、写代码做项目、读技术书——以练习和产出为主
【什么不算学习】看「学习类」短视频或直播、收藏资料不看、反复整理笔记和计划、刷技术圈新闻；这些按娱乐算
【娱乐】游戏、刷视频、闲逛社交媒体都是娱乐：当天有益目标基本完成后才可以，单次不超过 30 分钟
【休息与必需】吃饭、洗漱、运动、连续学习 50 分钟后休息 10 分钟、困了就睡——这些随时可以做
【时间】23:00 以后不开始任何娱乐；23:30 以后只做睡前准备
【反复询问】同一件事刚被拒绝又来问，仍然不做";

const DECISION_INSTRUCTIONS: &str = "\
The user struggles with procrastination and is asking whether to do something RIGHT NOW (user_request). \
Decide only by what is best for the user's long-term goals (personal_goals) under the user's own decision_rules, \
taking into account current_time, today_progress, timer_running and recent_decisions_today. \
If the request is a way to avoid unfinished beneficial work, it must not be done. \
Necessities and rest that the rules allow may be done. \
A request that was refused recently and is asked again should still be refused unless the situation has clearly changed.";

const DO_DESC: &str = "\
Do it now: it is beneficial work according to the goals and rules, or it is a necessity / rest that the rules allow at this time, \
and it does not exceed any daily limit or break any rule.";

const DONT_DESC: &str = "\
Don't do it now: it is procrastination or entertainment while beneficial targets are still unmet, \
it would exceed a consuming-activity limit, it breaks a rule (for example it is too late at night), \
or it repeats a request that was recently refused.";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub id: String,
    /// "do" | "dont"
    pub decision: String,
    /// 请求所属的已记录项目（做 → 可直接开始计时）
    pub category_id: Option<String>,
    /// 不做时建议立刻开始的有益项目
    pub instead_id: Option<String>,
}

#[derive(Deserialize)]
struct ApiResponse {
    answers: serde_json::Map<String, Value>,
}

pub fn api_key() -> Option<String> {
    ["TYPESAFE_API_KEY", "JEV_API_KEY"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
}

fn fmt_time(ms: i64) -> String {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_default()
}

/// 组装 Jev 的 state：字段名用英文，内容保留用户原文
pub fn build_state(
    question: &str,
    goals: &str,
    rules: &str,
    s: &DaySummary,
    recent: &[db::Decision],
) -> Value {
    let now = Local::now();
    let weekday = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
        [now.weekday().num_days_from_monday() as usize];
    let now_ms = now.timestamp_millis();
    let running_secs = |id: &str| match &s.running {
        Some(r) if r.activity_id == id && s.day == s.today => (now_ms - r.start_at) / 1000,
        _ => 0,
    };
    let progress: Vec<Value> = s
        .activities
        .iter()
        .map(|a| {
            let minutes = (s.totals.get(&a.id).copied().unwrap_or(0) + running_secs(&a.id)) / 60;
            let gain = a.kind == "gain";
            let status = match (gain, a.daily_minutes) {
                (_, 0) => "no target set",
                (true, t) if minutes >= t => "target met",
                (true, _) => "below target",
                (false, t) if minutes > t => "over limit",
                (false, _) => "within limit",
            };
            json!({
                "activity": a.name,
                "type": if gain { "beneficial" } else { "consuming" },
                "minutes_done": minutes,
                if gain { "daily_target_minutes" } else { "daily_limit_minutes" }: a.daily_minutes,
                "status": status,
            })
        })
        .collect();
    let timer = s.running.as_ref().and_then(|r| {
        let a = s.activities.iter().find(|a| a.id == r.activity_id)?;
        Some(format!("{} ({} min so far)", a.name, (now_ms - r.start_at) / 60_000))
    });
    let recent: Vec<Value> = recent
        .iter()
        .map(|d| json!({ "time": fmt_time(d.asked_at), "request": d.question, "decision": d.decision }))
        .collect();
    json!({
        "current_time": format!("{} ({weekday})", now.format("%Y-%m-%d %H:%M")),
        "user_request": question,
        "personal_goals": goals,
        "decision_rules": rules,
        "today_progress": progress,
        "timer_running": timer,
        "recent_decisions_today": recent,
    })
}

pub fn build_questions(s: &DaySummary) -> Value {
    let mut q = serde_json::Map::new();
    q.insert(
        "decision".into(),
        json!({
            "type": "choice",
            "instructions": DECISION_INSTRUCTIONS,
            "criteria": { "do": DO_DESC, "dont": DONT_DESC },
        }),
    );
    if !s.activities.is_empty() {
        let mut c = serde_json::Map::new();
        for a in &s.activities {
            let kind = if a.kind == "gain" { "beneficial" } else { "consuming" };
            c.insert(a.name.clone(), json!(format!("The user's tracked {kind} activity \"{}\"", a.name)));
        }
        c.insert("none".into(), json!("The request is not any of the tracked activities"));
        q.insert(
            "category".into(),
            json!({
                "type": "choice",
                "instructions": "Which of the user's tracked activities is the user_request itself?",
                "criteria": c,
            }),
        );
    }
    let gains: Vec<_> = s.activities.iter().filter(|a| a.kind == "gain").collect();
    if !gains.is_empty() {
        let mut c = serde_json::Map::new();
        for a in gains {
            let done = s.totals.get(&a.id).copied().unwrap_or(0) / 60;
            c.insert(a.name.clone(), json!(format!("{} min done today, daily target {} min", done, a.daily_minutes)));
        }
        q.insert(
            "instead".into(),
            json!({
                "type": "choice",
                "instructions": "If the user should not do the request, which beneficial activity should they start right now instead? \
                                 Prefer the one furthest behind its daily target that fits the current time.",
                "criteria": c,
            }),
        );
    }
    Value::Object(q)
}

pub struct Answer {
    pub decision: String,
    pub p_do: Option<f64>,
    pub confidence: Option<f64>,
    pub category: Option<String>,
    pub instead: Option<String>,
}

pub async fn ask(key: &str, state: Value, questions: Value) -> Result<Answer> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(20)).build()?;
    let body = json!({ "state": state, "model": MODEL, "questions": questions });
    let mut wait = Duration::from_millis(800);
    let mut attempt = 0;
    let resp = loop {
        attempt += 1;
        let r = client.post(ENDPOINT).bearer_auth(key).json(&body).send().await?;
        let status = r.status().as_u16();
        // 429 / 529：官方建议指数退避重试
        if (status == 429 || status == 529) && attempt < 3 {
            tokio_sleep(wait).await;
            wait *= 2;
            continue;
        }
        if !r.status().is_success() {
            let text = r.text().await.unwrap_or_default();
            match status {
                401 => bail!("API Key 无效（401），请检查 TYPESAFE_API_KEY"),
                429 => bail!("请求太频繁（429），稍后再试"),
                529 => bail!("Jev 服务繁忙（529），稍后再试"),
                _ => bail!("Jev 返回 {status}: {}", text.chars().take(300).collect::<String>()),
            }
        }
        break r.json::<ApiResponse>().await?;
    };
    let choice = |id: &str| resp.answers.get(id).and_then(|a| a.get("choice")).and_then(|c| c.as_str()).map(String::from);
    let decision = choice("decision").ok_or_else(|| anyhow!("Jev 没有返回 decision"))?;
    let dec = resp.answers.get("decision");
    Ok(Answer {
        p_do: dec.and_then(|a| a.pointer("/probabilities/do")).and_then(|v| v.as_f64()),
        confidence: dec.and_then(|a| a.get("confidence")).and_then(|v| v.as_f64()),
        decision,
        category: choice("category").filter(|c| c != "none"),
        instead: choice("instead"),
    })
}

async fn tokio_sleep(d: Duration) {
    // 不引入 tokio 依赖：在阻塞线程池里睡眠
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary() -> DaySummary {
        // 复用正式迁移，得到默认的 4 个项目
        let path = std::env::temp_dir().join(format!("tt-test-{}.db", uuid::Uuid::new_v4()));
        let conn = db::open(&path).unwrap();
        let s = db::day_summary(&conn, None).unwrap();
        drop(conn);
        let _ = std::fs::remove_file(&path);
        s
    }

    #[test]
    fn questions_cover_activities() {
        let s = summary();
        let q = build_questions(&s);
        assert_eq!(q["decision"]["criteria"].as_object().unwrap().len(), 2);
        let cat = q["category"]["criteria"].as_object().unwrap();
        assert!(cat.contains_key("刷视频") && cat.contains_key("none"));
        let inst = q["instead"]["criteria"].as_object().unwrap();
        assert_eq!(inst.len(), 3); // 只有有益项
        let st = build_state("打一把游戏", DEFAULT_GOALS, DEFAULT_RULES, &s, &[]);
        assert_eq!(st["today_progress"].as_array().unwrap().len(), 4);
        assert_eq!(st["today_progress"][3]["daily_limit_minutes"], 60);
    }

    /// 真实调用 Jev（需要 TYPESAFE_API_KEY）：cargo test --lib live_jev -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_jev() {
        let key = api_key().expect("TYPESAFE_API_KEY not set");
        let s = summary();
        for q in ["打一把王者荣耀", "背 30 分钟单词", "去吃午饭"] {
            let st = build_state(q, DEFAULT_GOALS, DEFAULT_RULES, &s, &[]);
            let a = tauri::async_runtime::block_on(ask(&key, st, build_questions(&s))).unwrap();
            println!("{q} => {} (p_do={:?}, conf={:?}, category={:?}, instead={:?})", a.decision, a.p_do, a.confidence, a.category, a.instead);
        }
    }
}
