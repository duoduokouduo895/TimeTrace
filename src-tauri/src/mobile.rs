//! 移动端（Android）：单窗口。与桌面同名的命令在这里给出等价实现或安全的默认值，
//! 前端因此无需区分平台调用；真正的差异在界面层处理。

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::{Db, DataDir, Res, db, with_db};

pub fn plugins(b: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    b
}

/// 手机上数据放在 App 私有目录（卸载 App 时一并删除）
pub fn data_dir(app: &AppHandle) -> anyhow::Result<PathBuf> {
    Ok(app.path().app_data_dir()?)
}

pub fn migrate_legacy_db(_app: &AppHandle, _db_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

pub fn setup(_app: &AppHandle) -> anyhow::Result<()> {
    Ok(())
}

#[tauri::command]
pub fn set_pinned(app: AppHandle, state: State<Db>, pinned: bool) -> Res<()> {
    with_db(&state, |c| db::set_setting(c, "pinned", &serde_json::json!(pinned)))?;
    let _ = app.emit("settings-changed", ());
    Ok(())
}

#[tauri::command]
pub fn open_data_dir(_dir: State<DataDir>) -> Res<()> {
    Err("手机上无法直接打开数据目录".into())
}

/// 手机上复盘页是 App 内的一个标签，不新开窗口
#[tauri::command]
pub async fn open_review(app: AppHandle) -> Res<()> {
    let _ = app.emit("navigate", "review");
    Ok(())
}

/// 手机上决策是 App 内的浮层
#[tauri::command]
pub async fn open_decide(app: AppHandle) {
    let _ = app.emit("open-decision", ());
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    hotkey: String,
    ok: bool,
}

#[tauri::command]
pub fn hotkey_status() -> Res<HotkeyStatus> {
    Ok(HotkeyStatus { hotkey: String::new(), ok: false })
}

#[tauri::command]
pub fn set_hotkey(_hotkey: String) -> Res<HotkeyStatus> {
    Err("手机上没有全局快捷键".into())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutostartInfo {
    enabled: bool,
    available: bool,
}

#[tauri::command]
pub fn get_autostart() -> Res<AutostartInfo> {
    Ok(AutostartInfo { enabled: false, available: false })
}

#[tauri::command]
pub fn set_autostart(_enabled: bool) -> Res<()> {
    Err("手机上不支持开机自启".into())
}
