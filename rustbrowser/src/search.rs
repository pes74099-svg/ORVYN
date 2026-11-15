//! Своя поисковая выдача.
//!
//! Запрос уходит из Rust (через системный curl), страница результатов рисуется
//! нами же — поэтому в адресной строке и в интерфейсе не видно, какой движок
//! использован.

use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Не показывать консольное окно при запуске curl.exe из GUI-приложения.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const TEMPLATE: &str = include_str!("../assets/search.html");

const LOADING: &str = r#"<!doctype html><html lang="ru"><head><meta charset="utf-8"><title>Поиск</title>
<style>
html,body{margin:0;height:100%;background:#070a10;color:#e9eefb;font-family:"Segoe UI",system-ui,sans-serif}
.wrap{height:100%;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:18px}
.spin{width:34px;height:34px;border-radius:50%;border:3px solid #1a2334;border-top-color:#4fd1ff;animation:s .8s linear infinite}
@keyframes s{to{transform:rotate(360deg)}}
.q{color:#8d9ab3;font-size:14px;max-width:60vw;text-align:center;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
</style></head><body><div class="wrap"><div class="spin"></div><div class="q">Ищем: {{QUERY}}</div></div></body></html>"#;

pub struct Hit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

pub fn loading_page(q: &str) -> String {
    LOADING.replace("{{QUERY}}", &esc(q))
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h = hex(bytes[i + 1]);
            let l = hex(bytes[i + 2]);
            if let (Some(h), Some(l)) = (h, l) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn unescape(s: &str) -> String {
    let mut out = s
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");
    // Числовые сущности (их в выдаче может быть несколько).
    while let Some(pos) = out.find("&#") {
        let after = &out[pos + 2..];
        let Some(end) = after.find(';') else { break };
        let num = &after[..end];
        let code = if let Some(stripped) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
            u32::from_str_radix(stripped, 16).ok()
        } else {
            num.parse::<u32>().ok()
        };
        let Some(ch) = code.and_then(char::from_u32) else { break };
        let mut buf = [0u8; 4];
        out.replace_range(pos..pos + 2 + end + 1, ch.encode_utf8(&mut buf));
    }
    out
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut inside = false;
    for ch in s.chars() {
        match ch {
            '<' => inside = true,
            '>' => inside = false,
            c if !inside => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn http_get(url: &str) -> Option<String> {
    let mut cmd = Command::new("curl.exe");
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd
        .args([
            "-sL",
            "--max-time",
            "15",
            "--compressed",
            "-A",
            UA,
            "-H",
            "Accept-Language: ru-RU,ru;q=0.9,en;q=0.8",
            url,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

fn markers(html: &str) -> Vec<usize> {
    let pats = [
        "class=\"result__a\"",
        "class='result-link'",
        "class=\"result-link\"",
        "class='result__a'",
    ];
    let mut v: Vec<usize> = Vec::new();
    for p in pats {
        let mut start = 0;
        while let Some(i) = html[start..].find(p) {
            v.push(start + i);
            start = start + i + p.len();
        }
    }
    v.sort_unstable();
    v.dedup();
    v
}

fn parse(html: &str) -> Vec<Hit> {
    let marks = markers(html);
    let mut hits = Vec::new();
    for (n, &m) in marks.iter().enumerate() {
        let limit = marks.get(n + 1).copied().unwrap_or(html.len());
        let seg = &html[m..limit];

        let Some(href_i) = seg.find("href=\"") else { continue };
        let after = &seg[href_i + 6..];
        let Some(q_end) = after.find('"') else { continue };
        let raw_href = unescape(&after[..q_end]);

        let url = if let Some(u) = extract_uddg(&raw_href) {
            u
        } else if raw_href.starts_with("http") {
            raw_href.clone()
        } else if raw_href.starts_with("//") {
            format!("https:{raw_href}")
        } else {
            continue;
        };
        if url.contains("duckduckgo.com/y.js") || url.contains("ad_domain=") {
            continue;
        }

        let Some(gt) = after.find('>') else { continue };
        let title_html = &after[gt + 1..];
        let Some(close) = title_html.find("</a>") else { continue };
        let title = unescape(&strip_tags(&title_html[..close]));
        if title.is_empty() {
            continue;
        }

        let mut snippet = String::new();
        if let Some(sp) = title_html[close..].find("snippet") {
            let s2 = &title_html[close + sp..];
            if let Some(g2) = s2.find('>') {
                let rest = &s2[g2 + 1..];
                let end = rest.find("</a>").or_else(|| rest.find("</td>")).unwrap_or(rest.len().min(400));
                snippet = unescape(&strip_tags(&rest[..end]));
            }
        }

        hits.push(Hit { title, url, snippet });
        if hits.len() >= 30 {
            break;
        }
    }
    hits
}

/// Разбор выдачи Brave Search (у DDG слишком часто вылезает капча).
fn parse_brave(html: &str) -> Vec<Hit> {
    const MARK: &str = "class=\"snippet ";
    let mut hits = Vec::new();
    let mut start = 0;
    while let Some(i) = html[start..].find(MARK) {
        let m = start + i;
        let limit = html[m + MARK.len()..]
            .find(MARK)
            .map(|j| m + MARK.len() + j)
            .unwrap_or(html.len());
        let seg = &html[m..limit];
        start = limit;

        let Some(hi) = seg.find("href=\"") else { continue };
        let after = &seg[hi + 6..];
        let Some(qe) = after.find('"') else { continue };
        let url = unescape(&after[..qe]);
        if !url.starts_with("http") {
            continue;
        }

        let Some(ti) = seg.find("class=\"title search-snippet-title") else { continue };
        let tseg = &seg[ti..];
        let title = if let Some(ai) = tseg.find("title=\"") {
            let a2 = &tseg[ai + 7..];
            match a2.find('"') {
                Some(q2) => unescape(&a2[..q2]),
                None => String::new(),
            }
        } else if let Some(gt) = tseg.find('>') {
            let rest = &tseg[gt + 1..];
            let end = rest.find("</div>").unwrap_or(rest.len().min(200));
            unescape(&strip_tags(&rest[..end]))
        } else {
            String::new()
        };
        if title.is_empty() {
            continue;
        }

        let mut snippet = String::new();
        if let Some(si) = seg.find("class=\"generic-snippet") {
            let s2 = &seg[si..];
            if let Some(ci) = s2.find("class=\"content") {
                let c2 = &s2[ci..];
                if let Some(gt) = c2.find('>') {
                    let rest = &c2[gt + 1..];
                    let end = rest.find("</div>").unwrap_or(rest.len().min(400));
                    snippet = unescape(&strip_tags(&rest[..end]));
                }
            }
        }

        hits.push(Hit { title, url, snippet });
        if hits.len() >= 30 {
            break;
        }
    }
    hits
}

fn extract_uddg(href: &str) -> Option<String> {
    let i = href.find("uddg=")?;
    let rest = &href[i + 5..];
    let end = rest.find('&').unwrap_or(rest.len());
    Some(percent_decode(&rest[..end]))
}

pub fn results_page(query: &str, hits: &[Hit], error: Option<&str>, light: bool) -> String {
    let mut list = String::new();
    if let Some(e) = error {
        list.push_str(&format!(
            "<div class=\"err\">Не удалось получить результаты: {}</div>",
            esc(e)
        ));
    }
    for (i, h) in hits.iter().enumerate() {
        let host = host_of(&h.url);
        let letter = host.chars().next().unwrap_or('•').to_ascii_uppercase();
        list.push_str(&format!(
            "<div class=\"res\"><div class=\"idx\">{i}</div><div class=\"body\">\
             <div class=\"host\"><span class=\"fav\">{fav}</span>{host}</div>\
             <a class=\"title\" href=\"{url}\">{title}</a>\
             <div class=\"snip\">{snip}</div></div></div>",
            i = i + 1,
            fav = esc(&letter.to_string()),
            host = esc(&host),
            url = esc(&h.url),
            title = esc(&h.title),
            snip = esc(&h.snippet),
        ));
    }
    if hits.is_empty() && error.is_none() {
        list.push_str("<div class=\"err\">Ничего не нашлось. Попробуй переформулировать запрос.</div>");
    }

    TEMPLATE
        .replace("{{QUERY}}", &esc(query))
        .replace("{{COUNT}}", &hits.len().to_string())
        .replace("{{RESULTS}}", &list)
        .replace("{{THEME}}", if light { "light" } else { "dark" })
}

fn host_of(url: &str) -> String {
    let no_scheme = url.splitn(2, "://").nth(1).unwrap_or(url);
    let host = no_scheme.split('/').next().unwrap_or("");
    host.trim_start_matches("www.").to_string()
}

pub fn search(query: &str) -> Result<Vec<Hit>, String> {
    let q = encode(query);
    let sources = [
        ("brave", format!("https://search.brave.com/search?q={q}&source=web")),
        ("ddg", format!("https://html.duckduckgo.com/html/?q={q}&kl=ru-ru")),
        ("ddg", format!("https://lite.duckduckgo.com/lite/?q={q}")),
        ("ddg", format!("https://html.duckduckgo.com/html/?q={q}")),
    ];
    for (kind, url) in sources {
        if let Some(html) = http_get(&url) {
            let hits = if kind == "brave" {
                parse_brave(&html)
            } else {
                parse(&html)
            };
            if !hits.is_empty() {
                return Ok(hits);
            }
        }
    }
    Err("нет ответа от поискового бэкенда".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_search_returns_hits() {
        match search("rust programming language") {
            Ok(hits) => {
                println!("найдено: {}", hits.len());
                for h in hits.iter().take(5) {
                    println!("- {} | {} | {}", h.title, h.url, h.snippet.chars().take(60).collect::<String>());
                }
                assert!(!hits.is_empty(), "выдача пустая");
            }
            Err(e) => panic!("поиск упал: {e}"),
        }
    }

    #[test]
    fn page_renders() {
        let hits = vec![Hit { title: "<Test & Co>".into(), url: "https://example.com/a?b=1".into(), snippet: "x".into() }];
        let html = results_page("тест", &hits, None, false);
        assert!(html.contains("example.com"));
        assert!(!html.contains("{{"));
    }

    #[test]
    fn brave_results_parse() {
        let html = r#"<div class="snippet svelte-x" data-pos="1">
            <div class="result-body"><div class="result-wrapper"><div class="result-content">
            <a href="https://rust-lang.org/" target="_self" class="l1">
              <div class="title search-snippet-title line-clamp-1 svelte-1" title="Rust Programming Language">Rust Programming Language</div>
            </a>
            <div class="generic-snippet svelte-2"><div class="content svelte-3 line-clamp">A language empowering everyone.</div></div>
            </div></div></div></div>
            <div class="snippet svelte-x" data-pos="2">
            <div class="result-content"><a href="https://en.wikipedia.org/wiki/Rust" class="l1">
              <div class="title search-snippet-title" title="Rust (programming language)">Rust (programming language)</div>
            </a></div></div>"#;
        let hits = parse_brave(html);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].url, "https://rust-lang.org/");
        assert_eq!(hits[0].title, "Rust Programming Language");
        assert!(hits[0].snippet.contains("empowering"));
        assert_eq!(hits[1].title, "Rust (programming language)");
    }
}
