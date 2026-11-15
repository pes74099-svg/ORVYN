//! Локальная защита от вредоносных сайтов.
//!
//! WebView2 сам включает Microsoft SmartScreen, а здесь — небольшой список
//! известных тестовых/фишинговых доменов и предупреждающая страница. Список
//! намеренно короткий: лучше пропустить, чем ложно заблокировать нормальный сайт.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const BAD_HOSTS: [&str; 6] = [
    "malware.wicar.org",
    "www.malware.wicar.org",
    "testsafebrowsing.appspot.com",
    "malware.testing.google.test",
    "secure.eicar.org",
    "www.secure.eicar.org",
];

#[derive(Clone)]
pub struct Guard {
    allowed: Arc<Mutex<HashSet<String>>>,
    strict: Arc<AtomicBool>,
}

impl Default for Guard {
    fn default() -> Self {
        Self {
            allowed: Arc::new(Mutex::new(HashSet::new())),
            strict: Arc::new(AtomicBool::new(true)),
        }
    }
}

impl Guard {
    pub fn allow(&self, host: &str) {
        if let Ok(mut set) = self.allowed.lock() {
            set.insert(host.to_string());
        }
    }

    pub fn is_allowed(&self, host: &str) -> bool {
        self.allowed
            .lock()
            .map(|s| s.contains(host))
            .unwrap_or(false)
    }

    /// Включает/выключает предупреждения о подозрительных переходах.
    pub fn set_strict(&self, on: bool) {
        self.strict.store(on, Ordering::Relaxed);
    }

    /// true, если переход нужно отменить и показать предупреждение.
    pub fn blocks(&self, url: &str) -> bool {
        if !is_bad_url(url) {
            return false;
        }
        let Some(host) = host_of(url) else { return false };
        !self.is_allowed(&host)
    }

    /// Причина предупреждения о подозрительном (но не явно вредоносном) адресе.
    pub fn warn(&self, url: &str) -> Option<String> {
        if !self.strict.load(Ordering::Relaxed) {
            return None;
        }
        if let Some(host) = host_of(url) {
            if self.is_allowed(&host) {
                return None;
            }
        }
        suspicious_reason(url)
    }
}

pub fn host_of(url: &str) -> Option<String> {
    let rest = match url.split_once("://") {
        Some((scheme, rest)) => match scheme.to_ascii_lowercase().as_str() {
            "http" | "https" | "file" => rest,
            _ => return None,
        },
        None => url,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let hostport = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let host = match hostport.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') && p.chars().all(|c| c.is_ascii_digit()) => h,
        _ => hostport,
    };
    if host.is_empty() {
        None
    } else {
        Some(host.trim_end_matches('.').to_ascii_lowercase())
    }
}

/// Происхождение сайта для правил WebView2: `scheme://host[:port]`.
/// Порт важен: `http://127.0.0.1:8766` и `http://127.0.0.1` — разные сайты.
pub fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let authority = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    if authority.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{}", authority.to_ascii_lowercase()))
}

pub fn is_bad_host(host: &str) -> bool {
    BAD_HOSTS.iter().any(|b| host == *b || host.ends_with(&format!(".{b}")))
}

pub fn is_bad_url(url: &str) -> bool {
    match host_of(url) {
        Some(h) => is_bad_host(&h),
        None => false,
    }
}

fn authority_of(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    Some(rest.split(['/', '?', '#']).next().unwrap_or(rest))
}

/// Домен закодирован punycode (`xn--…`) и может выглядеть как чужой.
pub fn is_punycode_host(host: &str) -> bool {
    host.split('.').any(|label| label.starts_with("xn--"))
}

/// Публичный IP-адрес вместо нормального домена.
pub fn is_public_ip_host(host: &str) -> bool {
    use std::net::IpAddr;
    let h = host.trim_start_matches('[').trim_end_matches(']');
    match h.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified())
        }
        Ok(IpAddr::V6(v6)) => {
            let seg = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || (seg & 0xfe00) == 0xfc00
                || (seg & 0xffc0) == 0xfe80)
        }
        Err(_) => false,
    }
}

/// Причина, по которой переход стоит показать с предупреждением.
/// `None` — обычный адрес, ничего подозрительного.
pub fn suspicious_reason(url: &str) -> Option<String> {
    let authority = authority_of(url)?;
    let host = host_of(url)?;
    if authority.contains('@') {
        return Some("в адресе есть символ @: настоящий сайт может быть другим".to_string());
    }
    if is_punycode_host(&host) {
        return Some("домен записан в punycode (xn--) и может маскироваться под другой сайт".to_string());
    }
    if is_public_ip_host(&host) && url.to_ascii_lowercase().starts_with("http://") {
        return Some("сайт открывается по IP-адресу без домена и без HTTPS".to_string());
    }
    None
}

const DANGER_EXT: [&str; 18] = [
    "exe", "msi", "bat", "cmd", "com", "scr", "pif", "ps1", "vbs", "vbe", "jse", "jar", "hta",
    "cpl", "reg", "dll", "msix", "appx",
];

const DOC_EXT: [&str; 16] = [
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "jpg", "jpeg", "png", "gif", "txt", "zip",
    "rar", "mp3", "mp4",
];

/// Проверка файла перед загрузкой. Возвращает причину, если файл опасного типа.
/// Это предупреждение, а не приговор: пользователь может продолжить вручную.
pub fn dangerous_download(path: &str, url: &str) -> Option<String> {
    let name = path
        .rsplit(['\\', '/'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path);
    if name.chars().any(|c| c == '\u{202e}' || c == '\u{202d}') {
        return Some("в имени файла есть невидимый символ, меняющий направление текста".to_string());
    }
    let raw = if name.is_empty() {
        let clean = url.split(['?', '#']).next().unwrap_or(url);
        clean.rsplit('/').next().unwrap_or(clean)
    } else {
        name
    };
    let lower = raw.to_ascii_lowercase();
    let Some((stem, ext)) = lower.rsplit_once('.') else {
        return None;
    };
    if !DANGER_EXT.contains(&ext) {
        return None;
    }
    if let Some((_, inner)) = stem.rsplit_once('.') {
        if DOC_EXT.contains(&inner) {
            return Some(format!("двойное расширение .{inner}.{ext} — так маскируют программы"));
        }
    }
    Some(format!("исполняемый файл .{ext}"))
}

pub fn warning_page(url: &str, reason: Option<&str>, light: bool) -> String {
    let u = url
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    let why = reason
        .map(|r| {
            format!(
                "<div class=\"why\">{}</div>",
                r.replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
            )
        })
        .unwrap_or_default();
    let theme = if light { "light" } else { "dark" };
    format!(
        r#"<!doctype html><html lang="ru"><head><meta charset="utf-8"><title>Опасный сайт</title>
<style>
html,body{{margin:0;height:100%;background:#070a10;color:#e9eefb;font-family:"Segoe UI",system-ui,sans-serif}}
body.light{{background:#f4f7fb;color:#1b2436}}
.wrap{{min-height:100%;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:14px;padding:40px;text-align:center}}
.badge{{width:64px;height:64px;border-radius:20px;display:grid;place-items:center;background:rgba(255,107,129,.13);border:1px solid rgba(255,107,129,.4);color:#ff6b81;font-size:30px;font-weight:700}}
h1{{margin:6px 0 0;font-size:22px;font-weight:600}}
p{{margin:0;max-width:560px;color:#8d9ab3;line-height:1.6;font-size:13.5px}}
body.light p{{color:#5b6882}}
.why{{padding:8px 12px;border-radius:9px;background:rgba(255,107,129,.09);border:1px solid rgba(255,107,129,.3);color:#ffb3bf;font-size:12.5px;max-width:620px}}
body.light .why{{color:#a3243a;background:#fff2f4}}
.url{{margin-top:4px;padding:8px 12px;border-radius:9px;background:#0e131c;border:1px solid #1c2637;
      font:12px/1.4 Consolas,monospace;color:#bfe9ff;max-width:640px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}
body.light .url{{background:#fff;border-color:#dfe6f1;color:#0e6f9e}}
.row{{display:flex;gap:9px;margin-top:12px;flex-wrap:wrap;justify-content:center}}
button{{font:inherit;font-size:13px;padding:9px 16px;border-radius:10px;border:1px solid #223149;
        background:#142033;color:#b9d8ea;cursor:pointer}}
button:hover{{background:#1a2a42}}
button.primary{{background:linear-gradient(135deg,#4fd1ff,#8b7cff);border-color:transparent;color:#061019;font-weight:700}}
body.light button{{background:#eef3fa;border-color:#d9e2ef;color:#2c3c55}}
</style></head><body class="{theme}"><div class="wrap">
<div class="badge">!</div>
<h1>Сайт выглядит опасным</h1>
<p>ORVYN остановил переход: домен есть в локальном списке угроз, либо адрес выглядит подозрительно. Microsoft SmartScreen тоже проверяет такие страницы. Если ты уверен, что сайт безопасен, можно открыть его вручную.</p>
{why}
<div class="url">{u}</div>
<div class="row"><button class="primary" onclick="go()">Всё равно открыть</button><button onclick="home()">На новую вкладку</button></div>
</div>
<script>
function go(){{ try {{ window.ipc.postMessage(JSON.stringify({{ rb: "allowbad", url: {url_js} }})); }} catch (e) {{}} }}
function home(){{ try {{ window.ipc.postMessage(JSON.stringify({{ rb: "gonav", url: "orvyn://newtab" }})); }} catch (e) {{}} }}
</script></body></html>"#,
        theme = theme,
        u = u,
        why = why,
        url_js = serde_json::to_string(url).unwrap_or_else(|_| "\"\"".to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_parse() {
        assert_eq!(host_of("https://Example.COM/path?q=1").as_deref(), Some("example.com"));
        assert_eq!(host_of("http://user@a.b:8080/x").as_deref(), Some("a.b"));
        assert_eq!(host_of("orvyn://newtab"), None);
    }

    #[test]
    fn blocks_test_domains() {
        assert!(is_bad_url("https://malware.wicar.org/"));
        assert!(is_bad_url("http://sub.testsafebrowsing.appspot.com/x"));
        assert!(!is_bad_url("https://example.com/"));
        let g = Guard::default();
        assert!(g.blocks("https://malware.wicar.org/"));
        g.allow("malware.wicar.org");
        assert!(!g.blocks("https://malware.wicar.org/"));
    }

    #[test]
    fn suspicious_redirects() {
        assert!(suspicious_reason("https://xn--80ak6aa92e.com/").is_some());
        assert!(suspicious_reason("https://user@example.com/").is_some());
        assert!(suspicious_reason("http://8.8.8.8/").is_some());
        assert!(suspicious_reason("http://192.168.1.1/").is_none());
        assert!(suspicious_reason("https://example.com/").is_none());
        assert!(suspicious_reason("orvyn://newtab").is_none());
        let g = Guard::default();
        assert!(g.warn("https://xn--80ak6aa92e.com/").is_some());
        g.allow("xn--80ak6aa92e.com");
        assert!(g.warn("https://xn--80ak6aa92e.com/").is_none());
        g.set_strict(false);
        assert!(g.warn("https://user@example.com/").is_none());
    }

    #[test]
    fn origins_keep_port() {
        assert_eq!(origin_of("http://127.0.0.1:8766/x").as_deref(), Some("http://127.0.0.1:8766"));
        assert_eq!(origin_of("HTTPS://Example.COM:443/a?b#c").as_deref(), Some("https://example.com:443"));
        assert_eq!(origin_of("https://user@example.com/").as_deref(), Some("https://example.com"));
        assert_eq!(origin_of("orvyn://newtab"), None);
    }

    #[test]
    fn dangerous_downloads() {
        assert!(dangerous_download("C:\\Users\\x\\Downloads\\setup.exe", "").is_some());
        assert!(dangerous_download("", "https://a.b/file.msi?x=1").is_some());
        assert!(dangerous_download("C:\\x\\invoice.pdf.exe", "").unwrap().contains("двойное"));
        assert!(dangerous_download("C:\\x\\photo.jpg", "").is_none());
        assert!(dangerous_download("C:\\x\\archive.zip", "").is_none());
    }
}
