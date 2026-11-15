//! ORVYN BACKUP: выборочная резервная копия в JSON.

use crate::storage;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Backup {
    pub app: String,
    pub version: u32,
    pub created: u64,
    pub bookmarks: Option<Vec<storage::Bookmark>>,
    pub settings: Option<storage::Settings>,
    pub history: Option<Vec<storage::HistoryEntry>>,
    pub session: Option<Vec<storage::SessionTab>>,
    pub workspaces: Option<Vec<storage::Workspace>>,
    pub saved_sets: Option<Vec<storage::SavedSet>>,
}

impl Backup {
    pub fn new() -> Self {
        Self {
            app: "ORVYN".to_string(),
            version: storage::STORE_VERSION,
            created: storage::now(),
            ..Self::default()
        }
    }

    /// Есть ли в файле хоть что-то полезное.
    pub fn is_empty(&self) -> bool {
        self.bookmarks.is_none()
            && self.settings.is_none()
            && self.history.is_none()
            && self.session.is_none()
            && self.workspaces.is_none()
            && self.saved_sets.is_none()
    }
}

pub fn write(path: &Path, backup: &Backup) -> Result<(), String> {
    let data = serde_json::to_string_pretty(backup).map_err(|e| e.to_string())?;
    std::fs::write(path, data).map_err(|e| e.to_string())
}

pub fn read(path: &Path) -> Result<Backup, String> {
    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let backup: Backup = serde_json::from_str(&data).map_err(|e| format!("это не файл ORVYN: {e}"))?;
    if backup.is_empty() {
        return Err("в файле нет данных для восстановления".to_string());
    }
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_roundtrip() {
        let mut b = Backup::new();
        b.bookmarks = Some(vec![storage::Bookmark {
            url: "https://example.com".to_string(),
            title: "Example".to_string(),
        }]);
        b.session = Some(vec![storage::SessionTab {
            url: "https://example.com".to_string(),
            title: "Example".to_string(),
            pinned: false,
            group: "Учёба".to_string(),
        }]);
        let text = serde_json::to_string(&b).unwrap();
        let back: Backup = serde_json::from_str(&text).unwrap();
        assert_eq!(back.app, "ORVYN");
        assert_eq!(back.bookmarks.as_ref().unwrap().len(), 1);
        assert_eq!(back.session.as_ref().unwrap().len(), 1);
        assert!(!back.is_empty());
        assert!(Backup::default().is_empty());
    }
}
