//! Музыка: распознавание музыкальных сайтов и прокси-режим для обхода
//! блокировок по региону.
//!
//! Прокси не «встроенный» и не бесплатный: пользователь указывает свой
//! SOCKS5/HTTP-сервер (например, из VPN или арендованный), а ORVYN открывает
//! через него только музыкальные вкладки. Обычные сайты идут напрямую.

use crate::storage::Settings;

/// Сайты, которые считаются музыкальными. Для них включается прокси-режим,
/// кнопка «Регион» и фоновая игра.
pub const MUSIC_HOSTS: &[&str] = &[
    "soundcloud.com",
    "soundcloud.app.goo.gl",
    "music.yandex.ru",
    "music.yandex.com",
    "music.yandex.kz",
    "music.yandex.by",
    "music.yandex.uz",
    "music.youtube.com",
    "open.spotify.com",
    "deezer.com",
    "deezer.page.link",
    "bandcamp.com",
    "audiomack.com",
    "music.apple.com",
    "radio.garden",
    "zvuk.com",
    "tidal.com",
    "listen.tidal.com",
];

fn authority(url: &str) -> (&str, &str) {
    let rest = match url.find("://") {
        Some(i) => &url[i + 3..],
        None => url,
    };
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let host = match hostport.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') && p.chars().all(|c| c.is_ascii_digit()) => h,
        _ => hostport,
    };
    (host, path)
}

/// Похож ли адрес на музыкальный сайт.
pub fn is_music_url(url: &str) -> bool {
    let u = url.trim();
    let lower = u.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return false;
    }
    let (raw_host, path) = authority(u);
    let host = raw_host.trim_end_matches('.').to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let path = path.to_ascii_lowercase();

    // ВК: музыка живёт на /audio и /music, остальной ВК — обычный сайт.
    if host == "vk.com" || host.ends_with(".vk.com") {
        return path.starts_with("/audio") || path.starts_with("/music");
    }
    // Яндекс: старый адрес yandex.ru/music.
    if host == "yandex.ru" || host == "yandex.com" {
        return path.starts_with("/music");
    }

    MUSIC_HOSTS
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// Тег прокси для папки данных WebView2. Разные прокси должны жить в разных
/// папках: WebView2 не разрешает два окружения с одинаковым каталогом и
/// разными параметрами.
pub fn proxy_tag(s: &Settings) -> String {
    let Some((kind, host, port)) = s.proxy_parts() else {
        return "off".to_string();
    };
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in format!("{kind}|{}|{port}", host.to_ascii_lowercase()).bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_music_sites() {
        assert!(is_music_url("https://soundcloud.com/artist/track"));
        assert!(is_music_url("https://music.yandex.ru/album/1"));
        assert!(is_music_url("https://music.youtube.com/watch?v=1"));
        assert!(is_music_url("https://open.spotify.com/track/1"));
        assert!(is_music_url("https://artist.bandcamp.com/album/x"));
        assert!(is_music_url("https://vk.com/audio?z=audio_1"));
        assert!(is_music_url("https://yandex.ru/music"));
        assert!(is_music_url("https://zvuk.com/release/123"));
    }

    #[test]
    fn ignores_regular_sites() {
        assert!(!is_music_url("https://youtube.com/watch?v=1"));
        assert!(!is_music_url("https://vk.com/feed"));
        assert!(!is_music_url("https://example.com/soundcloud.com"));
        assert!(!is_music_url("orvyn://newtab"));
        assert!(!is_music_url(""));
    }

    #[test]
    fn proxy_tag_changes_with_settings() {
        let mut s = Settings::default();
        assert_eq!(proxy_tag(&s), "off");
        s.proxy_on = true;
        s.proxy_host = "127.0.0.1".into();
        s.proxy_port = "1080".into();
        let a = proxy_tag(&s);
        s.proxy_port = "1081".into();
        let b = proxy_tag(&s);
        assert_ne!(a, "off");
        assert_ne!(a, b);
    }
}
