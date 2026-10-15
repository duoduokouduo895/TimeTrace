mod db;
#[cfg(desktop)]
mod desktop;
mod jev;
#[cfg(mobile)]
mod mobile;
mod reminder;

// 平台差异集中在 desktop / mobile 两个模块，同名命令前端无需区分
#[cfg(desktop)]
use desktop as platform;
#[cfg(mobile)]
use mobile as platform;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager, State};

pub(crate) struct Db(pub(crate) Mutex<Connection>);

/// 数据目录：数据库 + 主题图片
pub(crate) struct DataDir(pub(crate) PathBuf);

pub(crate) type Res<T> = Result<T, String>;

pub(crate) fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// 在持有连接的情况下执行 f，失败转成字符串返回前端
pub(crate) fn with_db<T>(state: &State<Db>, f: impl FnOnce(&Connection) -> anyhow::Result<T>) -> Res<T> {
    let conn = state.0.lock().map_err(err)?;
    f(&conn).map_err(err)
}

/// 写操作后通知所有窗口刷新
fn mutate<T>(app: &AppHandle, state: &State<Db>, f: impl FnOnce(&Connection) -> anyhow::Result<T>) -> Res<T> {
    let out = with_db(state, f)?;
    let _ = app.emit("data-changed", ());
    Ok(out)
}

#[tauri::command]
fn day_summary(state: State<Db>, day: Option<String>) -> Res<db::DaySummary> {
    with_db(&state, |c| db::day_summary(c, day))
}

#[tauri::command]
fn month_overview(state: State<Db>, month: String) -> Res<Vec<db::DayStat>> {
    with_db(&state, |c| db::month_overview(c, &month))
}

#[tauri::command]
fn save_activity(app: AppHandle, state: State<Db>, activity: db::ActivityInput) -> Res<String> {
    mutate(&app, &state, |c| db::save_activity(c, activity))
}

#[tauri::command]
fn delete_activity(app: AppHandle, state: State<Db>, id: String) -> Res<()> {
    mutate(&app, &state, |c| db::delete_activity(c, &id))
}

#[tauri::command]
fn move_activity(app: AppHandle, state: State<Db>, id: String, dir: i64) -> Res<()> {
    mutate(&app, &state, |c| db::move_activity(c, &id, dir))
}

#[tauri::command]
fn start_timer(app: AppHandle, state: State<Db>, activity_id: String) -> Res<()> {
    mutate(&app, &state, |c| db::start_timer(c, &activity_id))
}

#[tauri::command]
fn stop_timer(app: AppHandle, state: State<Db>) -> Res<()> {
    mutate(&app, &state, db::stop_timer)
}

#[tauri::command]
fn add_entry(
    app: AppHandle,
    state: State<Db>,
    activity_id: String,
    seconds: i64,
    start_at: Option<i64>,
    day: Option<String>,
    note: Option<String>,
) -> Res<String> {
    mutate(&app, &state, |c| db::add_entry(c, &activity_id, seconds, start_at, day, note, "manual"))
}

#[tauri::command]
fn delete_entry(app: AppHandle, state: State<Db>, id: String) -> Res<()> {
    mutate(&app, &state, |c| db::delete_entry(c, &id))
}

#[tauri::command]
fn save_review(app: AppHandle, state: State<Db>, review: db::Review) -> Res<()> {
    mutate(&app, &state, |c| db::save_review(c, review))
}

// ---------- 设置 / 主题 ----------

#[tauri::command]
fn get_settings(state: State<Db>) -> Res<HashMap<String, serde_json::Value>> {
    with_db(&state, db::get_settings)
}

#[tauri::command]
fn set_setting(app: AppHandle, state: State<Db>, key: String, value: serde_json::Value) -> Res<()> {
    with_db(&state, |c| db::set_setting(c, &key, &value))?;
    if key == "reminder" {
        reminder::reset(&app);
    }
    let _ = app.emit("settings-changed", ());
    Ok(())
}

fn background_path(dir: &Path) -> PathBuf {
    dir.join("theme").join("background.jpg")
}

#[tauri::command]
fn get_background(dir: State<DataDir>) -> Res<Option<String>> {
    let path = background_path(&dir.0);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(err)?;
    Ok(Some(format!("data:image/jpeg;base64,{}", B64.encode(bytes))))
}

/// data_url：前端压缩好的 JPEG（data:image/jpeg;base64,...）
#[tauri::command]
fn set_background(app: AppHandle, dir: State<DataDir>, data_url: String) -> Res<()> {
    let b64 = data_url.split_once(",").map(|(_, b)| b).ok_or("图片数据格式不对")?;
    let bytes = B64.decode(b64).map_err(err)?;
    let path = background_path(&dir.0);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(err)?;
    std::fs::write(path, bytes).map_err(err)?;
    let _ = app.emit("settings-changed", ());
    Ok(())
}

#[tauri::command]
fn clear_background(app: AppHandle, dir: State<DataDir>) -> Res<()> {
    let path = background_path(&dir.0);
    if path.exists() {
        std::fs::remove_file(path).map_err(err)?;
    }
    let _ = app.emit("settings-changed", ());
    Ok(())
}

#[tauri::command]
fn data_dir(dir: State<DataDir>) -> String {
    dir.0.to_string_lossy().into_owned()
}

// ---------- 趋势 ----------

#[tauri::command]
fn range_stats(state: State<Db>, from: String, to: String) -> Res<db::RangeStats> {
    with_db(&state, |c| db::range_stats(c, &from, &to))
}

// ---------- 决策助手 ----------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionProfile {
    goals: String,
    rules: String,
    default_goals: &'static str,
    default_rules: &'static str,
    has_key: bool,
    /// "env" | "app" | null
    key_source: Option<&'static str>,
}

/// API Key 来源：电脑上优先读环境变量；手机上只能在 App 里填写（存于本地设置）
fn api_key_of(c: &Connection) -> Option<(String, &'static str)> {
    if let Some(k) = jev::api_key() {
        return Some((k, "env"));
    }
    db::get_settings(c)
        .ok()?
        .get("jevApiKey")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|k| (k.to_string(), "app"))
}

#[tauri::command]
fn set_api_key(app: AppHandle, state: State<Db>, key: String) -> Res<()> {
    with_db(&state, |c| db::set_setting(c, "jevApiKey", &serde_json::json!(key.trim())))?;
    let _ = app.emit("decisions-changed", ());
    Ok(())
}

fn profile_of(c: &Connection) -> anyhow::Result<(String, String)> {
    let p = db::get_settings(c)?.remove("decisionProfile").unwrap_or_default();
    let field = |k: &str, d: &str| {
        p.get(k).and_then(|v| v.as_str()).filter(|v| !v.trim().is_empty()).unwrap_or(d).to_string()
    };
    Ok((field("goals", jev::DEFAULT_GOALS), field("rules", jev::DEFAULT_RULES)))
}

#[tauri::command]
fn decision_profile(state: State<Db>) -> Res<DecisionProfile> {
    let (goals, rules, src) = with_db(&state, |c| {
        let (g, r) = profile_of(c)?;
        Ok((g, r, api_key_of(c).map(|(_, s)| s)))
    })?;
    Ok(DecisionProfile {
        goals,
        rules,
        default_goals: jev::DEFAULT_GOALS,
        default_rules: jev::DEFAULT_RULES,
        has_key: src.is_some(),
        key_source: src,
    })
}

#[tauri::command]
async fn decide(app: AppHandle, question: String) -> Res<jev::Verdict> {
    let question = question.trim().to_string();
    if question.is_empty() {
        return Err("先写下你想做什么".into());
    }
    let key = {
        let state = app.state::<Db>();
        let conn = state.0.lock().map_err(err)?;
        api_key_of(&conn).map(|(k, _)| k).ok_or("NO_KEY")?
    };
    let options = jev::parse_options(&question);
    let kind = if options.is_empty() { "yesno" } else { "choice" };
    // 取数据时持锁，网络请求前释放
    let (body_state, questions) = {
        let state = app.state::<Db>();
        let conn = state.0.lock().map_err(err)?;
        let (goals, rules) = profile_of(&conn).map_err(err)?;
        let summary = db::day_summary(&conn, None).map_err(err)?;
        let mut recent = db::list_decisions(&conn, Some(&summary.today), 8).map_err(err)?;
        recent.reverse();
        let st = jev::build_state(&question, &goals, &rules, &summary, &recent);
        (st, jev::build_questions(&options))
    };
    let ans = jev::ask(&key, body_state, questions).await.map_err(err)?;
    let d = {
        let state = app.state::<Db>();
        let conn = state.0.lock().map_err(err)?;
        db::insert_decision(&conn, &question, kind, &ans.choice, ans.probability, ans.confidence).map_err(err)?
    };
    let _ = app.emit("decisions-changed", ());
    Ok(jev::Verdict { id: d.id, kind: kind.into(), answer: ans.choice })
}

#[tauri::command]
fn list_decisions(state: State<Db>, day: Option<String>, limit: i64) -> Res<Vec<db::Decision>> {
    with_db(&state, |c| db::list_decisions(c, day.as_deref(), limit))
}

#[tauri::command]
fn set_decision_action(app: AppHandle, state: State<Db>, id: String, action: String) -> Res<()> {
    with_db(&state, |c| db::set_decision_action(c, &id, &action))?;
    let _ = app.emit("decisions-changed", ());
    Ok(())
}

// ---------- 提醒 / 自启 ----------

#[tauri::command]
fn reminder_pending(app: AppHandle) -> bool {
    reminder::pending(&app)
}

#[tauri::command]
fn snooze_reminder(app: AppHandle, minutes: i64) {
    reminder::snooze(&app, minutes);
}

#[tauri::command]
fn dismiss_reminder(app: AppHandle) {
    reminder::dismiss(&app);
}

#[tauri::command]
fn test_reminder(app: AppHandle) {
    reminder::test(&app);
}

#[tauri::command]
fn play_chime() {
    reminder::play_chime();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    platform::plugins(tauri::Builder::default())
        .setup(|app| {
            let handle = app.handle();
            let dir = platform::data_dir(handle)?;
            std::fs::create_dir_all(&dir)?;
            let db_path = dir.join("timetrace.db");
            platform::migrate_legacy_db(handle, &db_path)?;
            let conn = db::open(&db_path)?;
            app.manage(Db(Mutex::new(conn)));
            app.manage(DataDir(dir));
            app.manage(reminder::Reminder(Default::default()));
            platform::setup(handle)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            day_summary,
            month_overview,
            save_activity,
            delete_activity,
            move_activity,
            start_timer,
            stop_timer,
            add_entry,
            delete_entry,
            save_review,
            get_settings,
            set_setting,
            get_background,
            set_background,
            clear_background,
            data_dir,
            reminder_pending,
            snooze_reminder,
            dismiss_reminder,
            test_reminder,
            play_chime,
            range_stats,
            decision_profile,
            set_api_key,
            decide,
            list_decisions,
            set_decision_action,
            platform::set_pinned,
            platform::open_data_dir,
            platform::open_review,
            platform::open_decide,
            platform::hotkey_status,
            platform::set_hotkey,
            platform::get_autostart,
            platform::set_autostart,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
