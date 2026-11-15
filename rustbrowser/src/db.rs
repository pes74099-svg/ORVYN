//! Локальная база ORVYN: история и закладки в SQLite.
//!
//! `store.json` остаётся для настроек, тем, сессий и рабочих пространств.
//! История и закладки переезжают в `orvyn.db` (WAL): так можно хранить больше
//! записей, не перезаписывая весь профиль на каждую навигацию.
//!
//! При первом запуске данные из `store.json` импортируются в базу.
//! Если база недоступна, приложение продолжает работать на JSON.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use rusqlite::{params, Connection};

use crate::storage::{Bookmark, HistoryEntry, Store};

const HISTORY_LIMIT: i64 = 2000;
const SCHEMA_VERSION: i64 = 1;

static DB: OnceLock<Mutex<Connection>> = OnceLock::new();

pub fn path() -> PathBuf {
    crate::storage::data_dir().join("orvyn.db")
}

pub fn ready() -> bool {
    DB.get().is_some()
}

/// Открывает базу, создаёт схему и индексы. Вызывается один раз при старте.
pub fn init() -> Result<(), String> {
    if DB.get().is_some() {
        return Ok(());
    }
    if let Some(dir) = path().parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("каталог базы: {e}"))?;
    }
    // Если файл базы пропал, но есть резервная копия — поднимаем её, а не начинаем с пустой.
    if !path().exists() {
        let bak = backup_path();
        if bak.exists() && std::fs::copy(&bak, path()).is_ok() {
            crate::logging::warn("orvyn.db отсутствовала, восстановлена из orvyn.db.bak");
        }
    }
    match open_prepared() {
        Ok(conn) => {
            DB.set(Mutex::new(conn))
                .map_err(|_| "база уже открыта".to_string())?;
            Ok(())
        }
        Err(first) => {
            crate::logging::error(&format!("orvyn.db не открылась: {first}"));
            if restore_backup() {
                let conn = open_prepared()
                    .map_err(|e| format!("{first}; после восстановления: {e}"))?;
                DB.set(Mutex::new(conn))
                    .map_err(|_| "база уже открыта".to_string())?;
                crate::logging::warn("база восстановлена из orvyn.db.bak");
                Ok(())
            } else {
                Err(first)
            }
        }
    }
}

fn open_prepared() -> Result<Connection, String> {
    let conn = Connection::open(path()).map_err(|e| format!("открытие orvyn.db: {e}"))?;
    configure(&conn).map_err(|e| format!("настройка базы: {e}"))?;
    migrate(&conn).map_err(|e| format!("схема базы: {e}"))?;
    if let Err(e) = backup_db(&conn) {
        crate::logging::warn(&format!("резервная копия базы: {e}"));
    }
    Ok(conn)
}

fn backup_path() -> PathBuf {
    crate::storage::data_dir().join("orvyn.db.bak")
}

/// Онлайн-копия базы при старте: `VACUUM INTO` даёт цельный файл без WAL.
fn backup_db(conn: &Connection) -> Result<(), String> {
    let has: i64 = conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM history) + (SELECT COUNT(*) FROM bookmarks)",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if has == 0 {
        return Ok(());
    }
    let bak = backup_path();
    let tmp = crate::storage::data_dir().join("orvyn.db.bak.tmp");
    let _ = std::fs::remove_file(&tmp);
    let literal = tmp.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{literal}'"))
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&bak);
    std::fs::rename(&tmp, &bak).map_err(|e| e.to_string())
}

fn restore_backup() -> bool {
    let bak = backup_path();
    if !bak.exists() {
        return false;
    }
    let dir = crate::storage::data_dir();
    let _ = std::fs::remove_file(dir.join("orvyn.db-wal"));
    let _ = std::fs::remove_file(dir.join("orvyn.db-shm"));
    std::fs::copy(&bak, path()).is_ok()
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "busy_timeout", 2000)?;
    Ok(())
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= SCHEMA_VERSION {
        return Ok(());
    }
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS history (
            url_key TEXT PRIMARY KEY,
            url     TEXT NOT NULL,
            title   TEXT NOT NULL DEFAULT '',
            ts      INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS history_ts ON history(ts);

        CREATE TABLE IF NOT EXISTS bookmarks (
            url_key TEXT PRIMARY KEY,
            url     TEXT NOT NULL,
            title   TEXT NOT NULL DEFAULT '',
            added   INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS bookmarks_added ON bookmarks(added);

        CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

fn with<T>(f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T, String> {
    let db = DB.get().ok_or_else(|| "база не открыта".to_string())?;
    let conn = db.lock().map_err(|_| "база заблокирована".to_string())?;
    f(&conn).map_err(|e| e.to_string())
}

/// Переносит историю и закладки из старого `store.json`, если база пустая.
pub fn import_from(store: &Store) -> Result<(usize, usize), String> {
    with(|conn| {
        let mut added_history = 0usize;
        let mut added_bookmarks = 0usize;
        let history_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0))?;
        let bookmark_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get(0))?;

        if history_count == 0 && !store.history.is_empty() {
            added_history = replace_history_conn(conn, &store.history)?;
        }
        if bookmark_count == 0 && !store.bookmarks.is_empty() {
            added_bookmarks = replace_bookmarks_conn(conn, &store.bookmarks)?;
        }
        Ok((added_history, added_bookmarks))
    })
}

pub fn load_history(limit: usize) -> Result<Vec<HistoryEntry>, String> {
    with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT url, title, ts FROM (
                 SELECT url, title, ts FROM history ORDER BY ts DESC LIMIT ?1
             ) ORDER BY ts ASC",
        )?;
        let rows = stmt.query_map([limit as i64], |r| {
            Ok(HistoryEntry {
                url: r.get(0)?,
                title: r.get(1)?,
                ts: r.get::<_, i64>(2)? as u64,
            })
        })?;
        rows.collect()
    })
}

pub fn load_bookmarks() -> Result<Vec<Bookmark>, String> {
    with(|conn| {
        let mut stmt =
            conn.prepare("SELECT url, title FROM bookmarks ORDER BY added ASC, url ASC")?;
        let rows = stmt.query_map([], |r| {
            Ok(Bookmark {
                url: r.get(0)?,
                title: r.get(1)?,
            })
        })?;
        rows.collect()
    })
}

pub fn add_history(url: &str, title: &str, ts: u64) {
    if let Err(e) = with(|conn| add_history_conn(conn, url, title, ts)) {
        crate::logging::error(&format!("история не записана: {e}"));
    }
}

fn add_history_conn(
    conn: &Connection,
    url: &str,
    title: &str,
    ts: u64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO history(url_key, url, title, ts) VALUES(?1, ?2, ?3, ?4)
         ON CONFLICT(url_key) DO UPDATE SET title = excluded.title, ts = excluded.ts",
        params![url.to_lowercase(), url, title, ts as i64],
    )?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0))?;
    if count > HISTORY_LIMIT {
        conn.execute(
            "DELETE FROM history WHERE url_key IN (
                 SELECT url_key FROM history ORDER BY ts ASC LIMIT ?1
             )",
            [count - HISTORY_LIMIT],
        )?;
    }
    Ok(())
}

pub fn upsert_bookmark(url: &str, title: &str) {
    if let Err(e) = with(|conn| {
        conn.execute(
            "INSERT INTO bookmarks(url_key, url, title, added) VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(url_key) DO UPDATE SET title = excluded.title",
            params![url.to_lowercase(), url, title, crate::storage::now() as i64],
        )?;
        Ok(())
    }) {
        crate::logging::error(&format!("закладка не записана: {e}"));
    }
}

pub fn remove_bookmark(url: &str) {
    if let Err(e) = with(|conn| {
        conn.execute(
            "DELETE FROM bookmarks WHERE url_key = ?1",
            [url.to_lowercase()],
        )?;
        Ok(())
    }) {
        crate::logging::error(&format!("закладка не удалена: {e}"));
    }
}

pub fn replace_history(items: &[HistoryEntry]) {
    if let Err(e) = with(|conn| replace_history_conn(conn, items).map(|_| ())) {
        crate::logging::error(&format!("история не перезаписана: {e}"));
    }
}

fn replace_history_conn(conn: &Connection, items: &[HistoryEntry]) -> rusqlite::Result<usize> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM history", [])?;
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO history(url_key, url, title, ts) VALUES(?1, ?2, ?3, ?4)",
        )?;
        for e in items {
            stmt.execute(params![
                e.url.to_lowercase(),
                e.url,
                e.title,
                e.ts as i64
            ])?;
        }
    }
    let count: i64 = tx.query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0))?;
    if count > HISTORY_LIMIT {
        tx.execute(
            "DELETE FROM history WHERE url_key IN (
                 SELECT url_key FROM history ORDER BY ts ASC LIMIT ?1
             )",
            [count - HISTORY_LIMIT],
        )?;
    }
    let inserted = count.min(HISTORY_LIMIT);
    tx.commit()?;
    Ok(inserted as usize)
}

pub fn replace_bookmarks(items: &[Bookmark]) {
    if let Err(e) = with(|conn| replace_bookmarks_conn(conn, items)) {
        crate::logging::error(&format!("закладки не перезаписаны: {e}"));
    }
}

fn replace_bookmarks_conn(conn: &Connection, items: &[Bookmark]) -> rusqlite::Result<usize> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM bookmarks", [])?;
    {
        let base = crate::storage::now().saturating_sub(items.len() as u64);
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO bookmarks(url_key, url, title, added) VALUES(?1, ?2, ?3, ?4)",
        )?;
        for (i, b) in items.iter().enumerate() {
            stmt.execute(params![
                b.url.to_lowercase(),
                b.url,
                b.title,
                (base + i as u64) as i64
            ])?;
        }
    }
    tx.commit()?;
    let inserted: i64 =
        conn.query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get(0))?;
    Ok(inserted as usize)
}

pub fn clear_history() {
    if let Err(e) = with(|conn| {
        conn.execute("DELETE FROM history", [])?;
        Ok(())
    }) {
        crate::logging::error(&format!("история не очищена: {e}"));
    }
}

pub fn counts() -> Result<(usize, usize), String> {
    with(|conn| {
        let h: i64 = conn.query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0))?;
        let b: i64 = conn.query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get(0))?;
        Ok((h as usize, b as usize))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn history_dedupes_and_keeps_latest() {
        let conn = test_db();
        add_history_conn(&conn, "https://Example.com/a", "A", 10).unwrap();
        add_history_conn(&conn, "https://example.com/a", "A2", 20).unwrap();
        add_history_conn(&conn, "https://example.com/b", "B", 30).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        let title: String = conn
            .query_row(
                "SELECT title FROM history WHERE url_key = 'https://example.com/a'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(title, "A2");
    }

    #[test]
    fn history_is_capped() {
        let conn = test_db();
        for i in 0..(HISTORY_LIMIT + 5) {
            add_history_conn(&conn, &format!("https://example.com/{i}"), "x", i as u64)
                .unwrap();
        }
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, HISTORY_LIMIT);
        let oldest: String = conn
            .query_row("SELECT url FROM history ORDER BY ts ASC LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(oldest, "https://example.com/5");
    }

    #[test]
    fn bookmarks_replace_and_order() {
        let conn = test_db();
        let items = vec![
            Bookmark {
                url: "https://a.example".into(),
                title: "A".into(),
            },
            Bookmark {
                url: "https://b.example".into(),
                title: "B".into(),
            },
        ];
        replace_bookmarks_conn(&conn, &items).unwrap();
        let mut stmt = conn
            .prepare("SELECT url FROM bookmarks ORDER BY added ASC, url ASC")
            .unwrap();
        let urls: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|x| x.unwrap())
            .collect();
        assert_eq!(urls, vec!["https://a.example", "https://b.example"]);
    }
}
