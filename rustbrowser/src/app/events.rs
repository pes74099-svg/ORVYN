//! События рантайма: загрузки, страницы, AI, прокси, WARP, снимки и закрытие.
//!
//! Вынесено из app.rs, чтобы цикл событий оставался читаемым.

use super::*;

impl App {
    pub(super) fn on_user_event(&mut self, ev: UserEvent) {
        match ev {
            UserEvent::Chrome(raw) => self.on_chrome(&raw),
            UserEvent::Page(id, raw) => self.on_page(id, &raw),
            UserEvent::Loaded { id, url, done } => {
                let mut hist = None;
                let mut incognito = false;
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                    if !url.is_empty() && !url.starts_with("about:blank") {
                        t.url = url.clone();
                    }
                    if url.starts_with("http") {
                        t.kind = TabKind::Web;
                    }
                    t.loading = !done;
                    if !done {
                        t.heap_mb = 0.0;
                    }
                    incognito = t.incognito;
                    if done {
                        hist = Some((t.url.clone(), t.title.clone()));
                    }
                }
                if done {
                    self.apply_site_policy(id);
                }
                if let Some((u, ti)) = hist {
                    if !incognito {
                        storage::add_history(&mut self.store, &u, &ti);
                    }
                }
                self.refresh();
            }
            UserEvent::Title { id, title } => {
                let mut changed = false;
                let mut incognito = false;
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                    t.title = title.clone();
                    incognito = t.incognito;
                    if let Some(last) = self.store.history.last_mut() {
                        if last.url == t.url && last.title != title {
                            last.title = title;
                            changed = true;
                        }
                    }
                }
                if changed && !incognito {
                    storage::save(&self.store);
                }
                self.refresh();
            }
            UserEvent::NewWindow { url, proxied, incognito } => {
                let s = &self.store.settings;
                let music = s.proxy_on && s.proxy_ready() && music::is_music_url(&url);
                self.new_tab_full(Some(url), true, proxied || music, false, incognito);
            }
            UserEvent::ProxyTest(result) => {
                self.proxy_status = result;
                self.refresh();
            }
            UserEvent::Usage(host, n) => {
                self.usage.push((host, n));
                self.usage_pending = self.usage_pending.saturating_sub(1);
                if self.usage_pending == 0 {
                    self.usage_scanning = false;
                    self.usage.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                    self.usage.retain(|(_, n)| *n > 0);
                    self.usage.truncate(8);
                    self.status = if self.usage.is_empty() {
                        "Сайты не занимают заметного места.".to_string()
                    } else {
                        "Подсчёт места сайтов готов.".to_string()
                    };
                    self.refresh();
                }
            }
            UserEvent::CleanSize(n) => {
                self.clean_size = n;
                self.clean_scanning = false;
                self.refresh();
            }
            UserEvent::ShotSize { id, width, height } => {
                self.capture_page(id, width, height);
            }
            UserEvent::Saved { kind, path, id } => {
                self.status = format!("{kind}: {path}");
                self.page_toast(id, &format!("{kind}: {path}"));
                self.refresh();
            }
            UserEvent::SaveError(msg) => {
                self.status = msg;
                self.refresh();
            }
            UserEvent::Quit => {}
            UserEvent::WarpStatus(label, reason) => {
                let prev = self.warp_status.clone();
                if label == "Connected" {
                    if prev != "Connected" || self.warp_since.is_none() {
                        self.warp_since = Some(Instant::now());
                    }
                } else {
                    self.warp_since = None;
                }
                if label == "Missing" || label == "Error" {
                    if !reason.is_empty() {
                        self.warp_error = reason.clone();
                    }
                } else if label == "Connected" {
                    self.warp_error.clear();
                }
                self.warp_status = label.clone();
                if self.warp_auto_pending {
                    if label == "Connected" {
                        self.warp_auto_pending = false;
                    } else if matches!(label.as_str(), "Disconnected" | "Unknown" | "Error") {
                        self.warp_auto_pending = false;
                        self.warp_action("connect");
                    } else if label == "Missing" {
                        self.warp_auto_pending = false;
                    }
                }
                if self.panel.as_deref() == Some("warp") {
                    self.refresh();
                }
            }
            UserEvent::WarpAction(res) => {
                self.warp_busy = false;
                match res {
                    Ok(msg) => {
                        self.warp_error.clear();
                        self.status = msg;
                        self.warp_check();
                    }
                    Err(e) => {
                        self.warp_error = e.clone();
                        self.status = e;
                    }
                }
                self.refresh();
            }
            UserEvent::AiReply(res) => {
                self.ai_busy = false;
                match res {
                    Ok(r) => {
                        if !r.model.is_empty() {
                            self.ai_model = r.model;
                        }
                        self.ai_history.push(storage::AiMessage {
                            role: "assistant".to_string(),
                            content: r.text,
                        });
                        if self.ai_history.len() > 40 {
                            let cut = self.ai_history.len() - 40;
                            self.ai_history.drain(0..cut);
                        }
                        self.ai_persist();
                        self.ai_error.clear();
                        self.status = "ORVYN AI ответил.".to_string();
                    }
                    Err(e) => {
                        self.ai_error = e.clone();
                        self.status = e;
                    }
                }
                self.refresh();
            }
            UserEvent::AiPageText(text) => {
                let text = text.trim().to_string();
                if text.is_empty() {
                    self.ai_error = "На странице нет текста, который можно взять.".to_string();
                } else {
                    let count = text.chars().count();
                    self.ai_context = Some(text.chars().take(12_000).collect());
                    self.ai_error.clear();
                    self.status = format!(
                        "Контекст страницы: {count} символов. Он уйдёт только со следующим вопросом."
                    );
                }
                self.refresh();
            }
            UserEvent::Update(res) => {
                self.update_checking = false;
                if let Ok(r) = &res {
                    if update::is_newer(env!("CARGO_PKG_VERSION"), &r.tag) {
                        self.status = format!("Доступна новая версия {}.", r.tag);
                    }
                }
                self.update_result = Some(res);
                self.refresh();
            }
            UserEvent::BadSite { id, url, reason } => {
                let light = self.store.settings.light();
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                    t.url = url.clone();
                    t.title = "Опасный сайт".to_string();
                    t.kind = TabKind::Web;
                    t.loading = false;
                    if let Some(wv) = t.webview.as_ref() {
                        let _ = wv.load_html(&security::warning_page(&url, reason.as_deref(), light));
                    }
                }
                self.panel = None;
                self.layout();
                self.refresh();
            }
            UserEvent::SearchDone { id, q, html } => {
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                    if t.kind != TabKind::Search(q) {
                        return;
                    }
                    if let Some(wv) = t.webview.as_ref() {
                        let _ = wv.load_html(&html);
                    }
                    t.loading = false;
                }
                self.refresh();
            }
            UserEvent::Download(msg) => {
                match msg {
                    downloads::Msg::Started { id, url, path } => {
                        let name = std::path::Path::new(&path)
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| {
                                url.rsplit('/').next().unwrap_or("файл").to_string()
                            });
                        self.downloads.push(DownloadItem {
                            id,
                            url,
                            name,
                            path,
                            bytes: 0,
                            total: -1,
                            speed: 0,
                            state: "progress".to_string(),
                            reason: String::new(),
                        });
                        self.refresh_downloads(true);
                    }
                    downloads::Msg::Danger { id, url, path, reason } => {
                        if let Some(d) = self.downloads.iter_mut().find(|d| d.id == id) {
                            d.state = "blocked".to_string();
                            d.speed = 0;
                            d.reason = reason;
                        } else {
                            let name = std::path::Path::new(&path)
                                .file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .filter(|s| !s.is_empty())
                                .unwrap_or_else(|| {
                                    url.rsplit('/').next().unwrap_or("файл").to_string()
                                });
                            self.downloads.push(DownloadItem {
                                id,
                                url,
                                name,
                                path,
                                bytes: 0,
                                total: -1,
                                speed: 0,
                                state: "blocked".to_string(),
                                reason,
                            });
                        }
                        self.refresh_downloads(true);
                    }
                    downloads::Msg::Progress { id, bytes, total, speed, state } => {
                        let mut done = false;
                        if let Some(d) = self.downloads.iter_mut().find(|d| d.id == id) {
                            d.bytes = bytes;
                            d.total = total;
                            if speed > 0 {
                                d.speed = speed;
                            }
                            if state == "completed" {
                                d.state = "completed".to_string();
                                d.speed = 0;
                                if total > 0 {
                                    d.bytes = total as u64;
                                }
                                done = true;
                            } else if state == "paused" {
                                if d.state != "blocked" {
                                    d.state = "paused".to_string();
                                }
                                d.speed = 0;
                            } else if state == "canceled" {
                                d.state = "canceled".to_string();
                                d.speed = 0;
                                done = true;
                            } else if state == "interrupted" {
                                d.state = "interrupted".to_string();
                                d.speed = 0;
                                done = true;
                            } else if d.state != "paused" && d.state != "blocked" {
                                d.state = "progress".to_string();
                            }
                        }
                        if done {
                            downloads::forget(id);
                        }
                        self.refresh_downloads(done);
                    }
                }
            }
        }
    }
}
