//! Обработчики сообщений из веб-страниц и горячие клавиши.
//!
//! Вынесено из `app.rs`, чтобы основной файл не разрастался.
//! Доступ к полям `App` остаётся через родительский модуль.

use super::*;

impl App {
    pub(super) fn on_page(&mut self, id: usize, raw: &str) {
        let Ok(v) = serde_json::from_str::<Value>(raw) else { return };
        let kind = v["rb"].as_str().unwrap_or("");
        match kind {
            "blocked" => {
                self.blocked = self.blocked.saturating_add(1);
                if let Some(h) = v["host"].as_str() {
                    if !h.is_empty() {
                        *self.blocked_domains.entry(h.to_string()).or_insert(0) += 1;
                    }
                }
                if let Some(tab) = self.tabs.iter().find(|t| t.id == id) {
                    if let Some(h) = security::host_of(&tab.url) {
                        *self.blocked_site.entry(h).or_insert(0) += 1;
                    }
                }
                if self.last_blocked.elapsed().as_millis() > 250 {
                    self.last_blocked = Instant::now();
                    let js = format!("window.setBlocked && window.setBlocked({});", self.blocked);
                    let _ = self.chrome.evaluate_script(&js);
                    if self.panel.as_deref() == Some("shield") {
                        self.refresh();
                    }
                }
            }
            "heap" => {
                if let Some(mb) = v["mb"].as_f64() {
                    if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                        t.heap_mb = mb.max(0.0);
                    }
                }
            }
            "gesture" => {
                let action = v["action"].as_str().unwrap_or("");
                let Some(tab) = self.tabs.iter().find(|t| t.id == id) else { return };
                if let Some(wv) = tab.webview.as_ref() {
                    match action {
                        "back" => {
                            let _ = wv.go_back();
                        }
                        "forward" => {
                            let _ = wv.go_forward();
                        }
                        "top" => {
                            let _ = wv.evaluate_script("window.scrollTo({top:0,behavior:'smooth'})");
                        }
                        "bottom" => {
                            let _ = wv.evaluate_script(
                                "window.scrollTo({top:document.documentElement.scrollHeight,behavior:'smooth'})",
                            );
                        }
                        _ => {}
                    }
                }
                if action == "back" || action == "forward" {
                    self.refresh();
                }
            }
            "key" => {
                if let Some(combo) = v["combo"].as_str() {
                    self.hotkey(combo);
                }
            }
            "newtab" => {
                if let Some(url) = v["url"].as_str() {
                    let incog = self.tabs.iter().find(|t| t.id == id).map(|t| t.incognito).unwrap_or(false);
                    self.new_tab_full(Some(url.to_string()), false, false, false, incog);
                }
            }
            "proxytab" => {
                if let Some(url) = v["url"].as_str() {
                    if self.store.settings.proxy_ready() {
                        self.new_tab_full(Some(url.to_string()), true, true, false, false);
                    } else {
                        self.panel = Some("music".to_string());
                        self.layout();
                        self.refresh();
                    }
                }
            }
            "panel" => {
                if let Some(name) = v["name"].as_str() {
                    self.panel = Some(name.to_string());
                    if self.panel.as_deref() == Some("monitor") {
                        self.sample_stats();
                    }
                    self.layout();
                    self.refresh();
                }
            }
            "search" => {
                if let Some(q) = v["q"].as_str() {
                    if q.trim().is_empty() {
                        return;
                    }
                    if let Some(idx) = self.tabs.iter().position(|t| t.id == id) {
                        self.start_search_at(idx, q);
                    }
                }
            }
            "allowbad" => {
                if let Some(url) = v["url"].as_str() {
                    if let Some(host) = security::host_of(url) {
                        self.guard.allow(&host);
                    }
                    self.navigate(url);
                }
            }
            "gonav" => {
                if let Some(url) = v["url"].as_str() {
                    self.navigate(url);
                }
            }
            _ => {}
        }
    }

    pub(super) fn hotkey(&mut self, combo: &str) {
        match combo {
            "ctrl+t" => self.new_tab(None, true),
            "ctrl+shift+t" => self.reopen_closed(),
            "ctrl+shift+n" => self.new_incognito(),
            "ctrl+w" => {
                if let Some(t) = self.current_tab() {
                    let id = t.id;
                    self.close_tab(id);
                }
            }
            "ctrl+l" => {
                let _ = self.chrome.focus();
                let _ = self.chrome.evaluate_script("window.focusBar && window.focusBar();");
            }
            "ctrl+r" | "f5" => self.reload_active(),
            "ctrl+d" => self.toggle_bookmark(),
            "ctrl+tab" => self.cycle(1),
            "ctrl+shift+tab" => self.cycle(-1),
            "ctrl+shift+i" => {
                if let Some(wv) = self.current_tab().and_then(|t| t.webview.as_ref()) {
                    wv.open_devtools();
                }
            }
            "alt+arrowleft" => {
                if let Some(wv) = self.current_tab().and_then(|t| t.webview.as_ref()) {
                    let _ = wv.go_back();
                }
                self.refresh();
            }
            "alt+arrowright" => {
                if let Some(wv) = self.current_tab().and_then(|t| t.webview.as_ref()) {
                    let _ = wv.go_forward();
                }
                self.refresh();
            }
            "alt+home" => {
                let home = self.store.settings.homepage.clone();
                self.navigate(&home);
            }
            "ctrl+equal" | "ctrl++" | "ctrl+=" => self.zoom_by(0.1),
            "ctrl+-" => self.zoom_by(-0.1),
            "ctrl+0" => self.zoom_by(0.0),
            "ctrl+shift+d" => {
                if let Some(t) = self.current_tab() {
                    let id = t.id;
                    self.duplicate_tab(id);
                }
            }
            "ctrl+f" => self.find_in_page(),
            "ctrl+m" => {
                if let Some(t) = self.current_tab() {
                    let id = t.id;
                    self.toggle_mute(id);
                }
            }
            "ctrl+shift+a" => {
                self.panel = Some("tabs".to_string());
                self.layout();
                self.refresh();
            }
            "ctrl+shift+b" => {
                let next = !self.store.settings.toolbar_hidden;
                self.set_setting("toolbarHidden", &serde_json::Value::Bool(next));
            }
            "ctrl+shift+l" => self.vault_fill_current(),
            "alt+a" => {
                self.panel = Some("ai".to_string());
                self.layout();
                self.refresh();
            }
            _ => {}
        }
    }

    pub(super) fn zoom_by(&mut self, d: f64) {
        if let Some(t) = self.tabs.get_mut(self.active) {
            t.zoom = if d == 0.0 { 1.0 } else { (t.zoom + d).clamp(0.25, 5.0) };
            let z = t.zoom;
            if let Some(wv) = t.webview.as_ref() {
                let _ = wv.zoom(z);
            }
        }
        self.refresh();
    }
}
