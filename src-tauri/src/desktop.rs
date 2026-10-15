//! 仅桌面端：多窗口（复盘、决策弹窗）、置顶、全局快捷键、开机自启、Windows 资源管理器。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

use crate::{Db, DataDir, Res, db, err, with_db};

pub fn plugins(b: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    b.plugin(tauri_plugin_window_state::Builder::default().with_denylist(&["decide"]).build())
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
        .on_window_event(|window, event| {
            // 关掉小组件即退出（复盘窗口开着也一起关）
            if window.label() == "main" && matches!(event, WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
        })
}

/// 数据目录优先级：环境变量 TIMETRACE_DATA > 项目根目录下的 data/
pub fn data_dir(_app: &AppHandle) -> anyhow::Result<PathBuf> {
    if let Some(p) = std::env::var_os("TIMETRACE_DATA") {
        return Ok(PathBuf::from(p));
    }
    // 编译期确定的项目根目录（src-tauri 的上一级）
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("data"))
        .unwrap_or_else(|| PathBuf::from("data")))
}

/// 第一版把库放在 %APPDATA%，首次启动时拷贝过来（旧文件保留）
pub fn migrate_legacy_db(app: &AppHandle, db_path: &Path) -> anyhow::Result<()> {
    if !db_path.exists() {
        let old = app.path().app_data_dir()?.join("timetrace.db");
        if old.exists() {
            db::copy_db(&old, db_path)?;
        }
    }
    Ok(())
}

/// 数据库就绪后的桌面端初始化：置顶、决策弹窗、快捷键、提醒线程
pub fn setup(app: &AppHandle) -> anyhow::Result<()> {
    let settings = with_conn(app, db::get_settings)?;
    let pinned = settings.get("pinned").and_then(|v| v.as_bool()).unwrap_or(true);
    if let Some(w) = app.get_webview_window("main") {
        w.set_always_on_top(pinned)?;
    }
    crate::reminder::spawn(app.clone());
    create_decide_window(app)?;
    // 快捷键可能被其他程序占用：注册失败不影响启动，只在设置页提示
    let hk = settings
        .get("decideHotkey")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| DEFAULT_HOTKEY.to_string());
    let ok = register_hotkey(app, &hk);
    app.manage(Hotkey(Mutex::new((hk, ok))));
    Ok(())
}

fn with_conn<T>(app: &AppHandle, f: impl FnOnce(&rusqlite::Connection) -> anyhow::Result<T>) -> anyhow::Result<T> {
    let state = app.state::<Db>();
    let conn = state.0.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
    f(&conn)
}

#[tauri::command]
pub fn set_pinned(app: AppHandle, state: State<Db>, pinned: bool) -> Res<()> {
    if let Some(w) = app.get_webview_window("main") {
        w.set_always_on_top(pinned).map_err(err)?;
    }
    with_db(&state, |c| db::set_setting(c, "pinned", &serde_json::json!(pinned)))?;
    let _ = app.emit("settings-changed", ());
    Ok(())
}

#[tauri::command]
pub fn open_data_dir(dir: State<DataDir>) -> Res<()> {
    std::process::Command::new("explorer").arg(&dir.0).spawn().map_err(err)?;
    Ok(())
}

// 在命令里建窗口需要 async，否则 Windows 上会死锁
#[tauri::command]
pub async fn open_review(app: AppHandle) -> Res<()> {
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

// ---------- 决策弹窗 ----------

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

#[tauri::command]
pub async fn open_decide(app: AppHandle) {
    summon_decision(&app);
}

// ---------- 全局快捷键 ----------

const DEFAULT_HOTKEY: &str = "ctrl+alt+d";

/// (快捷键, 是否注册成功)
pub struct Hotkey(Mutex<(String, bool)>);

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
pub struct HotkeyStatus {
    hotkey: String,
    ok: bool,
}

#[tauri::command]
pub fn hotkey_status(hk: State<Hotkey>) -> Res<HotkeyStatus> {
    let g = hk.0.lock().map_err(err)?;
    Ok(HotkeyStatus { hotkey: g.0.clone(), ok: g.1 })
}

#[tauri::command]
pub fn set_hotkey(app: AppHandle, state: State<Db>, hk: State<Hotkey>, hotkey: String) -> Res<HotkeyStatus> {
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

// ---------- 开机自启 ----------

/// 开发版注册的是 debug exe 的路径，只允许在安装版里开启自启
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutostartInfo {
    enabled: bool,
    available: bool,
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Res<AutostartInfo> {
    Ok(AutostartInfo {
        enabled: app.autolaunch().is_enabled().map_err(err)?,
        available: !cfg!(debug_assertions),
    })
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Res<()> {
    if cfg!(debug_assertions) {
        return Err("开发版不支持开机自启，请在安装版里设置".into());
    }
    let al = app.autolaunch();
    if enabled { al.enable() } else { al.disable() }.map_err(err)
}
