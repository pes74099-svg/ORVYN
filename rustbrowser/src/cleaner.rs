//! ORVYN CLEAN: размер профиля и расписание автоочистки.

use std::path::Path;

/// Суммарный размер папки в байтах. Ошибки чтения пропускаются:
/// профиль WebView2 может меняться, пока браузер работает.
pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(rd) = std::fs::read_dir(path) else { return 0 };
    for entry in rd.flatten() {
        let p = entry.path();
        let Ok(md) = entry.metadata() else { continue };
        if md.is_dir() {
            total = total.saturating_add(dir_size(&p));
        } else {
            total = total.saturating_add(md.len());
        }
    }
    total
}

/// Размер основного профиля и музыкальных профилей WebView2.
pub fn profiles_size() -> u64 {
    let base = crate::storage::data_dir();
    let mut total = dir_size(&base.join("webview"));
    if let Ok(rd) = std::fs::read_dir(&base) {
        for entry in rd.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("webview-music-") && entry.path().is_dir() {
                total = total.saturating_add(dir_size(&entry.path()));
            }
        }
    }
    total
}

/// Пора ли запускать автоочистку. `last` — прошлая очистка в секундах Unix,
/// 0 — очистки ещё не было.
pub fn clean_due(last: u64, days: u32, now: u64) -> bool {
    if days == 0 {
        return false;
    }
    if last == 0 {
        return true;
    }
    now.saturating_sub(last) >= u64::from(days) * 86_400
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_works() {
        let day = 86_400u64;
        assert!(!clean_due(100, 0, 100 + day));
        assert!(clean_due(0, 7, 1_000));
        assert!(!clean_due(100, 7, 100 + 6 * day));
        assert!(clean_due(100, 7, 100 + 7 * day));
        assert!(clean_due(100, 7, 100 + 99 * day));
        assert!(!clean_due(500, 7, 100));
    }
}
