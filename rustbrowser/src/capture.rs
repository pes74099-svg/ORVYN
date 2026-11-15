//! Сохранение страницы: полный скриншот и офлайн-снимок MHTML.

use std::path::PathBuf;

/// Декодирует стандартный base64, который отдаёт DevTools.
pub fn b64_decode(src: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = src.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks_exact(4) {
        let a = val(chunk[0])?;
        let b = val(chunk[1])?;
        let c = if chunk[2] == b'=' { 0 } else { val(chunk[2])? };
        let d = if chunk[3] == b'=' { 0 } else { val(chunk[3])? };
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            out.push((b << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            out.push((c << 6) | d);
        }
    }
    Some(out)
}

fn stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn free_path(dir: PathBuf, ext: &str) -> PathBuf {
    let base = format!("orvyn-{}.{}", stamp(), ext);
    let mut p = dir.join(&base);
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("orvyn-{}-{}.{}", stamp(), n, ext));
        n += 1;
    }
    p
}

/// Папка для снимков страницы: Downloads\ORVYN.
pub fn downloads_dir() -> PathBuf {
    let base = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("ORVYN");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Папка для скриншотов: Pictures\ORVYN.
pub fn pictures_dir() -> PathBuf {
    let base = dirs::picture_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("ORVYN");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn save_png(b64: &str) -> Result<PathBuf, String> {
    let data = b64_decode(b64).ok_or_else(|| "битые данные PNG".to_string())?;
    let p = free_path(pictures_dir(), "png");
    std::fs::write(&p, data).map_err(|e| e.to_string())?;
    Ok(p)
}

pub fn save_mhtml(data: &str) -> Result<PathBuf, String> {
    let p = free_path(downloads_dir(), "mhtml");
    std::fs::write(&p, data.as_bytes()).map_err(|e| e.to_string())?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::b64_decode;

    #[test]
    fn base64_roundtrip() {
        assert_eq!(b64_decode("T1JWWU4=").as_deref(), Some(&b"ORVYN"[..]));
        assert_eq!(b64_decode("YQ==").as_deref(), Some(&b"a"[..]));
        assert_eq!(b64_decode("YWI=").as_deref(), Some(&b"ab"[..]));
        assert_eq!(b64_decode("YWJj").as_deref(), Some(&b"abc"[..]));
        assert!(b64_decode("!!!").is_none());
    }
}
