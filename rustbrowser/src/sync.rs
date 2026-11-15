//! ORVYN SYNC v1: обмен закладками и историей через файл в общей папке.
//!
//! Никаких серверов и аккаунтов: пользователь указывает папку (например,
//! синхронизируемую OneDrive/Яндекс.Диском), ORVYN пишет туда `orvyn-sync.json`
//! и при следующей синхронизации объединяет данные с локальными.
//!
//! Объединение безопасное: ничего не удаляется. Настройки и пароли в файл
//! не попадают — только закладки и история.

use crate::storage;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "orvyn-sync.json";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct SyncFile {
    pub app: String,
    pub version: u32,
    pub updated: u64,
    pub bookmarks: Vec<storage::Bookmark>,
    pub history: Vec<storage::HistoryEntry>,
}

impl SyncFile {
    pub fn from_store(store: &storage::Store) -> Self {
        Self {
            app: "ORVYN".to_string(),
            version: 1,
            updated: storage::now(),
            bookmarks: store.bookmarks.clone(),
            history: store.history.clone(),
        }
    }
}

pub fn path_in(dir: &str) -> PathBuf {
    Path::new(dir).join(FILE_NAME)
}

/// Читает файл синхронизации. `Ok(None)` — файла ещё нет (первый запуск).
pub fn load(path: &Path) -> Result<Option<SyncFile>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(path).map_err(|e| format!("не читается файл: {e}"))?;
    let file: SyncFile =
        serde_json::from_str(&data).map_err(|e| format!("файл синхронизации повреждён: {e}"))?;
    if file.app != "ORVYN" {
        return Err("это не файл синхронизации ORVYN".to_string());
    }
    Ok(Some(file))
}

pub fn save(path: &Path, file: &SyncFile) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("не создать папку: {e}"))?;
    }
    let data = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    std::fs::write(path, data).map_err(|e| format!("не записать файл: {e}"))
}

/// Объединяет локальные данные с файлом. Ничего не удаляет.
/// Возвращает новые списки и счётчики добавленного.
pub fn merge(
    local_bookmarks: &[storage::Bookmark],
    local_history: &[storage::HistoryEntry],
    remote: &SyncFile,
) -> (
    Vec<storage::Bookmark>,
    Vec<storage::HistoryEntry>,
    usize,
    usize,
) {
    let mut bookmarks = local_bookmarks.to_vec();
    let mut known: HashSet<String> = bookmarks.iter().map(|b| b.url.clone()).collect();
    let mut added_bookmarks = 0;
    for b in &remote.bookmarks {
        if b.url.is_empty() {
            continue;
        }
        if known.insert(b.url.clone()) {
            bookmarks.push(b.clone());
            added_bookmarks += 1;
        } else if let Some(own) = bookmarks.iter_mut().find(|x| x.url == b.url) {
            if own.title.is_empty() && !b.title.is_empty() {
                own.title = b.title.clone();
            }
        }
    }

    let mut history = local_history.to_vec();
    let mut seen: HashSet<(u64, String)> =
        history.iter().map(|h| (h.ts, h.url.clone())).collect();
    let mut added_history = 0;
    for h in &remote.history {
        if h.url.is_empty() {
            continue;
        }
        if seen.insert((h.ts, h.url.clone())) {
            history.push(h.clone());
            added_history += 1;
        }
    }
    history.sort_by(|a, b| b.ts.cmp(&a.ts));
    if history.len() > 10_000 {
        history.truncate(10_000);
    }

    (bookmarks, history, added_bookmarks, added_history)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bm(url: &str, title: &str) -> storage::Bookmark {
        storage::Bookmark {
            url: url.to_string(),
            title: title.to_string(),
        }
    }

    fn hist(url: &str, ts: u64) -> storage::HistoryEntry {
        storage::HistoryEntry {
            url: url.to_string(),
            title: url.to_string(),
            ts,
        }
    }

    #[test]
    fn merge_keeps_local_and_adds_remote() {
        let local = vec![bm("https://a.example", "A")];
        let remote = SyncFile {
            app: "ORVYN".to_string(),
            bookmarks: vec![bm("https://a.example", "A2"), bm("https://b.example", "B")],
            history: vec![],
            ..SyncFile::default()
        };
        let (out, _, added, _) = merge(&local, &[], &remote);
        assert_eq!(out.len(), 2);
        assert_eq!(added, 1);
        assert_eq!(out[0].title, "A");
    }

    #[test]
    fn merge_fills_empty_titles() {
        let local = vec![bm("https://a.example", "")];
        let remote = SyncFile {
            app: "ORVYN".to_string(),
            bookmarks: vec![bm("https://a.example", "A")],
            ..SyncFile::default()
        };
        let (out, _, added, _) = merge(&local, &[], &remote);
        assert_eq!(added, 0);
        assert_eq!(out[0].title, "A");
    }

    #[test]
    fn merge_history_dedupes_by_time_and_url() {
        let local = vec![hist("https://a.example", 10)];
        let remote = SyncFile {
            app: "ORVYN".to_string(),
            history: vec![hist("https://a.example", 10), hist("https://a.example", 20)],
            ..SyncFile::default()
        };
        let (_, out, _, added) = merge(&[], &local, &remote);
        assert_eq!(added, 1);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].ts, 20);
    }

    #[test]
    fn merge_sorts_history_newest_first() {
        let remote = SyncFile {
            app: "ORVYN".to_string(),
            history: vec![hist("https://old.example", 1), hist("https://new.example", 5)],
            ..SyncFile::default()
        };
        let (_, out, _, _) = merge(&[], &[], &remote);
        assert_eq!(out[0].url, "https://new.example");
    }
}
