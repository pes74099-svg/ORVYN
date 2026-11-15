//! Диагностика WebView2Loader.dll.
//!
//! Наш exe импортирует `WebView2Loader.dll` жёстко (static import), поэтому
//! Windows обязана найти её до запуска `main`. Ошибка «система не обнаружила
//! WebView2Loader.dll» появляется именно там, и перехватить её из кода нельзя.
//!
//! Отсюда правило: DLL обязана лежать рядом с `ORVYN.exe`. Это делают установщик
//! и ZIP-релиз. На машине разработчика DLL случайно нашлась в PATH (папка
//! Cloudflare WARP), из-за чего проблема не проявлялась локально.
//!
//! Этот модуль не «лечит» отсутствие файла, а пишет в лог, какая копия
//! используется: рядом с exe или чужая из PATH. По логу сразу видно, если
//! установщик не доложил файл.

use std::path::PathBuf;

/// Имя файла, который обязан лежать рядом с exe.
pub const LOADER: &str = "WebView2Loader.dll";

/// Путь к загрузчику рядом с программой.
pub fn bundled() -> Option<PathBuf> {
    let path = std::env::current_exe().ok()?.parent()?.join(LOADER);
    path.is_file().then_some(path)
}

/// Загрузчики внутри установленного WebView2 Runtime, от новых к старым.
pub fn runtime_loaders() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in runtime_dirs() {
        for arch in ["x64", "arm64", "x86"] {
            let p = dir.join(arch).join(LOADER);
            if p.is_file() {
                out.push(p);
            }
        }
    }
    out
}

/// Все установленные версии WebView2 Runtime, от новых к старым.
pub fn runtime_dirs() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(v) = std::env::var("ProgramFiles(x86)") {
        roots.push(PathBuf::from(v).join("Microsoft").join("EdgeWebView"));
    }
    if let Ok(v) = std::env::var("ProgramFiles") {
        roots.push(PathBuf::from(v).join("Microsoft").join("EdgeWebView"));
    }
    if let Ok(v) = std::env::var("LOCALAPPDATA") {
        roots.push(PathBuf::from(v).join("Microsoft").join("EdgeWebView"));
    }

    let mut versions: Vec<(Vec<u64>, PathBuf)> = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root.join("Application")) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let key = version_key(&name);
            if key.is_empty() {
                continue;
            }
            let dir = entry.path().join("EBWebView");
            if dir.is_dir() {
                versions.push((key, dir));
            }
        }
    }
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions.into_iter().map(|(_, p)| p).collect()
}

/// Что записать в лог о загрузчике.
pub fn report() -> String {
    if let Some(p) = bundled() {
        return format!("рядом с программой: {}", p.display());
    }
    match runtime_loaders().into_iter().next() {
        Some(p) => format!(
            "рядом с exe файла нет, Windows взяла копию из WebView2 Runtime: {}",
            p.display()
        ),
        None => format!(
            "файла {LOADER} нет ни рядом с программой, ни в WebView2 Runtime — переустанови ORVYN"
        ),
    }
}

/// Есть ли на машине установленный Microsoft Edge WebView2 Runtime.
pub fn runtime_present() -> bool {
    !runtime_dirs().is_empty()
}

/// Номер версии из имени папки: «120.0.2210.91» → [120, 0, 2210, 91].
///
/// Пустой список означает «это не папка версии».
fn version_key(name: &str) -> Vec<u64> {
    if !name.starts_with(|c: char| c.is_ascii_digit()) {
        return Vec::new();
    }
    name.split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_key_orders_numerically() {
        assert!(version_key("120.0.2210.91") > version_key("99.0.1150.30"));
        assert_eq!(version_key("1.2.3"), vec![1, 2, 3]);
        assert!(version_key("Application").is_empty());
    }

    #[test]
    fn loader_name_is_plain_file_name() {
        assert_eq!(std::path::Path::new(LOADER).file_name().unwrap(), LOADER);
        assert!(!LOADER.contains(['\\', '/']));
    }
}
