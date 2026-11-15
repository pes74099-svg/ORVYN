//! Локальный сейф паролей: создание, разблокировка, записи, заполнение, экспорт.
//!
//! Вынесено из app.rs; шифрование живёт в vault.rs, здесь только команды интерфейса.

use super::*;

impl App {
    pub(super) fn vault_create(&mut self, password: &str, repeat: &str) {
        if password.chars().count() < 8 {
            self.vault_error = "Мастер-пароль — минимум 8 символов.".to_string();
            self.refresh();
            return;
        }
        if password != repeat {
            self.vault_error = "Пароли не совпадают.".to_string();
            self.refresh();
            return;
        }
        let mut password = password.to_string();
        let res = vault::create(&password);
        password.zeroize();
        match res {
            Ok(v) => {
                self.vault = Some(v);
                self.vault_exists = true;
                self.vault_error.clear();
                self.vault_last_use = Instant::now();
                self.panel = Some("passwords".to_string());
                self.status = "Сейф создан. Мастер-пароль нигде не сохранён — не забудь его.".to_string();
                self.layout();
            }
            Err(e) => {
                self.vault_error = e.clone();
                self.status = e;
            }
        }
        self.refresh();
    }

    pub(super) fn vault_unlock(&mut self, password: &str) {
        let mut password = password.to_string();
        let res = vault::open(&password);
        password.zeroize();
        match res {
            Ok(v) => {
                self.vault = Some(v);
                self.vault_exists = true;
                self.vault_error.clear();
                self.vault_last_use = Instant::now();
                self.status = "Сейф открыт.".to_string();
            }
            Err(e) => {
                self.vault_error = e.clone();
                self.status = e;
            }
        }
        self.refresh();
    }

    pub(super) fn vault_lock(&mut self, msg: &str) {
        self.vault = None;
        self.vault_last_use = Instant::now();
        self.status = msg.to_string();
        self.refresh();
    }

    pub(super) fn vault_save_entry(
        &mut self,
        id: u64,
        origin: &str,
        title: &str,
        username: &str,
        password: &str,
    ) {
        if self.vault.is_none() {
            self.panel = Some("passwords".to_string());
            self.status = "Сначала открой сейф.".to_string();
            self.layout();
            self.refresh();
            return;
        }
        let origin = vault::normalize_origin(origin);
        if origin.is_empty() {
            self.vault_error = "Нужен адрес сайта вида https://example.com.".to_string();
            self.refresh();
            return;
        }
        if password.is_empty() {
            self.vault_error = "Пароль пустой.".to_string();
            self.refresh();
            return;
        }
        let title = if title.trim().is_empty() {
            origin.clone()
        } else {
            title.trim().chars().take(80).collect::<String>()
        };
        let entry = vault::Credential {
            id,
            origin: origin.clone(),
            title: title.clone(),
            username: username.trim().chars().take(160).collect(),
            password: password.to_string(),
            updated: storage::now(),
        };
        let saved = if let Some(v) = self.vault.as_mut() {
            v.upsert(entry);
            v.save()
        } else {
            Err("сейф закрыт".to_string())
        };
        self.vault_last_use = Instant::now();
        match saved {
            Ok(()) => {
                self.vault_error.clear();
                self.status = format!("Пароль для {origin} сохранён.");
            }
            Err(e) => {
                self.vault_error = e.clone();
                self.status = e;
            }
        }
        self.refresh();
    }

    pub(super) fn vault_delete(&mut self, id: u64) {
        let res = if let Some(v) = self.vault.as_mut() {
            if v.remove_credential(id) {
                v.save()
            } else {
                Ok(())
            }
        } else {
            Err("сейф закрыт".to_string())
        };
        self.vault_last_use = Instant::now();
        match res {
            Ok(()) => self.status = "Пароль удалён.".to_string(),
            Err(e) => {
                self.vault_error = e.clone();
                self.status = e;
            }
        }
        self.refresh();
    }

    pub(super) fn vault_fill(&mut self, id: u64) {
        let Some(v) = self.vault.as_ref() else {
            self.panel = Some("passwords".to_string());
            self.status = "Сначала открой сейф.".to_string();
            self.layout();
            self.refresh();
            return;
        };
        let Some(e) = v.entries.iter().find(|e| e.id == id).cloned() else {
            return;
        };
        let Some(tab) = self.current_tab() else {
            return;
        };
        let tab_id = tab.id;
        let Some(origin) = security::origin_of(&tab.url) else {
            self.status = "Заполнить можно только на обычной странице http(s).".to_string();
            self.refresh();
            return;
        };
        if !e.origin.eq_ignore_ascii_case(&origin) {
            self.status = format!(
                "Пароль сохранён для {}, а открыт {}. Заполнение запрещено.",
                e.origin, origin
            );
            self.refresh();
            return;
        }
        let user = serde_json::to_string(&e.username).unwrap_or_else(|_| "\"\"".to_string());
        let pass = serde_json::to_string(&e.password).unwrap_or_else(|_| "\"\"".to_string());
        let js = format!(
            r#"(() => {{
  const user = {user}, pass = {pass};
  const setv = (el, v) => {{
    if (!el) return false;
    const proto = el.constructor && el.constructor.prototype;
    const d = proto ? Object.getOwnPropertyDescriptor(proto, "value") : null;
    if (d && d.set) d.set.call(el, v); else el.value = v;
    el.dispatchEvent(new Event("input", {{bubbles:true}}));
    el.dispatchEvent(new Event("change", {{bubbles:true}}));
    return true;
  }};
  const p = document.querySelector('input[type=password]');
  const form = p && p.form ? p.form : document;
  const u = form.querySelector('input[type=email],input[autocomplete=username],input[name*="user" i],input[name*="login" i],input[type=text]');
  const a = user ? setv(u, user) : false;
  const b = pass ? setv(p, pass) : false;
  const f = b ? p : (a ? u : null);
  if (f) f.focus();
}})();"#
        );
        if let Some(wv) = self
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.webview.as_ref())
        {
            let _ = wv.evaluate_script(&js);
            self.page_toast(tab_id, "Логин и пароль подставлены");
        }
        self.panel = None;
        self.layout();
        self.status = format!("Пароль для {} подставлен.", e.origin);
        self.vault_last_use = Instant::now();
        self.refresh();
    }

    pub(super) fn vault_fill_current(&mut self) {
        if self.vault.is_none() {
            self.panel = Some("passwords".to_string());
            self.status = if self.vault_exists {
                "Введи мастер-пароль, чтобы открыть сейф.".to_string()
            } else {
                "Сейф ещё не создан.".to_string()
            };
            self.layout();
            self.refresh();
            return;
        }
        let Some(origin) = self
            .current_tab()
            .and_then(|t| security::origin_of(&t.url))
        else {
            self.status = "Заполнить можно только на обычной странице http(s).".to_string();
            self.refresh();
            return;
        };
        let ids: Vec<u64> = self
            .vault
            .as_ref()
            .map(|v| {
                v.entries
                    .iter()
                    .filter(|e| e.origin.eq_ignore_ascii_case(&origin))
                    .map(|e| e.id)
                    .collect()
            })
            .unwrap_or_default();
        match ids.len() {
            0 => {
                self.panel = Some("passwords".to_string());
                self.status = format!("Для {origin} паролей нет. Добавь в панели паролей.");
                self.layout();
                self.refresh();
            }
            1 => self.vault_fill(ids[0]),
            _ => {
                self.panel = Some("passwords".to_string());
                self.status = format!("Для {origin} несколько записей — выбери нужную.");
                self.layout();
                self.refresh();
            }
        }
    }

    pub(super) fn vault_export(&mut self) {
        if !vault::exists() {
            self.status = "Сейф ещё не создан.".to_string();
            self.refresh();
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .set_title("Сохранить сейф ORVYN")
            .set_file_name("orvyn-vault.json")
            .add_filter("ORVYN vault", &["json"])
            .save_file()
        else {
            self.status = "Экспорт сейфа отменён.".to_string();
            self.refresh();
            return;
        };
        self.status = match std::fs::copy(vault::path(), &path) {
            Ok(_) => format!("Сейф сохранён: {}. Файл зашифрован.", path.display()),
            Err(e) => format!("Не удалось сохранить сейф: {e}"),
        };
        self.refresh();
    }

    pub(super) fn vault_import(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Открыть сейф ORVYN")
            .add_filter("ORVYN vault", &["json"])
            .pick_file()
        else {
            self.status = "Импорт сейфа отменён.".to_string();
            self.refresh();
            return;
        };
        self.status = match vault::install_file(&path) {
            Ok(()) => {
                self.vault = None;
                self.vault_exists = true;
                self.vault_error.clear();
                self.panel = Some("passwords".to_string());
                self.layout();
                "Сейф импортирован. Открой его мастер-паролем от того файла.".to_string()
            }
            Err(e) => format!("Не удалось импортировать сейф: {e}"),
        };
        self.refresh();
    }

    pub(super) fn vault_reset(&mut self) {
        self.status = match vault::remove() {
            Ok(()) => {
                self.vault = None;
                self.vault_exists = false;
                self.vault_error.clear();
                "Сейф удалён с этого ПК.".to_string()
            }
            Err(e) => e,
        };
        self.refresh();
    }

}
