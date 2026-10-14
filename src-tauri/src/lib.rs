mod db;
mod jev;
mod reminder;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

pub(crate) struct Db(pub(crate) Mutex<Connection>);

/// 数据目录：数据库 + 主题图片
struct DataDir(PathBuf);

type Res<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// 在持有连接的情况下执行 f，失败转成字符串返回前端
fn with_db<T>(state: &State<Db>, f: impl FnOnce(&Connection) -> anyhow::Result<T>) -> Res<T> {
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

#[tauri::command]
fn set_pinned(app: AppHandle, state: State<Db>, pinned: bool) -> Res<()> {
    if let Some(w) = app.get_webview_window("main") {
        w.set_always_on_top(pinned).map_err(err)?;
    }
    with_db(&state, |c| db::set_setting(c, "pinned", &serde_json::json!(pinned)))?;
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

#[tauri::command]
fn open_data_dir(dir: State<DataDir>) -> Res<()> {
    std::process::Command::new("explorer").arg(&dir.0).spawn().map_err(err)?;
    Ok(())
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
    let (goals, rules) = with_db(&state, profile_of)?;
    Ok(DecisionProfile {
        goals,
        rules,
        default_goals: jev::DEFAULT_GOALS,
        default_rules: jev::DEFAULT_RULES,
        has_key: jev::api_key().is_some(),
    })
}

#[tauri::command]
async fn decide(app: AppHandle, question: String) -> Res<jev::Verdict> {
    let question = question.trim().to_string();
    if question.is_empty() {
        return Err("先写下你想做什么".into());
    }
    let key = jev::api_key().ok_or("NO_KEY")?;
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

/// 屏幕中央的决策弹窗：启动时隐藏创建，唤出时只需显示，速度快
fn create_decide_window(app: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, "decide", WebviewUrl::App("index.html".into()))
        .title("TimeTrace · 做不做")
        .inner_size(460.0, 120.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .center()
        .build()?;
    Ok(())
}

/// 唤出决策弹窗（全局快捷键 / 小组件闪电按钮）
fn summon_decision(app: &AppHandle) {
    if app.get_webview_window("decide").is_none() {
        let _ = create_decide_window(app);
    }
    if let Some(w) = app.get_webview_window("decide") {
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit_to("decide", "open-decision", ());
}

const DEFAULT_HOTKEY: &str = "ctrl+alt+d";

/// (快捷键, 是否注册成功)
struct Hotkey(Mutex<(String, bool)>);

fn with_conn<T>(app: &AppHandle, f: impl FnOnce(&Connection) -> anyhow::Result<T>) -> anyhow::Result<T> {
    let state = app.state::<Db>();
    let conn = state.0.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
    f(&conn)
}

fn register_hotkey(app: &AppHandle, hk: &str) -> bool {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    match gs.register(hk) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("hotkey {hk}: {e}");
            false
        }
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct HotkeyStatus {
    hotkey: String,
    ok: bool,
}

#[tauri::command]
fn hotkey_status(hk: State<Hotkey>) -> Res<HotkeyStatus> {
    let g = hk.0.lock().map_err(err)?;
    Ok(HotkeyStatus { hotkey: g.0.clone(), ok: g.1 })
}

#[tauri::command]
fn set_hotkey(app: AppHandle, state: State<Db>, hk: State<Hotkey>, hotkey: String) -> Res<HotkeyStatus> {
    let mut g = hk.0.lock().map_err(err)?;
    if register_hotkey(&app, &hotkey) {
        with_db(&state, |c| db::set_setting(c, "decideHotkey", &serde_json::json!(hotkey)))?;
        *g = (hotkey, true);
        Ok(HotkeyStatus { hotkey: g.0.clone(), ok: true })
    } else {
        // 新的也被占用：恢复原来的
        g.1 = register_hotkey(&app, &g.0);
        Err(format!("{hotkey} 已被其他程序占用，换一个试试"))
    }
}

#[tauri::command]
async fn open_decide(app: AppHandle) {
    summon_decision(&app);
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

/// 开发版注册的是 debug exe 的路径，只允许在安装版里开启自启
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AutostartInfo {
    enabled: bool,
    available: bool,
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> Res<AutostartInfo> {
    Ok(AutostartInfo {
        enabled: app.autolaunch().is_enabled().map_err(err)?,
        available: !cfg!(debug_assertions),
    })
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Res<()> {
    if cfg!(debug_assertions) {
        return Err("开发版不支持开机自启，请在安装版里设置".into());
    }
    let al = app.autolaunch();
    if enabled { al.enable() } else { al.disable() }.map_err(err)
}

// 在命令里建窗口需要 async，否则 Windows 上会死锁
#[tauri::command]
async fn open_review(app: AppHandle) -> Res<()> {
    if let Some(w) = app.get_webview_window("review") {
        let _ = w.unminimize();
        w.show().map_err(err)?;
        w.set_focus().map_err(err)?;
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "review", WebviewUrl::App("index.html".into()))
        .title("TimeTrace · 晚间复盘")
        .inner_size(1080.0, 800.0)
        .min_inner_size(760.0, 520.0)
        .build()
        .map_err(err)?;
    Ok(())
}

/// 数据目录优先级：环境变量 TIMETRACE_DATA > 项目根目录下的 data/
fn resolve_data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("TIMETRACE_DATA") {
        return PathBuf::from(p);
    }
    // 编译期确定的项目根目录（src-tauri 的上一级）
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("data"))
        .unwrap_or_else(|| PathBuf::from("data"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::default().with_denylist(&["decide"]).build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        summon_decision(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let dir = resolve_data_dir();
            std::fs::create_dir_all(&dir)?;
            let db_path = dir.join("timetrace.db");
            // 第一版把库放在 %APPDATA%，首次启动时拷贝过来（旧文件保留）
            if !db_path.exists() {
                let old = app.path().app_data_dir()?.join("timetrace.db");
                if old.exists() {
                    db::copy_db(&old, &db_path)?;
                }
            }
            let conn = db::open(&db_path)?;
            let pinned = db::get_settings(&conn)?
                .get("pinned")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if let Some(w) = app.get_webview_window("main") {
                w.set_always_on_top(pinned)?;
            }
            app.manage(Db(Mutex::new(conn)));
            app.manage(DataDir(dir));
            app.manage(reminder::Reminder(Default::default()));
            reminder::spawn(app.handle().clone());
            create_decide_window(app.handle())?;
            // 快捷键可能被其他程序占用：注册失败不影响启动，只在设置页提示
            let hk = with_conn(app.handle(), |c| {
                Ok(db::get_settings(c)?.get("decideHotkey").and_then(|v| v.as_str().map(String::from)))
            })
            .ok()
            .flatten()
            .unwrap_or_else(|| DEFAULT_HOTKEY.to_string());
            let ok = register_hotkey(app.handle(), &hk);
            app.manage(Hotkey(Mutex::new((hk, ok))));
            Ok(())
        })
        .on_window_event(|window, event| {
            // 关掉小组件即退出（复盘窗口开着也一起关）
            if window.label() == "main" && matches!(event, WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
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
            set_pinned,
            get_background,
            set_background,
            clear_background,
            data_dir,
            open_data_dir,
            open_review,
            reminder_pending,
            snooze_reminder,
            dismiss_reminder,
            test_reminder,
            play_chime,
            get_autostart,
            set_autostart,
            range_stats,
            decision_profile,
            decide,
            open_decide,
            hotkey_status,
            set_hotkey,
            list_decisions,
            set_decision_action,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
