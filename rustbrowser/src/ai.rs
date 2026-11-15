//! ORVYN AI: бесплатный облачный ассистент без ключей.
//!
//! Используется публичный бесплатный API Pollinations (анонимный тариф).
//! Запрос уходит из Rust, а не из страницы, поэтому ключей в браузере нет.
//! Быстрый GET-режим отвечает заметно быстрее JSON-чата и не тратит токены
//! на длинные рассуждения. Сервис внешний: возможны лимиты и перерывы.

use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const ENDPOINT: &str = "https://text.pollinations.ai/";
pub const MODEL: &str = "openai-fast";
pub const MODEL_LABEL: &str = "GPT-OSS 20B Fast";

const MAX_URL_CHARS: usize = 9_000;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct Reply {
    pub text: String,
    pub model: String,
}

pub fn ask(messages: &[Message]) -> Result<Reply, String> {
    let prompt = build_prompt(messages);
    if prompt.trim().is_empty() {
        return Err("Пустой запрос к AI.".to_string());
    }
    let mut url = format!("{ENDPOINT}{}?model={MODEL}", encode(&prompt));
    if url.chars().count() > MAX_URL_CHARS {
        let short: String = prompt.chars().take(2_500).collect();
        url = format!("{ENDPOINT}{}?model={MODEL}", encode(&short));
    }
    let response = ureq::get(&url)
        .timeout(Duration::from_secs(45))
        .set("User-Agent", "ORVYN/2.2")
        .call();
    match response {
        Ok(r) => {
            let status = r.status();
            let raw = r
                .into_string()
                .map_err(|e| format!("не удалось прочитать ответ AI: {e}"))?;
            if status != 200 {
                return Err(friendly(status, &raw));
            }
            parse_text(&raw)
        }
        Err(ureq::Error::Status(code, r)) => {
            let raw = r.into_string().unwrap_or_default();
            Err(friendly(code, &raw))
        }
        Err(e) => Err(format!(
            "нет связи с бесплатным AI: {e}. Проверь интернет и попробуй ещё раз."
        )),
    }
}

fn encoded_len(s: &str) -> usize {
    s.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => 1,
            b' ' => 3,
            _ => 3,
        })
        .sum()
}

fn build_prompt(messages: &[Message]) -> String {
    const BUDGET: usize = 7_000;
    let mut system = String::new();
    let mut tail: Vec<&Message> = Vec::new();
    for m in messages {
        if m.role == "system" {
            if !system.is_empty() {
                system.push('\n');
            }
            system.push_str(m.content.trim());
        } else {
            tail.push(m);
        }
    }
    let system: String = system.chars().take(1_500).collect();
    let mut out = String::new();
    if !system.is_empty() {
        out.push_str(system.trim());
        out.push_str("\n\n");
    }
    let start = out.len();
    let mut body = String::new();
    for m in tail.iter().rev() {
        let text = m.content.trim();
        if text.is_empty() {
            continue;
        }
        let label = if m.role == "assistant" {
            "Ассистент: "
        } else {
            "Пользователь: "
        };
        let room = BUDGET.saturating_sub(encoded_len(&out) + encoded_len(&body));
        let mut piece = format!("{label}{text}\n");
        if encoded_len(&piece) > room {
            if body.is_empty() && room > 400 {
                let take = (room / 3).max(120);
                piece = format!("{label}{}\n", text.chars().take(take).collect::<String>());
            } else {
                break;
            }
        }
        body.insert_str(0, &piece);
    }
    out.push_str(body.trim_end());
    if out.len() == start {
        // Нет реплик — не отправляем один системный текст.
        return String::new();
    }
    out.push('\n');
    out.push_str("Ассистент:");
    out
}

fn parse_text(raw: &str) -> Result<Reply, String> {
    let text = raw.trim();
    if text.is_empty() {
        return Err("AI вернул пустой ответ. Попробуй переформулировать вопрос.".to_string());
    }
    let low = text.to_ascii_lowercase();
    if low.starts_with("error") || low.starts_with("<html") {
        let short: String = text.chars().take(180).collect();
        return Err(format!("Бесплатный AI временно недоступен: {short}"));
    }
    let mut text = text.to_string();
    if text.chars().count() > 24_000 {
        text = text.chars().take(24_000).collect();
    }
    Ok(Reply {
        text,
        model: MODEL_LABEL.to_string(),
    })
}

fn friendly(code: u16, raw: &str) -> String {
    match code {
        429 => "Бесплатный лимит ORVYN AI исчерпан. Попробуй позже — ключи и платные тарифы не включаются сами.".to_string(),
        401 | 403 => "Бесплатный AI временно закрыл доступ. Попробуй позже.".to_string(),
        500..=599 => "Сервер бесплатного AI временно недоступен. Попробуй позже.".to_string(),
        414 => "Запрос к AI получился слишком длинным. Сократи текст и попробуй ещё раз.".to_string(),
        _ => {
            let body = raw.trim();
            let short: String = body.chars().take(180).collect();
            if short.is_empty() {
                format!("AI ответил ошибкой {code}.")
            } else {
                format!("AI ответил ошибкой {code}: {short}")
            }
        }
    }
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.as_bytes() {
        match *b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> Message {
        Message {
            role: role.to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn prompt_keeps_system_context_and_last_question() {
        let p = build_prompt(&[
            msg("system", "Ты помощник."),
            msg("user", "Первый вопрос"),
            msg("assistant", "Первый ответ"),
            msg("user", "Последний вопрос"),
        ]);
        assert!(p.starts_with("Ты помощник."));
        assert!(p.contains("Первый вопрос"));
        assert!(p.ends_with("Ассистент:"));
        assert!(p.contains("Последний вопрос"));
    }

    #[test]
    fn prompt_limits_old_history() {
        let long = "очень длинный текст ".repeat(1_000);
        let p = build_prompt(&[msg("user", &long), msg("user", "свежий вопрос")]);
        assert!(p.chars().count() < 4_500);
        assert!(p.contains("свежий вопрос"));
    }

    #[test]
    fn plain_text_reply_and_errors() {
        assert_eq!(parse_text("Привет!").unwrap().text, "Привет!");
        assert!(parse_text("  ").is_err());
        assert!(parse_text("<html>oops</html>").is_err());
        assert!(encode("привет тест").starts_with("%D0%BF"));
    }
}
