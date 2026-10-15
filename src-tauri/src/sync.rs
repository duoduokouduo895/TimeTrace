//! 多设备同步（坚果云等 WebDAV）。
//!
//! 远端目录 `TimeTrace/devices/`：每台设备只写自己的 `<设备ID>.json`（完整快照），
//! 读取其他设备的文件后逐条合并：同一主键取 updated_at 较新者（删除是软删除标记，同样随之同步）。
//! 两台设备首次启动各自生成的同名默认项目，合并后按名称归并为 ID 较小的那个。
//!
//! 不同步：正在进行的计时、API Key、同步账号、背景图与设备专属设置。

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{Connection, params_from_iter};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tauri::{AppHandle, Emitter, Manager};

use crate::Db;
use crate::db::{self, now_ms};

const FORMAT: i64 = 1;
const REMOTE_DIR: &str = "TimeTrace";

/// 参与同步的表：(表名, 主键, 列)
const TABLES: [(&str, &str, &[&str]); 4] = [
    ("activity", "id", &["id", "name", "color", "kind", "daily_minutes", "sort", "updated_at", "deleted"]),
    (
        "entry",
        "id",
        &["id", "activity_id", "day", "start_at", "end_at", "seconds", "source", "note", "updated_at", "deleted"],
    ),
    ("review", "day", &["day", "best", "worst", "tomorrow", "score", "updated_at"]),
    (
        "decision",
        "id",
        &["id", "asked_at", "day", "question", "kind", "decision", "probability", "confidence", "action", "updated_at"],
    ),
];

/// 参与同步的设置项
const SYNC_SETTINGS: [&str; 1] = ["decisionProfile"];

// ---------- 快照：导出与合并 ----------

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub format: i64,
    pub device_id: String,
    pub device_name: String,
    // BTreeMap：序列化顺序固定，内容不变时指纹不变，避免重复上传
    pub tables: BTreeMap<String, Vec<Map<String, Value>>>,
    /// key -> { value, updatedAt }
    pub settings: BTreeMap<String, Value>,
}

fn to_json(v: ValueRef) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
        ValueRef::Blob(_) => Value::Null,
    }
}

fn to_sql(v: &Value) -> SqlValue {
    match v {
        Value::Null => SqlValue::Null,
        Value::Bool(b) => SqlValue::Integer(*b as i64),
        Value::Number(n) => n.as_i64().map(SqlValue::Integer).unwrap_or_else(|| SqlValue::Real(n.as_f64().unwrap_or(0.0))),
        Value::String(s) => SqlValue::Text(s.clone()),
        other => SqlValue::Text(other.to_string()),
    }
}

pub fn export(conn: &Connection, device_id: &str, device_name: &str) -> Result<Snapshot> {
    let mut tables = BTreeMap::new();
    for (table, _, cols) in TABLES {
        let mut st = conn.prepare(&format!("SELECT {} FROM {table} ORDER BY 1", cols.join(",")))?;
        let rows = st
            .query_map([], |r| {
                let mut m = Map::new();
                for (i, c) in cols.iter().enumerate() {
                    m.insert((*c).to_string(), to_json(r.get_ref(i)?));
                }
                Ok(m)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        tables.insert(table.to_string(), rows);
    }
    let mut settings = BTreeMap::new();
    for key in SYNC_SETTINGS {
        let row: Option<(String, i64)> = conn
            .query_row("SELECT value, updated_at FROM setting WHERE key=?1", [key], |r| Ok((r.get(0)?, r.get(1)?)))
            .ok();
        if let Some((v, at)) = row {
            settings.insert(key.to_string(), json!({ "value": serde_json::from_str::<Value>(&v)?, "updatedAt": at }));
        }
    }
    Ok(Snapshot {
        format: FORMAT,
        device_id: device_id.to_string(),
        device_name: device_name.to_string(),
        tables,
        settings,
    })
}

/// 把另一台设备的快照合并进本地，返回写入（新增或更新）的行数
pub fn merge(conn: &Connection, snap: &Snapshot) -> Result<usize> {
    if snap.format > FORMAT {
        bail!("对方设备的 TimeTrace 版本较新，请先升级本机");
    }
    let tx = conn.unchecked_transaction()?;
    let mut changed = 0;
    for (table, pk, cols) in TABLES {
        let Some(rows) = snap.tables.get(table) else { continue };
        let placeholders = vec!["?"; cols.len()].join(",");
        let upsert = format!("INSERT OR REPLACE INTO {table}({}) VALUES({placeholders})", cols.join(","));
        let local_at = format!("SELECT updated_at FROM {table} WHERE {pk}=?1");
        for row in rows {
            let Some(key) = row.get(pk).and_then(|v| v.as_str()) else { continue };
            let incoming = row.get("updated_at").and_then(|v| v.as_i64()).unwrap_or(0);
            let local: Option<i64> = tx.query_row(&local_at, [key], |r| r.get(0)).ok();
            if local.is_some_and(|l| l >= incoming) {
                continue;
            }
            let values: Vec<SqlValue> = cols.iter().map(|c| to_sql(row.get(*c).unwrap_or(&Value::Null))).collect();
            tx.execute(&upsert, params_from_iter(values))?;
            changed += 1;
        }
    }
    for key in SYNC_SETTINGS {
        let Some(s) = snap.settings.get(key) else { continue };
        let incoming = s.get("updatedAt").and_then(|v| v.as_i64()).unwrap_or(0);
        let local: Option<i64> =
            tx.query_row("SELECT updated_at FROM setting WHERE key=?1", [key], |r| r.get(0)).ok();
        if local.is_some_and(|l| l >= incoming) {
            continue;
        }
        let value = s.get("value").cloned().unwrap_or(Value::Null);
        tx.execute(
            "INSERT OR REPLACE INTO setting(key, value, updated_at) VALUES(?1, ?2, ?3)",
            rusqlite::params![key, value.to_string(), incoming],
        )?;
        changed += 1;
    }
    tx.commit()?;
    Ok(changed)
}

/// 同名的未删除项目归并为 ID 最小的那个：记录与计时改挂过去，其余标记删除。
/// 各设备按同样规则处理，结果一致。返回归并掉的项目数。
pub fn dedupe_activities(conn: &Connection) -> Result<usize> {
    let mut st = conn.prepare("SELECT name, id FROM activity WHERE deleted=0 ORDER BY name, id")?;
    let rows = st
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);
    let tx = conn.unchecked_transaction()?;
    let now = now_ms();
    let mut merged = 0;
    let mut keep: Option<(String, String)> = None;
    for (name, id) in rows {
        match &keep {
            Some((n, k)) if *n == name => {
                tx.execute("UPDATE entry SET activity_id=?1, updated_at=?3 WHERE activity_id=?2", rusqlite::params![k, id, now])?;
                tx.execute("UPDATE running SET activity_id=?1 WHERE activity_id=?2", rusqlite::params![k, id])?;
                tx.execute("UPDATE activity SET deleted=1, updated_at=?2 WHERE id=?1", rusqlite::params![id, now])?;
                merged += 1;
            }
            _ => keep = Some((name, id)),
        }
    }
    tx.commit()?;
    Ok(merged)
}

// ---------- 配置与状态（存在本地 setting 表，不参与同步） ----------

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncConfig {
    pub enabled: bool,
    pub url: String,
    pub user: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncState {
    pub last_sync_at: Option<i64>,
    /// "ok" | "error"
    pub last_result: Option<String>,
    pub last_message: Option<String>,
    /// 上次上传内容的指纹，内容不变就不上传
    pub upload_hash: Option<String>,
    /// 其他设备文件名 -> ETag，未变化就不下载
    pub etags: HashMap<String, String>,
    pub dirs_ready: bool,
    /// 其他设备：文件名 -> 设备名
    pub peers: HashMap<String, String>,
}

fn get_json<T: for<'de> Deserialize<'de> + Default>(conn: &Connection, key: &str) -> T {
    db::get_settings(conn)
        .ok()
        .and_then(|s| s.get(key).cloned())
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

pub fn config(conn: &Connection) -> SyncConfig {
    get_json(conn, "syncConfig")
}

pub fn state(conn: &Connection) -> SyncState {
    get_json(conn, "syncState")
}

fn save_state(conn: &Connection, s: &SyncState) -> Result<()> {
    db::set_setting(conn, "syncState", &serde_json::to_value(s)?)
}

/// 本机设备 ID（首次使用时生成，不参与同步）
pub fn device_id(conn: &Connection) -> Result<String> {
    if let Some(id) = db::get_settings(conn)?.get("deviceId").and_then(|v| v.as_str()) {
        return Ok(id.to_string());
    }
    let id = uuid::Uuid::new_v4().to_string();
    db::set_setting(conn, "deviceId", &json!(id))?;
    Ok(id)
}

fn device_name() -> &'static str {
    if cfg!(target_os = "android") {
        "Android"
    } else if cfg!(windows) {
        "Windows"
    } else {
        "Desktop"
    }
}

fn fingerprint(s: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}-{}", h.finish(), s.len())
}

// ---------- WebDAV ----------

struct Dav {
    client: reqwest::Client,
    root: String,
    user: String,
    password: String,
}

impl Dav {
    fn new(cfg: &SyncConfig) -> Result<Self> {
        let mut base = cfg.url.trim().to_string();
        if base.is_empty() {
            bail!("还没填写 WebDAV 地址");
        }
        if !base.ends_with('/') {
            base.push('/');
        }
        Ok(Self {
            client: reqwest::Client::builder().timeout(Duration::from_secs(30)).build()?,
            root: format!("{base}{REMOTE_DIR}/"),
            user: cfg.user.trim().to_string(),
            password: cfg.password.clone(),
        })
    }

    fn req(&self, method: &str, path: &str) -> Result<reqwest::RequestBuilder> {
        let m = reqwest::Method::from_bytes(method.as_bytes())?;
        Ok(self.client.request(m, format!("{}{path}", self.root)).basic_auth(&self.user, Some(&self.password)))
    }

    fn check(status: reqwest::StatusCode, what: &str) -> Result<()> {
        match status.as_u16() {
            200..=299 => Ok(()),
            401 => bail!("坚果云拒绝登录（401）：检查账号和「应用密码」（不是登录密码）"),
            403 => bail!("没有权限（403）：{what}"),
            503 | 429 => bail!("请求太频繁，稍后再试（{}）", status.as_u16()),
            s => bail!("{what}失败（HTTP {s}）"),
        }
    }

    /// 建目录；已存在（405）也算成功
    async fn mkcol(&self, path: &str) -> Result<()> {
        let r = self.req("MKCOL", path)?.send().await?;
        if r.status().as_u16() == 405 {
            return Ok(());
        }
        Self::check(r.status(), "创建同步目录")
    }

    /// 列出目录下的文件：(文件名, ETag)
    async fn list(&self, path: &str) -> Result<Vec<(String, String)>> {
        let body = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:"><d:prop><d:getetag/></d:prop></d:propfind>"#;
        let r = self
            .req("PROPFIND", path)?
            .header("Depth", "1")
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(body)
            .send()
            .await?;
        Self::check(r.status(), "读取同步目录")?;
        Ok(parse_propfind(&r.text().await?))
    }

    /// 下载；对方 ETag 未变化时返回 None（304）
    async fn get(&self, path: &str, etag: Option<&str>) -> Result<Option<(String, Option<String>)>> {
        let mut rb = self.req("GET", path)?;
        if let Some(e) = etag {
            rb = rb.header("If-None-Match", e);
        }
        let r = rb.send().await?;
        if r.status().as_u16() == 304 {
            return Ok(None);
        }
        Self::check(r.status(), "下载其他设备的数据")?;
        let etag = r.headers().get("etag").and_then(|v| v.to_str().ok()).map(String::from);
        Ok(Some((r.text().await?, etag)))
    }

    async fn put(&self, path: &str, body: String) -> Result<()> {
        let r = self
            .req("PUT", path)?
            .header("Content-Type", "application/json; charset=utf-8")
            .body(body)
            .send()
            .await?;
        Self::check(r.status(), "上传本机数据")
    }
}

/// 从 PROPFIND 多状态响应里取出各条目的文件名与 ETag（命名空间前缀不固定，按标签名后缀匹配）
fn parse_propfind(xml: &str) -> Vec<(String, String)> {
    fn tag_text<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
        let open = block.find(&format!("{tag}>"))?;
        let start = block[..open].rfind('<')?;
        if block[start..open].contains('/') {
            return None;
        }
        let rest = &block[open + tag.len() + 1..];
        let end = rest.find("</")?;
        Some(rest[..end].trim())
    }
    let mut out = Vec::new();
    for block in xml.split("response>").filter(|b| b.contains("href>")) {
        let Some(href) = tag_text(block, "href") else { continue };
        let name = href.trim_end_matches('/').rsplit('/').next().unwrap_or_default();
        let name = percent_decode(name);
        if !name.ends_with(".json") {
            continue;
        }
        let etag = tag_text(block, "getetag").unwrap_or_default().replace("&quot;", "\"");
        out.push((name, etag));
    }
    out
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---------- 调度 ----------

pub struct SyncRuntime {
    dirty: AtomicBool,
    busy: tokio::sync::Mutex<()>,
    last_pull: Mutex<Option<Instant>>,
}

impl Default for SyncRuntime {
    fn default() -> Self {
        Self { dirty: AtomicBool::new(true), busy: tokio::sync::Mutex::new(()), last_pull: Mutex::new(None) }
    }
}

/// 本地数据有改动：稍后自动上传
pub fn mark_dirty(app: &AppHandle) {
    if let Some(rt) = app.try_state::<SyncRuntime>() {
        rt.dirty.store(true, Ordering::Relaxed);
    }
}

const TICK: Duration = Duration::from_secs(20);
const PULL_EVERY: Duration = Duration::from_secs(10 * 60);

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        loop {
            let rt = app.state::<SyncRuntime>();
            let pull_due = rt.last_pull.lock().map(|t| t.is_none_or(|t| t.elapsed() >= PULL_EVERY)).unwrap_or(true);
            if rt.dirty.load(Ordering::Relaxed) || pull_due {
                let enabled = with_conn(&app, |c| Ok(config(c).enabled)).unwrap_or(false);
                if enabled {
                    let _ = sync_now(&app).await;
                } else {
                    rt.dirty.store(false, Ordering::Relaxed);
                }
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

fn with_conn<T>(app: &AppHandle, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    locked(&app.state::<Db>().0, f)
}

fn locked<T>(db: &Mutex<Connection>, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    let conn = db.lock().map_err(|e| anyhow!("{e}"))?;
    f(&conn)
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub uploaded: bool,
    pub pulled_devices: usize,
    pub changed_rows: usize,
}

/// 完整同步一次：上传本机快照 → 下载其他设备的变化并合并 → 若合并后有变化再上传
pub async fn sync_now(app: &AppHandle) -> Result<SyncReport> {
    let rt = app.state::<SyncRuntime>();
    let _guard = rt.busy.lock().await;
    rt.dirty.store(false, Ordering::Relaxed);
    let _ = app.emit("sync-status", json!({ "state": "syncing" }));

    let result = run(&app.state::<Db>().0).await;
    if let Ok(mut t) = rt.last_pull.lock() {
        *t = Some(Instant::now());
    }
    let (res, msg) = match &result {
        Ok(r) if r.changed_rows > 0 => ("ok", format!("已合并 {} 条变更", r.changed_rows)),
        Ok(_) => ("ok", "已是最新".to_string()),
        Err(e) => ("error", e.to_string()),
    };
    let _ = with_conn(app, |c| {
        let mut st = state(c);
        st.last_sync_at = Some(now_ms());
        st.last_result = Some(res.into());
        st.last_message = Some(msg.clone());
        save_state(c, &st)
    });
    let _ = app.emit("sync-status", json!({ "state": res, "message": msg }));
    if result.as_ref().is_ok_and(|r| r.changed_rows > 0) {
        let _ = app.emit("data-changed", ());
        let _ = app.emit("decisions-changed", ());
    }
    result
}

/// 一次完整同步的流程本体（只依赖数据库连接，便于测试）
async fn run(db: &Mutex<Connection>) -> Result<SyncReport> {
    let (cfg, mut st, my_id) = locked(db, |c| Ok((config(c), state(c), device_id(c)?)))?;
    if cfg.user.trim().is_empty() || cfg.password.is_empty() {
        bail!("还没填写坚果云账号和应用密码");
    }
    let dav = Dav::new(&cfg)?;
    if !st.dirs_ready {
        dav.mkcol("").await.context("创建 TimeTrace 目录")?;
        dav.mkcol("devices/").await.context("创建 devices 目录")?;
        st.dirs_ready = true;
    }
    let my_file = format!("{my_id}.json");
    let mut report = SyncReport { uploaded: false, pulled_devices: 0, changed_rows: 0 };

    upload_if_changed(db, &dav, &mut st, &my_id, &my_file, &mut report).await?;

    // 拉取其他设备
    let files = dav.list("devices/").await?;
    for (name, etag) in files.into_iter().filter(|(n, _)| *n != my_file) {
        if !etag.is_empty() && st.etags.get(&name) == Some(&etag) {
            continue;
        }
        let known = st.etags.get(&name).map(String::as_str);
        let Some((body, new_etag)) = dav.get(&format!("devices/{name}"), known).await? else { continue };
        let snap: Snapshot = serde_json::from_str(&body).with_context(|| format!("解析 {name}"))?;
        let changed = locked(db, |c| {
            let n = merge(c, &snap)?;
            Ok(n + if n > 0 { dedupe_activities(c)? } else { 0 })
        })?;
        report.changed_rows += changed;
        report.pulled_devices += 1;
        st.peers.insert(name.clone(), snap.device_name.clone());
        st.etags.insert(name, new_etag.unwrap_or(etag));
    }

    // 合并带来了变化（含归并项目）→ 本机快照也变了，立即再上传一次，让对方尽快一致
    if report.changed_rows > 0 {
        upload_if_changed(db, &dav, &mut st, &my_id, &my_file, &mut report).await?;
    }
    locked(db, |c| save_state(c, &st))?;
    Ok(report)
}

async fn upload_if_changed(
    db: &Mutex<Connection>,
    dav: &Dav,
    st: &mut SyncState,
    my_id: &str,
    my_file: &str,
    report: &mut SyncReport,
) -> Result<()> {
    let body = locked(db, |c| Ok(serde_json::to_string(&export(c, my_id, device_name())?)?))?;
    let fp = fingerprint(&body);
    if st.upload_hash.as_deref() != Some(fp.as_str()) {
        dav.put(&format!("devices/{my_file}"), body).await?;
        st.upload_hash = Some(fp);
        report.uploaded = true;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> Connection {
        let path = std::env::temp_dir().join(format!("tt-sync-{}.db", uuid::Uuid::new_v4()));
        db::open(&path).unwrap()
    }

    fn act(c: &Connection, name: &str) -> String {
        db::list_activities(c).unwrap().into_iter().find(|a| a.name == name).unwrap().id
    }

    /// 双向交换快照直到稳定
    fn exchange(a: &Connection, b: &Connection) {
        for _ in 0..3 {
            let sa = export(a, "A", "Windows").unwrap();
            let sb = export(b, "B", "Android").unwrap();
            merge(b, &sa).unwrap();
            dedupe_activities(b).unwrap();
            merge(a, &sb).unwrap();
            dedupe_activities(a).unwrap();
        }
    }

    fn summary_ids(c: &Connection, table: &str) -> Vec<String> {
        let sql = if table == "review" {
            "SELECT day FROM review ORDER BY 1".to_string()
        } else {
            format!("SELECT id FROM {table} WHERE deleted=0 ORDER BY 1")
        };
        let mut st = c.prepare(&sql).unwrap();
        st.query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap()
    }

    #[test]
    fn two_devices_converge_and_default_activities_merge() {
        let (a, b) = (device(), device());
        // 两台设备各自的默认项目 ID 不同
        assert_ne!(act(&a, "背单词"), act(&b, "背单词"));
        db::add_entry(&a, &act(&a, "背单词"), 1200, None, Some("2026-10-10".into()), None, "manual").unwrap();
        db::add_entry(&b, &act(&b, "背单词"), 600, None, Some("2026-10-10".into()), None, "manual").unwrap();
        db::add_entry(&b, &act(&b, "刷视频"), 900, None, Some("2026-10-11".into()), None, "manual").unwrap();

        exchange(&a, &b);

        // 项目归并为同一组，且两边一致
        assert_eq!(db::list_activities(&a).unwrap().len(), 4);
        assert_eq!(summary_ids(&a, "activity"), summary_ids(&b, "activity"));
        assert_eq!(act(&a, "背单词"), act(&b, "背单词"));
        // 记录都在，且都挂在保留下来的项目上
        assert_eq!(summary_ids(&a, "entry"), summary_ids(&b, "entry"));
        let s = db::range_stats(&a, "2026-10-10", "2026-10-11").unwrap();
        let words: i64 = s.rows.iter().filter(|r| r.activity_id == act(&a, "背单词")).map(|r| r.seconds).sum();
        assert_eq!(words, 1800);
    }

    #[test]
    fn newer_edit_wins_and_deletes_propagate() {
        let (a, b) = (device(), device());
        exchange(&a, &b);
        let id = act(&a, "刷视频");
        // A 改上限，B 稍后再改 → B 的版本胜出
        let edit = |c: &Connection, minutes| {
            db::save_activity(
                c,
                db::ActivityInput {
                    id: Some(id.clone()),
                    name: "刷视频".into(),
                    color: "#e5484d".into(),
                    kind: "drain".into(),
                    daily_minutes: minutes,
                },
            )
            .unwrap()
        };
        edit(&a, 30);
        std::thread::sleep(Duration::from_millis(5));
        edit(&b, 45);
        exchange(&a, &b);
        for c in [&a, &b] {
            assert_eq!(db::list_activities(c).unwrap().iter().find(|x| x.id == id).unwrap().daily_minutes, 45);
        }
        // 删除记录会同步
        let e = db::add_entry(&a, &id, 300, None, Some("2026-10-12".into()), None, "manual").unwrap();
        exchange(&a, &b);
        assert!(summary_ids(&b, "entry").contains(&e));
        std::thread::sleep(Duration::from_millis(5));
        db::delete_entry(&b, &e).unwrap();
        exchange(&a, &b);
        assert!(!summary_ids(&a, "entry").contains(&e));
    }

    #[test]
    fn reviews_and_profile_sync_but_private_settings_do_not() {
        let (a, b) = (device(), device());
        db::save_review(&a, db::Review { day: "2026-10-12".into(), best: "跟读".into(), ..Default::default() }).unwrap();
        db::set_setting(&a, "decisionProfile", &json!({ "goals": "面试", "rules": "不刷视频" })).unwrap();
        db::set_setting(&a, "jevApiKey", &json!("secret")).unwrap();
        db::set_setting(&a, "syncConfig", &json!({ "password": "secret" })).unwrap();
        exchange(&a, &b);
        let sb = db::get_settings(&b).unwrap();
        assert_eq!(sb["decisionProfile"]["goals"], "面试");
        assert!(!sb.contains_key("jevApiKey"));
        assert!(!sb.contains_key("syncConfig"));
        assert_eq!(summary_ids(&b, "review"), vec!["2026-10-12".to_string()]);
    }

    /// 两台设备经由 WebDAV 同步的端到端测试。需要先启动本地模拟服务器：
    /// `python davmock.py 18080`，再运行 `cargo test --lib webdav_round_trip -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn webdav_round_trip() {
        let cfg = json!({ "enabled": true, "url": "http://127.0.0.1:18080/dav", "user": "u@x.com", "password": "apppass" });
        let (a, b) = (Mutex::new(device()), Mutex::new(device()));
        for d in [&a, &b] {
            db::set_setting(&d.lock().unwrap(), "syncConfig", &cfg).unwrap();
        }
        for (d, secs) in [(&a, 1200), (&b, 600)] {
            let c = d.lock().unwrap();
            let words = act(&c, "背单词");
            db::add_entry(&c, &words, secs, None, Some("2026-10-10".into()), None, "manual").unwrap();
        }
        let rt = |f| tauri::async_runtime::block_on(f);

        let r1 = rt(run(&a)).unwrap(); // A 上传，服务器上还没有别人
        let r2 = rt(run(&b)).unwrap(); // B 上传并拉到 A
        let r3 = rt(run(&a)).unwrap(); // A 拉到 B（含归并后的结果）
        let r4 = rt(run(&b)).unwrap(); // B 再拉一次，收敛
        let r5 = rt(run(&b)).unwrap(); // 无变化：不上传、不重复下载
        for (i, r) in [&r1, &r2, &r3, &r4, &r5].iter().enumerate() {
            println!("round {}: uploaded={} pulled={} changed={}", i + 1, r.uploaded, r.pulled_devices, r.changed_rows);
        }
        assert!(r1.uploaded && r1.pulled_devices == 0);
        assert!(!r4.uploaded, "B 拉取后无变化，不应重复上传");
        assert!(r2.changed_rows > 0);
        assert!(!r5.uploaded && r5.pulled_devices == 0 && r5.changed_rows == 0);
        let (ca, cb) = (a.lock().unwrap(), b.lock().unwrap());
        assert_eq!(summary_ids(&ca, "entry"), summary_ids(&cb, "entry"));
        assert_eq!(summary_ids(&ca, "activity"), summary_ids(&cb, "activity"));
        assert_eq!(summary_ids(&ca, "entry").len(), 2);

        // 账号错误时给出清楚的提示
        drop((ca, cb));
        let bad = Mutex::new(device());
        db::set_setting(&bad.lock().unwrap(), "syncConfig", &json!({ "url": "http://127.0.0.1:18080/dav", "user": "u@x.com", "password": "wrong" })).unwrap();
        let e = format!("{:#}", rt(run(&bad)).unwrap_err());
        println!("bad password -> {e}");
        assert!(e.contains("401"));
    }

    #[test]
    fn parses_jianguoyun_style_propfind() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<d:multistatus xmlns:d="DAV:">
<d:response><d:href>/dav/TimeTrace/devices/</d:href><d:propstat><d:prop><d:getetag/></d:prop></d:propstat></d:response>
<d:response><d:href>/dav/TimeTrace/devices/abc-1.json</d:href><d:propstat><d:prop><d:getetag>&quot;e1&quot;</d:getetag></d:prop></d:propstat></d:response>
<D:response><D:href>/dav/TimeTrace/devices/%E8%AE%BE%E5%A4%87.json</D:href><D:propstat><D:prop><D:getetag>"e2"</D:getetag></D:prop></D:propstat></D:response>
</d:multistatus>"#;
        assert_eq!(
            parse_propfind(xml),
            vec![("abc-1.json".to_string(), "\"e1\"".to_string()), ("设备.json".to_string(), "\"e2\"".to_string())]
        );
    }
}
