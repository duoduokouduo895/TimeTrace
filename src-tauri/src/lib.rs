mod db;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};

struct Db(Mutex<Connection>);

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
        .plugin(tauri_plugin_window_state::Builder::default().build())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
