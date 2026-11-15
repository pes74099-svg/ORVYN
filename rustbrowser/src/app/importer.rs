//! Импорт из других браузеров: поиск профилей, закладки, история и CSV паролей.
//!
//! Вынесено из app.rs; чтение баз и CSV живёт в import.rs.

use super::*;

impl App {
    /// Ищет профили Chrome, Edge, Firefox и других браузеров.
    pub(super) fn scan_import(&mut self) {
        self.import_targets = import::detect();
        self.status = if self.import_targets.is_empty() {
            "Браузеры для импорта не найдены.".to_string()
        } else {
            format!("Найдено профилей: {}.", self.import_targets.len())
        };
    }

    /// Импортирует закладки и историю из выбранного профиля.
    pub(super) fn import_run(&mut self, id: &str) {
        let Some(browser) = self.import_targets.iter().find(|b| b.id == id).cloned() else {
            self.status = "Профиль не найден. Открой список заново.".to_string();
            self.refresh();
            return;
        };
        match import::read(&browser) {
            Ok(data) => {
                let mut added_bookmarks = 0usize;
                let mut seen_bookmarks: HashSet<String> = self
                    .store
                    .bookmarks
                    .iter()
                    .map(|b| b.url.to_lowercase())
                    .collect();
                for bookmark in data.bookmarks {
                    if seen_bookmarks.insert(bookmark.url.to_lowercase()) {
                        self.store.bookmarks.push(bookmark);
                        added_bookmarks += 1;
                    }
                }

                let mut added_history = 0usize;
                if !data.history.is_empty() {
                    let mut seen_history: HashSet<String> = self
                        .store
                        .history
                        .iter()
                        .map(|h| h.url.to_lowercase())
                        .collect();
                    for entry in data.history {
                        if seen_history.insert(entry.url.to_lowercase()) {
                            self.store.history.push(entry);
                            added_history += 1;
                        }
                    }
                    self.store.history.sort_by_key(|h| h.ts);
                    if self.store.history.len() > 2000 {
                        let cut = self.store.history.len() - 2000;
                        self.store.history.drain(0..cut);
                    }
                }
                if added_bookmarks + added_history > 0 {
                    if added_bookmarks > 0 {
                        crate::db::replace_bookmarks(&self.store.bookmarks);
                    }
                    if added_history > 0 {
                        crate::db::replace_history(&self.store.history);
                    }
                    storage::save(&self.store);
                }
                let mut text = format!(
                    "{}: +{} закладок, +{} истории.",
                    browser.name, added_bookmarks, added_history
                );
                if !data.warnings.is_empty() {
                    text.push_str(&format!(" Не удалось: {}.", data.warnings.join("; ")));
                }
                self.status = text;
            }
            Err(e) => self.status = format!("{}: {e}", browser.name),
        }
        self.refresh();
    }

    /// Импортирует пароли из CSV-файла в открытый сейф.
    pub(super) fn import_passwords_csv(&mut self) {
        if self.vault.is_none() {
            self.status = "Сначала открой сейф паролей, потом повтори импорт.".to_string();
            self.panel = Some("passwords".to_string());
            self.layout();
            self.refresh();
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .set_title("Импорт паролей из CSV")
            .add_filter("CSV", &["csv"])
            .pick_file()
        else {
            return;
        };
        let mut text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                self.status = format!("Не удалось прочитать CSV: {e}");
                self.refresh();
                return;
            }
        };
        let rows = match import::parse_passwords_csv(&text) {
            Ok(r) => r,
            Err(e) => {
                text.zeroize();
                self.status = e;
                self.refresh();
                return;
            }
        };
        text.zeroize();

        let mut added = 0usize;
        let mut updated = 0usize;
        let mut skipped = 0usize;
        let mut save_error = None;
        if let Some(v) = self.vault.as_mut() {
            for mut row in rows {
                let origin = vault::normalize_origin(&row.origin);
                if origin.is_empty() || row.password.is_empty() {
                    skipped += 1;
                    row.password.zeroize();
                    continue;
                }
                let existing = v
                    .entries
                    .iter()
                    .find(|e| e.origin == origin && e.username == row.username)
                    .map(|e| e.id);
                let title = if row.title.trim().is_empty() {
                    origin.clone()
                } else {
                    row.title.trim().chars().take(80).collect::<String>()
                };
                let entry = vault::Credential {
                    id: existing.unwrap_or(0),
                    origin,
                    title,
                    username: row.username.trim().chars().take(160).collect(),
                    password: row.password.clone(),
                    updated: 0,
                };
                v.upsert(entry);
                if existing.is_some() {
                    updated += 1;
                } else {
                    added += 1;
                }
                row.password.zeroize();
            }
            save_error = v.save().err();
        }
        self.vault_last_use = Instant::now();
        self.status = match save_error {
            Some(e) => format!("Пароли импортированы, но сейф не сохранился: {e}"),
            None => format!(
                "Пароли: +{added}, обновлено {updated}, пропущено {skipped}. Удали CSV-файл."
            ),
        };
        self.refresh();
    }
}
