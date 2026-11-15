//! Загрузки: действия из интерфейса и обновление панели.
//!
//! Вынесено из app.rs; сам движок загрузок живёт в downloads.rs.

use super::*;

impl App {
    pub(super) fn download_action(&mut self, id: u64, action: &str) {
        if action == "resume" {
            let blocked = self
                .downloads
                .iter()
                .find(|d| d.id == id && d.state == "blocked")
                .map(|d| d.url.clone());
            if let Some(url) = blocked {
                downloads::allow_once(&url);
                self.downloads.retain(|d| d.id != id);
                if let Some(wv) = self.current_tab().and_then(|t| t.webview.as_ref()) {
                    let _ = wv.load_url(&url);
                }
                self.status = "Загрузка разрешена вручную, качается заново.".to_string();
                self.refresh();
                return;
            }
        }
        let Some(d) = self.downloads.iter_mut().find(|d| d.id == id) else { return };
        match action {
            "pause" => {
                if downloads::control(id, "pause") {
                    d.state = "paused".to_string();
                    d.speed = 0;
                }
            }
            "resume" => {
                if downloads::control(id, "resume") {
                    d.state = "progress".to_string();
                }
            }
            "cancel" => {
                if downloads::control(id, "cancel") {
                    d.state = "canceled".to_string();
                    d.speed = 0;
                    downloads::forget(id);
                }
            }
            "open" => {
                let path = d.path.clone();
                if !path.is_empty() {
                    let _ = std::process::Command::new("explorer").arg(path).spawn();
                }
            }
            "folder" => {
                let path = d.path.clone();
                if !path.is_empty() {
                    let _ = std::process::Command::new("explorer")
                        .arg(format!("/select,{path}"))
                        .spawn();
                }
            }
            "clear" => {
                self.downloads.retain(|d| {
                    !(d.state == "completed" || d.state == "canceled" || d.state == "interrupted")
                });
                downloads::forget(id);
            }
            _ => {}
        }
        self.refresh();
    }

    pub(super) fn refresh_downloads(&mut self, force: bool) {
        if self.panel.as_deref() != Some("downloads") && !force {
            return;
        }
        if force || self.last_dl_ui.elapsed() >= Duration::from_millis(300) {
            self.last_dl_ui = Instant::now();
            self.refresh();
        }
    }
}
