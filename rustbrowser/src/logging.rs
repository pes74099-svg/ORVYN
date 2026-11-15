//! Локальный лог ORVYN.
//!
//! Один файл `%APPDATA%\Orvyn\logs\orvyn.log`, ротация по 1 МБ, хранится 3 архива.
//! Никакой телеметрии и сетевых отправок: лог нужен, чтобы разбирать сбои на месте.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const MAX_BYTES: u64 = 1_000_000;
const KEEP: usize = 3;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Готовит каталог и файл лога, ставит перехват паник.
pub fn init(primary: bool) {
    let dir = crate::storage::data_dir().join("logs");
    if let Err(e) = fs::create_dir_all(&dir) {
        // Логировать некуда — молча выходим, браузер должен запуститься в любом случае.
        let _ = e;
        return;
    }
    let path = dir.join("orvyn.log");
    let _ = LOG_PATH.set(path.clone());
    rotate_if_needed(&path);
    install_panic_hook();
    if primary {
        info(&format!(
            "ORVYN {} запущен, pid {}",
            env!("CARGO_PKG_VERSION"),
            std::process::id()
        ));
        info(&format!("профиль: {}", crate::storage::data_dir().display()));
        info(&format!("лог: {}", path.display()));
    }
}

/// Путь к текущему файлу лога (для диагностики).
#[allow(dead_code)]
pub fn file_path() -> Option<PathBuf> {
    LOG_PATH.get().cloned()
}

pub fn info(msg: &str) {
    write_line("INFO", msg);
}

pub fn warn(msg: &str) {
    write_line("WARN", msg);
}

pub fn error(msg: &str) {
    write_line("ERROR", msg);
}

fn write_line(level: &str, msg: &str) {
    let Some(path) = LOG_PATH.get() else {
        return;
    };
    let line = format!("[{}] {:<5} {}\r\n", stamp(), level, msg);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}

fn rotate_if_needed(path: &Path) {
    let too_big = fs::metadata(path).map(|m| m.len() > MAX_BYTES).unwrap_or(false);
    if !too_big {
        return;
    }
    if KEEP == 0 {
        let _ = fs::remove_file(path);
        return;
    }
    let _ = fs::remove_file(old_path(path, KEEP));
    for i in (1..KEEP).rev() {
        let _ = fs::rename(old_path(path, i), old_path(path, i + 1));
    }
    let _ = fs::rename(path, old_path(path, 1));
}

fn old_path(path: &Path, n: usize) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(format!(".{n}"));
    PathBuf::from(s)
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "неизвестная паника".to_string()
        };
        let place = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "неизвестное место".to_string());
        error(&format!("ПАНИКА: {payload} | {place}"));
        let trace = std::backtrace::Backtrace::force_capture();
        error(&format!("backtrace:\r\n{trace}"));
        previous(info);
    }));
}

/// UTC-метка вида `2026-11-15 19:43:35Z` без внешних зависимостей.
fn stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

/// Гражданская дата из числа дней эпохи (алгоритм Говарда Хиннанта).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::civil_from_days;

    #[test]
    fn epoch_date() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_000), (2024, 10, 4));
    }
}
