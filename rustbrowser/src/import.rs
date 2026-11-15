//! Импорт данных из других браузеров.
//!
//! Читает закладки и историю из профилей Chromium-браузеров и Firefox.
//! Пароли импортируются отдельно из CSV-файла: браузеры шифруют свои базы,
//! а официальный экспорт делает сам пользователь.

use crate::storage::{Bookmark, HistoryEntry};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Chromium,
    Firefox,
}

#[derive(Clone, Debug)]
pub struct Browser {
    pub id: String,
    pub name: String,
    pub profile: PathBuf,
    pub kind: Kind,
    pub has_bookmarks: bool,
    pub has_history: bool,
}

#[derive(Default)]
pub struct ImportData {
    pub bookmarks: Vec<Bookmark>,
    pub history: Vec<HistoryEntry>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PasswordRow {
    pub origin: String,
    pub title: String,
    pub username: String,
    pub password: String,
}

/// Ищет установленные браузеры и их основные профили.
pub fn detect() -> Vec<Browser> {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let roaming = std::env::var_os("APPDATA").map(PathBuf::from);
    let mut out = Vec::new();

    let chromium = [
        ("chrome", "Google Chrome", local.as_ref().map(|p| p.join(r"Google\Chrome\User Data"))),
        ("edge", "Microsoft Edge", local.as_ref().map(|p| p.join(r"Microsoft\Edge\User Data"))),
        ("brave", "Brave", local.as_ref().map(|p| p.join(r"BraveSoftware\Brave-Browser\User Data"))),
        ("vivaldi", "Vivaldi", local.as_ref().map(|p| p.join(r"Vivaldi\User Data"))),
        ("yandex", "Yandex Browser", local.as_ref().map(|p| p.join(r"Yandex\YandexBrowser\User Data"))),
        ("opera", "Opera", roaming.as_ref().map(|p| p.join(r"Opera Software\Opera Stable"))),
        ("opera-gx", "Opera GX", roaming.as_ref().map(|p| p.join(r"Opera Software\Opera GX Stable"))),
    ];

    for (id, name, root) in chromium {
        let Some(root) = root else { continue };
        let Some(profile) = chromium_profile(&root) else { continue };
        let bookmarks = profile.join("Bookmarks").exists();
        let history = profile.join("History").exists();
        if bookmarks || history {
            out.push(Browser {
                id: id.to_string(),
                name: name.to_string(),
                profile,
                kind: Kind::Chromium,
                has_bookmarks: bookmarks,
                has_history: history,
            });
        }
    }

    let firefox_roots = [
        ("firefox", "Firefox", roaming.as_ref().map(|p| p.join(r"Mozilla\Firefox\Profiles"))),
        ("waterfox", "Waterfox", roaming.as_ref().map(|p| p.join(r"Waterfox\Profiles"))),
        ("librewolf", "LibreWolf", roaming.as_ref().map(|p| p.join(r"librewolf\Profiles"))),
    ];
    for (id, name, root) in firefox_roots {
        let Some(root) = root else { continue };
        let Some(profile) = firefox_profile(&root) else { continue };
        out.push(Browser {
            id: id.to_string(),
            name: name.to_string(),
            profile,
            kind: Kind::Firefox,
            has_bookmarks: true,
            has_history: true,
        });
    }

    out
}

fn chromium_profile(root: &Path) -> Option<PathBuf> {
    // Opera хранит профиль прямо в корне, остальные — в Default/Profile N.
    if root.join("Bookmarks").exists() || root.join("History").exists() {
        return Some(root.to_path_buf());
    }
    let default = root.join("Default");
    if default.join("Bookmarks").exists() || default.join("History").exists() {
        return Some(default);
    }
    let entries = std::fs::read_dir(root).ok()?;
    let mut profiles: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|p| p.join("Bookmarks").exists() || p.join("History").exists())
        .collect();
    profiles.sort();
    profiles.into_iter().next()
}

fn firefox_profile(root: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    let mut profiles: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("places.sqlite").exists())
        .collect();
    profiles.sort_by_key(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let rank = if name.contains("default-release") {
            0
        } else if name.contains("default") {
            1
        } else {
            2
        };
        (rank, name.to_string())
    });
    profiles.into_iter().next()
}

/// Читает закладки и историю из выбранного профиля.
pub fn read(browser: &Browser) -> Result<ImportData, String> {
    let mut data = ImportData::default();
    let mut tried = false;

    if browser.has_bookmarks {
        tried = true;
        let result = match browser.kind {
            Kind::Chromium => chromium_bookmarks(&browser.profile.join("Bookmarks")),
            Kind::Firefox => firefox_bookmarks(&browser.profile.join("places.sqlite")),
        };
        match result {
            Ok(mut items) => {
                items = dedupe_bookmarks(items);
                data.bookmarks = items;
            }
            Err(e) => data.warnings.push(format!("закладки: {e}")),
        }
    }

    if browser.has_history {
        tried = true;
        let result = match browser.kind {
            Kind::Chromium => chromium_history(&browser.profile.join("History")),
            Kind::Firefox => firefox_history(&browser.profile.join("places.sqlite")),
        };
        match result {
            Ok(items) => data.history = items,
            Err(e) => data.warnings.push(format!("история: {e}")),
        }
    }

    if !tried {
        return Err("в профиле нет закладок и истории".to_string());
    }
    if data.bookmarks.is_empty() && data.history.is_empty() && !data.warnings.is_empty() {
        return Err(data.warnings.join("; "));
    }
    Ok(data)
}

fn chromium_bookmarks(file: &Path) -> Result<Vec<Bookmark>, String> {
    let raw = std::fs::read_to_string(file).map_err(|e| format!("не читается файл закладок: {e}"))?;
    let value: Value =
        serde_json::from_str(&raw).map_err(|_| "файл закладок повреждён".to_string())?;
    let mut out = Vec::new();
    let Some(roots) = value.get("roots").and_then(|r| r.as_object()) else {
        return Ok(out);
    };
    for node in roots.values() {
        walk_bookmarks(node, &mut out);
    }
    Ok(out)
}

fn walk_bookmarks(node: &Value, out: &mut Vec<Bookmark>) {
    match node.get("type").and_then(|t| t.as_str()) {
        Some("url") => {
            let url = node.get("url").and_then(|u| u.as_str()).unwrap_or("");
            if !is_web_url(url) {
                return;
            }
            let title = node.get("name").and_then(|n| n.as_str()).unwrap_or("");
            out.push(Bookmark {
                url: limit(url, 2000),
                title: clean_title(title, url),
            });
        }
        _ => {
            if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
                for child in children {
                    walk_bookmarks(child, out);
                }
            }
        }
    }
}

fn chromium_history(file: &Path) -> Result<Vec<HistoryEntry>, String> {
    let db = TempDb::copy(file)?;
    let conn = Connection::open_with_flags(
        &db.0,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("не открывается история: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT url, COALESCE(title, ''), last_visit_time
             FROM urls
             WHERE last_visit_time > 0
               AND (url LIKE 'http://%' OR url LIKE 'https://%')
             ORDER BY last_visit_time DESC
             LIMIT 2000",
        )
        .map_err(|e| format!("не читается история: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            let url: String = row.get(0)?;
            let title: String = row.get(1)?;
            let ts: i64 = row.get(2)?;
            Ok((url, title, ts))
        })
        .map_err(|e| format!("не читается история: {e}"))?;

    let now = crate::storage::now();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for row in rows.flatten() {
        let (url, title, ts) = row;
        if !is_web_url(&url) || !seen.insert(url.to_lowercase()) {
            continue;
        }
        let unix = chrome_time(ts);
        out.push(HistoryEntry {
            url: limit(&url, 2000),
            title: clean_title(&title, &url),
            ts: if unix == 0 { now } else { unix.min(now) },
        });
    }
    out.sort_by_key(|h| h.ts);
    Ok(out)
}

fn firefox_bookmarks(file: &Path) -> Result<Vec<Bookmark>, String> {
    let db = TempDb::copy(file)?;
    let conn = Connection::open_with_flags(
        &db.0,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("не открывается база Firefox: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT p.url, COALESCE(b.title, p.title, p.url)
             FROM moz_bookmarks b
             JOIN moz_places p ON p.id = b.fk
             WHERE b.type = 1
               AND (p.url LIKE 'http://%' OR p.url LIKE 'https://%')
             LIMIT 5000",
        )
        .map_err(|e| format!("не читаются закладки Firefox: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            let url: String = row.get(0)?;
            let title: String = row.get(1)?;
            Ok((url, title))
        })
        .map_err(|e| format!("не читаются закладки Firefox: {e}"))?;
    let mut out = Vec::new();
    for row in rows.flatten() {
        let (url, title) = row;
        if is_web_url(&url) {
            out.push(Bookmark {
                url: limit(&url, 2000),
                title: clean_title(&title, &url),
            });
        }
    }
    Ok(out)
}

fn firefox_history(file: &Path) -> Result<Vec<HistoryEntry>, String> {
    let db = TempDb::copy(file)?;
    let conn = Connection::open_with_flags(
        &db.0,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("не открывается база Firefox: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT url, COALESCE(title, ''), last_visit_date
             FROM moz_places
             WHERE last_visit_date IS NOT NULL
               AND (url LIKE 'http://%' OR url LIKE 'https://%')
             ORDER BY last_visit_date DESC
             LIMIT 2000",
        )
        .map_err(|e| format!("не читается история Firefox: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            let url: String = row.get(0)?;
            let title: String = row.get(1)?;
            let ts: i64 = row.get(2)?;
            Ok((url, title, ts))
        })
        .map_err(|e| format!("не читается история Firefox: {e}"))?;
    let now = crate::storage::now();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for row in rows.flatten() {
        let (url, title, ts) = row;
        if !is_web_url(&url) || !seen.insert(url.to_lowercase()) {
            continue;
        }
        let unix = if ts > 0 { (ts / 1_000_000) as u64 } else { 0 };
        out.push(HistoryEntry {
            url: limit(&url, 2000),
            title: clean_title(&title, &url),
            ts: if unix == 0 { now } else { unix.min(now) },
        });
    }
    out.sort_by_key(|h| h.ts);
    Ok(out)
}

fn dedupe_bookmarks(items: Vec<Bookmark>) -> Vec<Bookmark> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|b| seen.insert(b.url.to_lowercase()))
        .collect()
}

fn is_web_url(url: &str) -> bool {
    let u = url.trim();
    u.starts_with("http://") || u.starts_with("https://")
}

fn clean_title(title: &str, url: &str) -> String {
    let t = title.trim();
    if t.is_empty() {
        limit(url, 300)
    } else {
        limit(t, 300)
    }
}

fn limit(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Chrome хранит время в микросекундах с 1601-01-01.
fn chrome_time(us: i64) -> u64 {
    const EPOCH_DIFF: i64 = 11_644_473_600_000_000;
    let unix = us.saturating_sub(EPOCH_DIFF);
    if unix > 0 {
        (unix / 1_000_000) as u64
    } else {
        0
    }
}

struct TempDb(PathBuf);

impl TempDb {
    fn copy(src: &Path) -> Result<Self, String> {
        let n = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let name = format!("orvyn-import-{}-{}.sqlite", std::process::id(), n);
        let dst = std::env::temp_dir().join(name);
        std::fs::copy(src, &dst)
            .map_err(|e| format!("не удалось скопировать базу {}: {e}", src.display()))?;
        // WAL-файлы нужны, если браузер не успел слить изменения в основной файл.
        for suffix in ["-wal", "-shm"] {
            let from = PathBuf::from(format!("{}{suffix}", src.display()));
            if from.exists() {
                let to = PathBuf::from(format!("{}{suffix}", dst.display()));
                let _ = std::fs::copy(from, to);
            }
        }
        Ok(Self(dst))
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let p = PathBuf::from(format!("{}{suffix}", self.0.display()));
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Разбирает CSV с паролями из Chrome, Edge, Firefox и других браузеров.
/// Нужны колонки url/origin, username и password.
pub fn parse_passwords_csv(text: &str) -> Result<Vec<PasswordRow>, String> {
    let text = text.trim_start_matches('\u{feff}');
    let rows = parse_csv(text);
    if rows.len() < 2 {
        return Err("CSV пустой или без строк с паролями".to_string());
    }
    let header: Vec<String> = rows[0]
        .iter()
        .map(|h| h.trim().trim_start_matches('\u{feff}').to_lowercase())
        .collect();
    let find = |names: &[&str]| {
        header
            .iter()
            .position(|h| names.iter().any(|n| h == n))
    };
    let url_i = find(&["url", "origin", "login_uri", "website"])
        .ok_or_else(|| "в CSV нет колонки url".to_string())?;
    let user_i = find(&["username", "login_username", "user", "login"])
        .ok_or_else(|| "в CSV нет колонки username".to_string())?;
    let pass_i = find(&["password", "login_password", "pass"])
        .ok_or_else(|| "в CSV нет колонки password".to_string())?;
    let name_i = find(&["name", "title"]);

    let mut out = Vec::new();
    for row in rows.iter().skip(1) {
        let get = |i: usize| row.get(i).map(|s| s.as_str()).unwrap_or("");
        let origin = get(url_i).trim();
        let username = get(user_i).trim();
        let password = get(pass_i);
        if origin.is_empty() || password.is_empty() {
            continue;
        }
        let title = name_i.map(get).unwrap_or("").trim();
        out.push(PasswordRow {
            origin: origin.to_string(),
            title: if title.is_empty() {
                origin.to_string()
            } else {
                title.to_string()
            },
            username: username.to_string(),
            password: password.to_string(),
        });
    }
    if out.is_empty() {
        return Err("в CSV не найдено ни одного пароля".to_string());
    }
    Ok(out)
}

fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => quoted = true,
                ',' => row.push(std::mem::take(&mut field)),
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| !r.iter().all(|f| f.trim().is_empty()));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromium_bookmarks_are_read_recursively() {
        let path = std::env::temp_dir().join(format!(
            "orvyn-import-bookmarks-{}.json",
            std::process::id()
        ));
        let json = r#"{
          "roots": {
            "bookmark_bar": {
              "type": "folder",
              "children": [
                {"type": "url", "name": "Example", "url": "https://example.com/"},
                {"type": "folder", "children": [
                  {"type": "url", "name": "Second", "url": "https://second.example/"}
                ]}
              ]
            },
            "other": {"type": "folder", "children": []}
          }
        }"#;
        std::fs::write(&path, json).unwrap();
        let items = chromium_bookmarks(&path).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].url, "https://example.com/");
        assert_eq!(items[1].title, "Second");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn passwords_csv_supports_chrome_and_firefox() {
        let chrome = "name,url,username,password,note\n\
Example,https://example.com,user@example.com,s3cret,\n";
        let rows = parse_passwords_csv(chrome).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].origin, "https://example.com");
        assert_eq!(rows[0].password, "s3cret");

        let firefox = "url,username,password,httpRealm,formActionOrigin,guid,timeCreated,timeLastUsed,timePasswordChanged\n\
\"https://site.example/login\",\"me\",\"p,w\",\"\",\"\",\"guid\",1,2,3\n";
        let rows = parse_passwords_csv(firefox).unwrap();
        assert_eq!(rows[0].password, "p,w");
    }

    #[test]
    fn csv_quotes_and_newlines() {
        let rows = parse_csv("a,b,c\n\"x,y\",\"he said \"\"hi\"\"\",\"line1\nline2\"\n");
        assert_eq!(rows[1][0], "x,y");
        assert_eq!(rows[1][1], "he said \"hi\"");
        assert_eq!(rows[1][2], "line1\nline2");
    }

    #[test]
    fn chrome_time_is_converted() {
        assert_eq!(chrome_time(11_644_473_600_000_000), 0);
        assert_eq!(chrome_time(11_644_473_600_000_000 + 5_000_000), 5);
    }

    #[test]
    fn history_is_read_from_chromium_db() {
        let path = std::env::temp_dir().join(format!(
            "orvyn-import-history-{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE urls (url TEXT, title TEXT, last_visit_time INTEGER);
                 INSERT INTO urls VALUES ('https://example.com/', 'Example', 11644473600000000 + 1000000);",
            )
            .unwrap();
        }
        let items = chromium_history(&path).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Example");
        assert!(items[0].ts > 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn firefox_history_is_read_from_db() {
        let path = std::env::temp_dir().join(format!(
            "orvyn-import-ff-{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE moz_places (url TEXT, title TEXT, last_visit_date INTEGER);
                 INSERT INTO moz_places VALUES ('https://example.org/', 'Example', 1600000000000000);",
            )
            .unwrap();
        }
        let items = firefox_history(&path).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].ts, 1_600_000_000);
        let _ = std::fs::remove_file(&path);
    }
}
