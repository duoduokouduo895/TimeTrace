//! SQLite 数据层。桌面端与后续 `tt` CLI 共用。
//! 所有记录用 UUID + updated_at + 软删除，为日后同步预留。

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{Result, bail};
use chrono::{Local, NaiveDate, TimeDelta, TimeZone};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// 一天从凌晨 4 点开始算，熬夜到 1 点仍归属前一天。
pub const DAY_START_HOUR: i64 = 4;
/// 计时短于该秒数视为误触，不落库。
const MIN_TIMER_SECONDS: i64 = 30;

pub fn now_ms() -> i64 {
    Local::now().timestamp_millis()
}

pub fn day_of(ts_ms: i64) -> String {
    let t = Local.timestamp_millis_opt(ts_ms).single().unwrap_or_else(Local::now);
    (t - TimeDelta::hours(DAY_START_HOUR)).format("%Y-%m-%d").to_string()
}

fn shift_day(day: &str, delta: i64) -> String {
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map(|d| (d + TimeDelta::days(delta)).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|_| day.to_string())
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub id: String,
    pub name: String,
    pub color: String,
    /// "gain" 有益（daily_minutes 为目标）| "drain" 消耗（daily_minutes 为上限）
    pub kind: String,
    pub daily_minutes: i64,
    pub sort: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityInput {
    pub id: Option<String>,
    pub name: String,
    pub color: String,
    pub kind: String,
    pub daily_minutes: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub activity_id: String,
    pub day: String,
    pub start_at: Option<i64>,
    pub end_at: Option<i64>,
    pub seconds: i64,
    /// "timer" | "manual" | "cli"
    pub source: String,
    pub note: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Running {
    pub activity_id: String,
    pub start_at: i64,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub day: String,
    pub best: String,
    pub worst: String,
    pub tomorrow: String,
    pub score: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySummary {
    pub day: String,
    pub today: String,
    pub day_start_hour: i64,
    pub activities: Vec<Activity>,
    pub entries: Vec<Entry>,
    /// activity_id -> 当日秒数（不含正在计时的部分）
    pub totals: HashMap<String, i64>,
    pub yesterday: HashMap<String, i64>,
    /// 前 7 天日均秒数
    pub avg7: HashMap<String, i64>,
    pub running: Option<Running>,
    pub review: Review,
}

const SCHEMA_V1: &str = "
CREATE TABLE activity (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  color TEXT NOT NULL,
  kind TEXT NOT NULL,
  daily_minutes INTEGER NOT NULL DEFAULT 0,
  sort INTEGER NOT NULL DEFAULT 0,
  updated_at INTEGER NOT NULL,
  deleted INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE entry (
  id TEXT PRIMARY KEY,
  activity_id TEXT NOT NULL,
  day TEXT NOT NULL,
  start_at INTEGER,
  end_at INTEGER,
  seconds INTEGER NOT NULL,
  source TEXT NOT NULL,
  note TEXT,
  updated_at INTEGER NOT NULL,
  deleted INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_entry_day ON entry(day);
CREATE TABLE review (
  day TEXT PRIMARY KEY,
  best TEXT NOT NULL DEFAULT '',
  worst TEXT NOT NULL DEFAULT '',
  tomorrow TEXT NOT NULL DEFAULT '',
  score INTEGER,
  updated_at INTEGER NOT NULL
);
CREATE TABLE running (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  activity_id TEXT NOT NULL,
  start_at INTEGER NOT NULL
);
";

pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
    conn.busy_timeout(Duration::from_secs(3))?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 1 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA_V1)?;
        let now = now_ms();
        let seed = [
            ("背单词", "#4f8cff", "gain", 60),
            ("英语跟读", "#22b07d", "gain", 45),
            ("面试学习", "#f0a020", "gain", 120),
            ("刷视频", "#e5484d", "drain", 60),
        ];
        for (i, (name, color, kind, minutes)) in seed.iter().enumerate() {
            tx.execute(
                "INSERT INTO activity(id,name,color,kind,daily_minutes,sort,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![new_id(), name, color, kind, minutes, i as i64, now],
            )?;
        }
        tx.pragma_update(None, "user_version", 1)?;
        tx.commit()?;
    }
    if version < 2 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(
            "CREATE TABLE setting (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER NOT NULL);",
        )?;
        tx.pragma_update(None, "user_version", 2)?;
        tx.commit()?;
    }
    Ok(())
}

/// 把旧位置的数据库完整拷贝到新位置（旧文件保留不动）
pub fn copy_db(from: &Path, to: &Path) -> Result<()> {
    let old = Connection::open(from)?;
    old.execute("VACUUM INTO ?1", params![to.to_string_lossy()])?;
    Ok(())
}

// ---------- setting ----------

/// 设置项以 JSON 文本存储，key -> JSON 值
pub fn get_settings(conn: &Connection) -> Result<HashMap<String, serde_json::Value>> {
    let mut st = conn.prepare("SELECT key, value FROM setting")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = HashMap::new();
    for row in rows {
        let (k, v) = row?;
        if let Ok(v) = serde_json::from_str(&v) {
            out.insert(k, v);
        }
    }
    Ok(out)
}

pub fn set_setting(conn: &Connection, key: &str, value: &serde_json::Value) -> Result<()> {
    conn.execute(
        "INSERT INTO setting(key,value,updated_at) VALUES(?1,?2,?3)
         ON CONFLICT(key) DO UPDATE SET value=?2, updated_at=?3",
        params![key, value.to_string(), now_ms()],
    )?;
    Ok(())
}

// ---------- activity ----------

pub fn list_activities(conn: &Connection) -> Result<Vec<Activity>> {
    let mut st = conn.prepare(
        "SELECT id,name,color,kind,daily_minutes,sort FROM activity WHERE deleted=0 ORDER BY sort, name",
    )?;
    let rows = st.query_map([], |r| {
        Ok(Activity {
            id: r.get(0)?,
            name: r.get(1)?,
            color: r.get(2)?,
            kind: r.get(3)?,
            daily_minutes: r.get(4)?,
            sort: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn save_activity(conn: &Connection, a: ActivityInput) -> Result<String> {
    let name = a.name.trim();
    if name.is_empty() {
        bail!("名称不能为空");
    }
    if a.kind != "gain" && a.kind != "drain" {
        bail!("未知类型: {}", a.kind);
    }
    let dup: Option<String> = conn
        .query_row(
            "SELECT id FROM activity WHERE deleted=0 AND name=?1 AND id<>COALESCE(?2,'')",
            params![name, a.id],
            |r| r.get(0),
        )
        .optional()?;
    if dup.is_some() {
        bail!("已存在同名项目: {name}");
    }
    let minutes = a.daily_minutes.max(0);
    let now = now_ms();
    match a.id {
        Some(id) => {
            conn.execute(
                "UPDATE activity SET name=?2,color=?3,kind=?4,daily_minutes=?5,updated_at=?6 WHERE id=?1",
                params![id, name, a.color, a.kind, minutes, now],
            )?;
            Ok(id)
        }
        None => {
            let id = new_id();
            let sort: i64 =
                conn.query_row("SELECT COALESCE(MAX(sort),-1)+1 FROM activity", [], |r| r.get(0))?;
            conn.execute(
                "INSERT INTO activity(id,name,color,kind,daily_minutes,sort,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![id, name, a.color, a.kind, minutes, sort, now],
            )?;
            Ok(id)
        }
    }
}

pub fn delete_activity(conn: &Connection, id: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE activity SET deleted=1,updated_at=?2 WHERE id=?1",
        params![id, now_ms()],
    )?;
    tx.execute("DELETE FROM running WHERE activity_id=?1", params![id])?;
    tx.commit()?;
    Ok(())
}

/// dir = -1 上移，1 下移
pub fn move_activity(conn: &Connection, id: &str, dir: i64) -> Result<()> {
    let mut list = list_activities(conn)?;
    let Some(i) = list.iter().position(|a| a.id == id) else {
        return Ok(());
    };
    let j = i as i64 + dir;
    if j < 0 || j >= list.len() as i64 {
        return Ok(());
    }
    list.swap(i, j as usize);
    let tx = conn.unchecked_transaction()?;
    let now = now_ms();
    for (k, a) in list.iter().enumerate() {
        tx.execute(
            "UPDATE activity SET sort=?2,updated_at=?3 WHERE id=?1 AND sort<>?2",
            params![a.id, k as i64, now],
        )?;
    }
    tx.commit()?;
    Ok(())
}

// ---------- timer ----------

pub fn running(conn: &Connection) -> Result<Option<Running>> {
    Ok(conn
        .query_row("SELECT activity_id,start_at FROM running WHERE id=1", [], |r| {
            Ok(Running { activity_id: r.get(0)?, start_at: r.get(1)? })
        })
        .optional()?)
}

pub fn start_timer(conn: &Connection, activity_id: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    if let Some(r) = running(&tx)? {
        if r.activity_id == activity_id {
            return Ok(());
        }
    }
    stop_inner(&tx)?;
    tx.execute(
        "INSERT INTO running(id,activity_id,start_at) VALUES(1,?1,?2)",
        params![activity_id, now_ms()],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn stop_timer(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    stop_inner(&tx)?;
    tx.commit()?;
    Ok(())
}

fn stop_inner(conn: &Connection) -> Result<()> {
    let Some(r) = running(conn)? else {
        return Ok(());
    };
    conn.execute("DELETE FROM running WHERE id=1", [])?;
    let end = now_ms();
    let seconds = (end - r.start_at) / 1000;
    if seconds >= MIN_TIMER_SECONDS {
        insert_entry(conn, &r.activity_id, &day_of(r.start_at), Some(r.start_at), Some(end), seconds, "timer", None)?;
    }
    Ok(())
}

// ---------- entry ----------

#[allow(clippy::too_many_arguments)]
fn insert_entry(
    conn: &Connection,
    activity_id: &str,
    day: &str,
    start_at: Option<i64>,
    end_at: Option<i64>,
    seconds: i64,
    source: &str,
    note: Option<&str>,
) -> Result<String> {
    let id = new_id();
    conn.execute(
        "INSERT INTO entry(id,activity_id,day,start_at,end_at,seconds,source,note,updated_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![id, activity_id, day, start_at, end_at, seconds, source, note, now_ms()],
    )?;
    Ok(id)
}

/// 手动/外部录入。
/// - 给了 start_at：按开始时间定位；
/// - 没给 start_at 且 day 是今天（或未指定）：视为「刚结束」，end = 现在；
/// - 没给 start_at 且 day 是过去某天：只记时长，不在时间轴上定位。
pub fn add_entry(
    conn: &Connection,
    activity_id: &str,
    seconds: i64,
    start_at: Option<i64>,
    day: Option<String>,
    note: Option<String>,
    source: &str,
) -> Result<String> {
    if seconds <= 0 || seconds > 24 * 3600 {
        bail!("时长需在 0~24 小时之间");
    }
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM activity WHERE id=?1 AND deleted=0)",
        params![activity_id],
        |r| r.get(0),
    )?;
    if !exists {
        bail!("项目不存在");
    }
    let now = now_ms();
    let today = day_of(now);
    let (start, end, day) = match (start_at, day) {
        (Some(s), _) => (Some(s), Some(s + seconds * 1000), day_of(s)),
        (None, Some(d)) if d != today => (None, None, d),
        _ => (Some(now - seconds * 1000), Some(now), today),
    };
    let note = note.as_deref().map(str::trim).filter(|s| !s.is_empty());
    insert_entry(conn, activity_id, &day, start, end, seconds, source, note)
}

pub fn delete_entry(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE entry SET deleted=1,updated_at=?2 WHERE id=?1",
        params![id, now_ms()],
    )?;
    Ok(())
}

fn entries_for_day(conn: &Connection, day: &str) -> Result<Vec<Entry>> {
    let mut st = conn.prepare(
        "SELECT id,activity_id,day,start_at,end_at,seconds,source,note FROM entry
         WHERE deleted=0 AND day=?1 ORDER BY start_at IS NULL, start_at",
    )?;
    let rows = st.query_map(params![day], |r| {
        Ok(Entry {
            id: r.get(0)?,
            activity_id: r.get(1)?,
            day: r.get(2)?,
            start_at: r.get(3)?,
            end_at: r.get(4)?,
            seconds: r.get(5)?,
            source: r.get(6)?,
            note: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// [from, to] 闭区间内每个项目的总秒数
fn totals(conn: &Connection, from: &str, to: &str) -> Result<HashMap<String, i64>> {
    let mut st = conn.prepare(
        "SELECT activity_id, SUM(seconds) FROM entry
         WHERE deleted=0 AND day BETWEEN ?1 AND ?2 GROUP BY activity_id",
    )?;
    let rows = st.query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

// ---------- review ----------

fn get_review(conn: &Connection, day: &str) -> Result<Review> {
    Ok(conn
        .query_row(
            "SELECT day,best,worst,tomorrow,score FROM review WHERE day=?1",
            params![day],
            |r| {
                Ok(Review {
                    day: r.get(0)?,
                    best: r.get(1)?,
                    worst: r.get(2)?,
                    tomorrow: r.get(3)?,
                    score: r.get(4)?,
                })
            },
        )
        .optional()?
        .unwrap_or_else(|| Review { day: day.to_string(), ..Default::default() }))
}

/// 该日是否已写过复盘（任一字段非空即算）
pub fn has_review(conn: &Connection, day: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM review WHERE day=?1
         AND (best<>'' OR worst<>'' OR tomorrow<>'' OR score IS NOT NULL))",
        params![day],
        |r| r.get(0),
    )?)
}

pub fn save_review(conn: &Connection, r: Review) -> Result<()> {
    conn.execute(
        "INSERT INTO review(day,best,worst,tomorrow,score,updated_at) VALUES(?1,?2,?3,?4,?5,?6)
         ON CONFLICT(day) DO UPDATE SET best=?2,worst=?3,tomorrow=?4,score=?5,updated_at=?6",
        params![r.day, r.best, r.worst, r.tomorrow, r.score, now_ms()],
    )?;
    Ok(())
}

// ---------- month ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayStat {
    pub day: String,
    pub gain: i64,
    pub drain: i64,
    pub reviewed: bool,
}

/// 某月（"YYYY-MM"）每天的有益/消耗秒数及是否写了复盘，供日历使用
pub fn month_overview(conn: &Connection, month: &str) -> Result<Vec<DayStat>> {
    let pattern = format!("{month}-%");
    let mut map: std::collections::BTreeMap<String, DayStat> = Default::default();
    let mut st = conn.prepare(
        "SELECT e.day,
                SUM(CASE WHEN a.kind='gain' THEN e.seconds ELSE 0 END),
                SUM(CASE WHEN a.kind='drain' THEN e.seconds ELSE 0 END)
         FROM entry e JOIN activity a ON a.id = e.activity_id
         WHERE e.deleted=0 AND e.day LIKE ?1 GROUP BY e.day",
    )?;
    for row in st.query_map(params![pattern], |r| Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?)))? {
        let (day, gain, drain) = row?;
        map.insert(day.clone(), DayStat { day, gain, drain, reviewed: false });
    }
    let mut st = conn.prepare(
        "SELECT day FROM review WHERE day LIKE ?1
         AND (best<>'' OR worst<>'' OR tomorrow<>'' OR score IS NOT NULL)",
    )?;
    for day in st.query_map(params![pattern], |r| r.get::<_, String>(0))? {
        let day = day?;
        map.entry(day.clone())
            .or_insert(DayStat { day, gain: 0, drain: 0, reviewed: false })
            .reviewed = true;
    }
    Ok(map.into_values().collect())
}

// ---------- summary ----------

pub fn day_summary(conn: &Connection, day: Option<String>) -> Result<DaySummary> {
    let today = day_of(now_ms());
    let day = day.unwrap_or_else(|| today.clone());
    let yday = shift_day(&day, -1);
    let avg7 = totals(conn, &shift_day(&day, -7), &yday)?
        .into_iter()
        .map(|(k, v)| (k, v / 7))
        .collect();
    Ok(DaySummary {
        activities: list_activities(conn)?,
        entries: entries_for_day(conn, &day)?,
        totals: totals(conn, &day, &day)?,
        yesterday: totals(conn, &yday, &yday)?,
        avg7,
        running: running(conn)?,
        review: get_review(conn, &day)?,
        day_start_hour: DAY_START_HOUR,
        day,
        today,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn
    }

    fn id_of(conn: &Connection, name: &str) -> String {
        list_activities(conn).unwrap().into_iter().find(|a| a.name == name).unwrap().id
    }

    #[test]
    fn seeds_default_activities_once() {
        let conn = mem();
        migrate(&conn).unwrap();
        let names: Vec<_> = list_activities(&conn).unwrap().into_iter().map(|a| a.name).collect();
        assert_eq!(names, ["背单词", "英语跟读", "面试学习", "刷视频"]);
    }

    #[test]
    fn day_boundary_is_4am() {
        let at = |h| Local.with_ymd_and_hms(2026, 10, 15, h, 30, 0).unwrap().timestamp_millis();
        assert_eq!(day_of(at(3)), "2026-10-14");
        assert_eq!(day_of(at(4)), "2026-10-15");
    }

    #[test]
    fn manual_entries_and_totals() {
        let conn = mem();
        let a = id_of(&conn, "英语跟读");
        add_entry(&conn, &a, 25 * 60, None, None, Some(" Englishpob ".into()), "manual").unwrap();
        add_entry(&conn, &a, 10 * 60, None, Some("2026-01-01".into()), None, "manual").unwrap();
        let s = day_summary(&conn, None).unwrap();
        assert_eq!(s.totals[&a], 25 * 60);
        assert_eq!(s.entries[0].note.as_deref(), Some("Englishpob"));
        assert!(s.entries[0].start_at.is_some());
        // 过去某天且无开始时间 → 未定位
        let past = day_summary(&conn, Some("2026-01-01".into())).unwrap();
        assert_eq!(past.entries[0].start_at, None);
        assert_eq!(past.totals[&a], 600);
        assert!(add_entry(&conn, &a, 0, None, None, None, "manual").is_err());
    }

    #[test]
    fn timer_switch_and_short_sessions_discarded() {
        let conn = mem();
        let a = id_of(&conn, "背单词");
        let b = id_of(&conn, "刷视频");
        start_timer(&conn, &a).unwrap();
        // 伪造 10 分钟前开始
        conn.execute("UPDATE running SET start_at=start_at-600000", []).unwrap();
        start_timer(&conn, &b).unwrap(); // 切换：a 落库
        assert_eq!(running(&conn).unwrap().unwrap().activity_id, b);
        stop_timer(&conn).unwrap(); // b 不足 30 秒，丢弃
        assert!(running(&conn).unwrap().is_none());
        let s = day_summary(&conn, None).unwrap();
        assert_eq!(s.entries.len(), 1);
        assert_eq!(s.totals[&a], 600);
    }

    #[test]
    fn review_upsert_and_activity_rules() {
        let conn = mem();
        save_review(&conn, Review { day: "2026-10-15".into(), best: "跟读".into(), ..Default::default() }).unwrap();
        save_review(&conn, Review { day: "2026-10-15".into(), best: "面试".into(), score: Some(4), ..Default::default() }).unwrap();
        let got = get_review(&conn, "2026-10-15").unwrap();
        assert_eq!((got.best.as_str(), got.score), ("面试", Some(4)));

        let dup = ActivityInput { id: None, name: "背单词".into(), color: "#fff".into(), kind: "gain".into(), daily_minutes: 10 };
        assert!(save_activity(&conn, dup).is_err());
        let v = id_of(&conn, "刷视频");
        start_timer(&conn, &v).unwrap();
        delete_activity(&conn, &v).unwrap();
        assert!(running(&conn).unwrap().is_none());
        move_activity(&conn, &id_of(&conn, "面试学习"), -1).unwrap();
        let names: Vec<_> = list_activities(&conn).unwrap().into_iter().map(|a| a.name).collect();
        assert_eq!(names, ["背单词", "面试学习", "英语跟读"]);
    }

    #[test]
    fn update_limit_settings_and_month() {
        let conn = mem();
        let v = id_of(&conn, "刷视频");
        let upd = ActivityInput { id: Some(v.clone()), name: "刷视频".into(), color: "#e5484d".into(), kind: "drain".into(), daily_minutes: 30 };
        save_activity(&conn, upd).unwrap();
        assert_eq!(list_activities(&conn).unwrap().iter().find(|a| a.id == v).unwrap().daily_minutes, 30);

        set_setting(&conn, "widgetScale", &serde_json::json!(1.2)).unwrap();
        set_setting(&conn, "widgetScale", &serde_json::json!(1.1)).unwrap();
        assert_eq!(get_settings(&conn).unwrap()["widgetScale"], serde_json::json!(1.1));

        let g = id_of(&conn, "背单词");
        add_entry(&conn, &g, 1200, None, Some("2026-01-03".into()), None, "manual").unwrap();
        add_entry(&conn, &v, 600, None, Some("2026-01-03".into()), None, "manual").unwrap();
        save_review(&conn, Review { day: "2026-01-05".into(), score: Some(3), ..Default::default() }).unwrap();
        let m = month_overview(&conn, "2026-01").unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].gain, m[0].drain, m[0].reviewed), (1200, 600, false));
        assert_eq!((m[1].day.as_str(), m[1].reviewed), ("2026-01-05", true));
    }
}
