//! Проверка обновлений через GitHub Releases. Без серверов и подписок.
//!
//! Скачивание и установка остаются за пользователем: мы показываем версию,
//! заметки и открываем страницу релиза или файл установщика в браузере.

use serde_json::Value;
use std::time::Duration;

/// Репозиторий, откуда берутся релизы.
pub const REPO: &str = "pes74099-svg/ORVYN";

#[derive(Clone, Debug)]
pub struct Release {
    pub tag: String,
    pub name: String,
    pub url: String,
    pub notes: String,
    pub setup_url: Option<String>,
    pub size_mb: u64,
}

/// Сравнивает версии вида «3.0.0» и «v3.0.0-beta» без лишних зависимостей.
pub fn is_newer(current: &str, latest: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        v.trim_start_matches(['v', 'V'])
            .split(['-', '+'])
            .next()
            .unwrap_or("")
            .split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    }
    let a = parts(current);
    let b = parts(latest);
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return y > x;
        }
    }
    false
}

/// Запрашивает последний опубликованный релиз.
pub fn check() -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = ureq::get(&url)
        .set("User-Agent", "ORVYN-updater")
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(10))
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(404, _) => "релизы ещё не опубликованы".to_string(),
            ureq::Error::Status(code, _) => format!("GitHub ответил кодом {code}"),
            other => format!("нет связи с GitHub: {other}"),
        })?;
    let text = resp
        .into_string()
        .map_err(|e| format!("не смог прочитать ответ: {e}"))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("непонятный ответ GitHub: {e}"))?;

    let tag = v["tag_name"].as_str().unwrap_or("").to_string();
    if tag.is_empty() {
        return Err("в ответе нет версии".to_string());
    }
    let mut setup_url = None;
    let mut size_mb = 0u64;
    if let Some(assets) = v["assets"].as_array() {
        for a in assets {
            let name = a["name"].as_str().unwrap_or("");
            if name.starts_with("OrvynSetup") && name.ends_with(".exe") {
                setup_url = a["browser_download_url"].as_str().map(str::to_string);
                size_mb = a["size"].as_u64().unwrap_or(0) / (1024 * 1024);
                break;
            }
        }
    }

    Ok(Release {
        tag,
        name: v["name"].as_str().unwrap_or("ORVYN").to_string(),
        url: v["html_url"].as_str().unwrap_or("").to_string(),
        notes: v["body"].as_str().unwrap_or("").chars().take(600).collect(),
        setup_url,
        size_mb,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(is_newer("2.2.0", "3.0.0"));
        assert!(is_newer("2.2.0", "v2.2.1"));
        assert!(!is_newer("3.0.0", "3.0.0"));
        assert!(!is_newer("3.0.0", "2.9.9"));
        assert!(is_newer("3.0.0", "3.0.1-beta"));
        assert!(!is_newer("3.0.1", "3.0.1-beta"));
    }
}
