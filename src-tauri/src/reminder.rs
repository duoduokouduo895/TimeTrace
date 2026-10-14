//! 晚间复盘提醒：后台线程定时检查，到点且当天未复盘时
//! 恢复小组件窗口、播放提示音，并通知前端弹出提醒横幅。

use std::sync::Mutex;
use std::time::Duration;

use anyhow::anyhow;
use chrono::{Local, NaiveDate, NaiveTime, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::Db;
use crate::db::{self, DAY_START_HOUR};

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct ReminderConfig {
    pub enabled: bool,
    /// "HH:MM"
    pub time: String,
    pub sound: bool,
}

impl Default for ReminderConfig {
    fn default() -> Self {
        Self { enabled: true, time: "22:30".into(), sound: true }
    }
}

#[derive(Default)]
pub struct ReminderState {
    /// 已经提醒过的归属日，避免一天内重复提醒
    fired_day: Option<String>,
    snooze_until: Option<i64>,
    /// 前端是否应显示提醒横幅
    pending: bool,
}

pub struct Reminder(pub Mutex<ReminderState>);

pub fn config(conn: &rusqlite::Connection) -> ReminderConfig {
    db::get_settings(conn)
        .ok()
        .and_then(|s| s.get("reminder").cloned())
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// 归属日 day 中 HH:MM 对应的时间戳；早于换日时刻的算到次日凌晨
fn fire_time(day: &str, hhmm: &str) -> Option<i64> {
    let date = NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
    let t = NaiveTime::parse_from_str(hhmm, "%H:%M").ok()?;
    let date = if (t.hour() as i64) < DAY_START_HOUR { date.succ_opt()? } else { date };
    Local.from_local_datetime(&date.and_time(t)).earliest().map(|d| d.timestamp_millis())
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(15));
            if let Err(e) = tick(&app) {
                eprintln!("reminder: {e}");
            }
        }
    });
}

fn tick(app: &AppHandle) -> anyhow::Result<()> {
    let now = db::now_ms();
    let today = db::day_of(now);
    let (cfg, reviewed) = {
        let state = app.state::<Db>();
        let conn = state.0.lock().map_err(|e| anyhow!("{e}"))?;
        (config(&conn), db::has_review(&conn, &today)?)
    };

    let reminder = app.state::<Reminder>();
    let mut s = reminder.0.lock().map_err(|e| anyhow!("{e}"))?;
    if !cfg.enabled || reviewed {
        if s.pending {
            s.pending = false;
            let _ = app.emit("reminder", false);
        }
        return Ok(());
    }
    let due = match s.snooze_until {
        Some(t) => now >= t,
        None => {
            s.fired_day.as_deref() != Some(today.as_str())
                && fire_time(&today, &cfg.time).is_some_and(|t| now >= t)
        }
    };
    if !due {
        return Ok(());
    }
    s.fired_day = Some(today);
    s.snooze_until = None;
    s.pending = true;
    drop(s);
    fire(app, cfg.sound);
    Ok(())
}

fn fire(app: &AppHandle, sound: bool) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
    }
    if sound {
        play_chime();
    }
    let _ = app.emit("reminder", true);
}

pub fn pending(app: &AppHandle) -> bool {
    app.state::<Reminder>().0.lock().map(|s| s.pending).unwrap_or(false)
}

pub fn snooze(app: &AppHandle, minutes: i64) {
    if let Ok(mut s) = app.state::<Reminder>().0.lock() {
        s.snooze_until = Some(db::now_ms() + minutes * 60_000);
        s.pending = false;
    }
    let _ = app.emit("reminder", false);
}

pub fn dismiss(app: &AppHandle) {
    if let Ok(mut s) = app.state::<Reminder>().0.lock() {
        s.snooze_until = None;
        s.pending = false;
    }
    let _ = app.emit("reminder", false);
}

/// 修改提醒时间后允许当天按新时间再提醒一次
pub fn reset(app: &AppHandle) {
    if let Ok(mut s) = app.state::<Reminder>().0.lock() {
        s.fired_day = None;
        s.snooze_until = None;
    }
}

/// 立即触发一次（设置页「测试提醒」）
pub fn test(app: &AppHandle) {
    if let Ok(mut s) = app.state::<Reminder>().0.lock() {
        s.pending = true;
    }
    fire(app, true);
}

#[cfg(windows)]
pub fn play_chime() {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    static CHIME: &[u8] = include_bytes!("../sounds/review-chime.wav");
    // SND_MEMORY：直接播放内嵌的 WAV 数据；SND_ASYNC：不阻塞
    unsafe {
        PlaySoundW(CHIME.as_ptr().cast(), std::ptr::null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

#[cfg(not(windows))]
pub fn play_chime() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_time_respects_day_start() {
        let at = |d, h, m| Local.with_ymd_and_hms(2026, 10, d, h, m, 0).unwrap().timestamp_millis();
        assert_eq!(fire_time("2026-10-15", "22:30"), Some(at(15, 22, 30)));
        // 凌晨 1 点仍属于 15 日这一天 → 实际是 16 日 01:00
        assert_eq!(fire_time("2026-10-15", "01:00"), Some(at(16, 1, 0)));
        assert_eq!(fire_time("2026-10-15", "25:00"), None);
    }
}
