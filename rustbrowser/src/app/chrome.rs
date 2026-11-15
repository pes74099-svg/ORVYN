//! Команды из интерфейса браузера (chrome WebView).
//!
//! Вынесено из app.rs: разбор одной большой строки команд стал отдельным модулем.

use super::*;

impl App {
    pub(super) fn on_chrome(&mut self, raw: &str) {
        let Ok(v) = serde_json::from_str::<Value>(raw) else { return };
        let cmd = v["cmd"].as_str().unwrap_or("").to_string();
        match cmd.as_str() {
            "newTab" => {
                let url = v["url"].as_str().map(|s| s.to_string());
                let incog = self.current_tab().map(|t| t.incognito).unwrap_or(false);
                self.new_tab_full(url, true, false, false, incog);
            }
            "newIncognito" => self.new_incognito(),
            "closeTab" => {
                let id = v["id"]
                    .as_u64()
                    .or_else(|| self.current_tab().map(|t| t.id as u64))
                    .unwrap_or(0);
                self.close_tab(id as usize);
            }
            "selectTab" => {
                if let Some(id) = v["id"].as_u64() {
                    if let Some(pos) = self.tabs.iter().position(|t| t.id == id as usize) {
                        self.select(pos);
                    }
                }
            }
            "pinTab" => {
                if let Some(id) = v["id"].as_u64() {
                    self.toggle_pin(id as usize);
                }
            }
            "muteTab" => {
                if let Some(id) = v["id"].as_u64() {
                    self.toggle_mute(id as usize);
                }
            }
            "dupTab" => {
                if let Some(id) = v["id"].as_u64() {
                    self.duplicate_tab(id as usize);
                }
            }
            "quit" => {
                let _ = self.proxy.send_event(UserEvent::Quit);
            }
            "windowMin" => {
                self.window.set_minimized(true);
            }
            "windowMax" => self.toggle_maximize(),
            "windowClose" => self.request_close(),
            "windowDrag" => {
                let _ = self.window.drag_window();
            }
            "findInPage" => self.find_in_page(),
            "copySelection" => self.copy_selection(),
            "copyLinks" => self.copy_links(),
            "reader" => self.reader_mode(),
            "translate" => {
                let lang = v["lang"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| self.store.settings.translate_lang.clone());
                self.translate_page(&lang);
            }
            "qr" => self.show_qr(),
            "screenshot" => self.screenshot(),
            "savePage" => self.save_page(),
            "reopenTab" => self.reopen_closed(),
            "navigate" => {
                let text = v["text"].as_str().unwrap_or("").to_string();
                self.navigate(&text);
            }
            "back" => {
                if let Some(wv) = self.current_tab().and_then(|t| t.webview.as_ref()) {
                    let _ = wv.go_back();
                }
                self.refresh();
            }
            "forward" => {
                if let Some(wv) = self.current_tab().and_then(|t| t.webview.as_ref()) {
                    let _ = wv.go_forward();
                }
                self.refresh();
            }
            "reload" => self.reload_active(),
            "home" => {
                let home = self.store.settings.homepage.clone();
                self.navigate(&home);
            }
            "bookmark" => self.toggle_bookmark(),
            "panel" => {
                let name = v["name"].as_str().unwrap_or("").to_string();
                self.panel = match name.as_str() {
                    "" => None,
                    other => {
                        if self.panel.as_deref() == Some(other) {
                            None
                        } else {
                            Some(other.to_string())
                        }
                    }
                };
                if self.panel.as_deref() == Some("monitor") {
                    self.sample_stats();
                }
                if self.panel.as_deref() == Some("clean") {
                    self.scan_profile_size();
                }
                if self.panel.as_deref() == Some("passwords") {
                    self.vault_last_use = Instant::now();
                }
                if self.panel.as_deref() == Some("import") {
                    self.scan_import();
                }
                self.layout();
                self.refresh();
            }
            "checkUpdate" => {
                if !self.update_checking {
                    self.update_checking = true;
                    self.update_result = None;
                    self.status = "Проверяю обновления…".to_string();
                    let proxy = self.proxy.clone();
                    std::thread::spawn(move || {
                        let _ = proxy.send_event(UserEvent::Update(update::check()));
                    });
                }
                self.refresh();
            }
            "openUpdate" => {
                if let Some(Ok(r)) = &self.update_result {
                    let url = r
                        .setup_url
                        .clone()
                        .unwrap_or_else(|| r.url.clone());
                    if !url.is_empty() {
                        self.panel = None;
                        self.new_tab_full(Some(url), true, false, false, false);
                    }
                }
            }
            "syncPick" => self.sync_pick(),
            "syncNow" => self.sync_now(),
            "cycleTab" => {
                self.cycle(v["delta"].as_i64().unwrap_or(1));
            }
            "delBookmark" => {
                if let Some(url) = v["url"].as_str() {
                    self.store.bookmarks.retain(|b| b.url != url);
                    crate::db::remove_bookmark(url);
                    storage::save(&self.store);
                }
                self.refresh();
            }
            "clearHistory" => self.clear_data("history"),
            "devtools" => {
                if let Some(wv) = self.current_tab().and_then(|t| t.webview.as_ref()) {
                    wv.open_devtools();
                }
                self.panel = None;
                self.layout();
                self.refresh();
            }
            "userscripts" => {
                let dir = adblock::userscript_dir();
                let _ = std::fs::create_dir_all(&dir);
                let _ = adblock::user_scripts();
                let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                self.panel = None;
                self.layout();
                self.refresh();
            }
            "music" => {
                self.panel = if self.panel.as_deref() == Some("music") {
                    None
                } else {
                    Some("music".to_string())
                };
                self.layout();
                self.refresh();
            }
            "proxy" => {
                self.save_proxy_form(&v);
                if self.store.settings.proxy_on && !self.store.settings.proxy_ready() {
                    self.store.settings.proxy_on = false;
                    self.proxy_status = "Не хватает адреса или порта — прокси выключен.".to_string();
                    storage::save(&self.store);
                } else if self.store.settings.proxy_on {
                    self.proxy_status = "Прокси включён: музыка пойдёт через него.".to_string();
                } else {
                    self.proxy_status = "Прокси выключен.".to_string();
                }
                self.panel = Some("music".to_string());
                self.layout();
                self.refresh();
            }
            "proxyTest" => {
                self.save_proxy_form(&v);
                if let Some((_, host, port)) = self.store.settings.proxy_parts() {
                    self.proxy_status = "Проверяю прокси…".to_string();
                    let proxy = self.proxy.clone();
                    std::thread::spawn(move || {
                        let _ = proxy.send_event(UserEvent::ProxyTest(probe_proxy(&host, &port)));
                    });
                } else {
                    self.proxy_status = "Укажи сервер и порт.".to_string();
                }
                self.panel = Some("music".to_string());
                self.refresh();
            }
            "toggle" => {
                let name = v["name"].as_str().unwrap_or("").to_string();
                let current = match name.as_str() {
                    "snow" => self.store.settings.snow,
                    "glow" => self.store.settings.glow,
                    "anim" => self.store.settings.anim,
                    "restore" => self.store.settings.restore_session,
                    "memSaver" => self.store.settings.memory_saver,
                    "hw" => self.store.settings.hw_accel,
                    "bg" => self.store.settings.bg_limit,
                    "permNotif" => self.store.settings.perm_notifications,
                    "permGeo" => self.store.settings.perm_geolocation,
                    "permCam" => self.store.settings.perm_camera,
                    "permMic" => self.store.settings.perm_microphone,
                    "protection" => self.store.settings.protection_on,
                    "redirectGuard" => self.store.settings.redirect_guard,
                    "dlGuard" => self.store.settings.download_guard,
                    "tabMetrics" => self.store.settings.tab_metrics,
                    "lazyTabs" => self.store.settings.lazy_tabs,
                    "autoClean" => self.store.settings.auto_clean,
                    "cleanHistory" => self.store.settings.clean_history,
                    "cleanCache" => self.store.settings.clean_cache,
                    "cleanCookies" => self.store.settings.clean_cookies,
                    "closeWarn" => self.store.settings.close_warn,
                    "bkBookmarks" => self.store.settings.backup_bookmarks,
                    "bkSettings" => self.store.settings.backup_settings,
                    "bkHistory" => self.store.settings.backup_history,
                    "bkSession" => self.store.settings.backup_session,
                    "bkSpaces" => self.store.settings.backup_spaces,
                    "bkSets" => self.store.settings.backup_sets,
                    "bkReplace" => self.store.settings.backup_replace,
                    "toolbarHidden" => self.store.settings.toolbar_hidden,
                    "panelLeft" => self.store.settings.panel_left,
                    "addressRow" => self.store.settings.address_row,
                    "warpAuto" => self.store.settings.warp_auto,
                    "aiOn" => self.store.settings.ai_on,
                    "aiSave" => self.store.settings.ai_save,
                    "mouseGestures" => self.store.settings.mouse_gestures,
                    "bookmarksBar" => self.store.settings.bookmarks_bar,
                    _ => return,
                };
                self.apply_toggle(&name, !current);
            }
            "set" => {
                let key = v["key"].as_str().unwrap_or("").to_string();
                self.set_setting(&key, &v["value"]);
            }
            "clear" => {
                let what = v["what"].as_str().unwrap_or("").to_string();
                self.clear_data(&what);
            }
            "cleanNow" => self.clean_selected(),
            "cleanUsage" => self.scan_usage(),
            "spaceNew" => {
                let name = v["name"].as_str().unwrap_or("").to_string();
                let icon = v["icon"].as_str().unwrap_or("star").to_string();
                self.workspace_new(&name, &icon);
            }
            "spaceOpen" => {
                if let Some(id) = v["id"].as_u64() {
                    self.workspace_open(id);
                }
            }
            "spaceSave" => self.workspace_save(),
            "tabGroup" => {
                if let Some(id) = v["id"].as_u64() {
                    let name = v["name"].as_str().unwrap_or("").to_string();
                    self.set_tab_group(id as usize, &name);
                }
            }
            "groupRename" => {
                let old = v["old"].as_str().unwrap_or("").to_string();
                let new = v["name"].as_str().unwrap_or("").to_string();
                self.rename_group(&old, &new);
            }
            "groupClear" => {
                let name = v["name"].as_str().unwrap_or("").to_string();
                self.clear_group(&name);
            }
            "groupFocus" => {
                let name = v["name"].as_str().unwrap_or("").to_string();
                self.focus_group(&name);
            }
            "setSave" => {
                let name = v["name"].as_str().unwrap_or("").to_string();
                self.saved_set_save(&name);
            }
            "setOpen" => {
                let name = v["name"].as_str().unwrap_or("").to_string();
                self.saved_set_open(&name);
            }
            "setRename" => {
                let old = v["old"].as_str().unwrap_or("").to_string();
                let new = v["name"].as_str().unwrap_or("").to_string();
                self.saved_set_rename(&old, &new);
            }
            "setDelete" => {
                let name = v["name"].as_str().unwrap_or("").to_string();
                self.saved_set_delete(&name);
            }
            "backupExport" => self.backup_export(),
            "backupImport" => self.backup_import(),
            "importScan" => {
                self.scan_import();
                self.refresh();
            }
            "importRun" => {
                let id = v["id"].as_str().unwrap_or("").to_string();
                self.import_run(&id);
            }
            "importPasswords" => self.import_passwords_csv(),
            "themeSave" => {
                let name = v["name"].as_str().unwrap_or("").to_string();
                self.theme_save(&name);
            }
            "themeApply" => {
                let name = v["name"].as_str().unwrap_or("").to_string();
                self.theme_apply(&name);
            }
            "themeDelete" => {
                let name = v["name"].as_str().unwrap_or("").to_string();
                self.theme_delete(&name);
            }
            "themeReset" => self.theme_reset(),
            "themeExport" => self.theme_export(),
            "themeImport" => self.theme_import(),
            "themePreset" => {
                if let Some(id) = v["id"].as_str() {
                    self.theme_preset_apply(id);
                }
            }
            "warp" => {
                let action = v["action"].as_str().unwrap_or("check").to_string();
                if action == "check" {
                    self.warp_check();
                } else {
                    self.warp_action(&action);
                }
            }
            "warpCheck" => self.warp_check(),
            "warpOpen" => self.warp_open(),
            "warpInstall" => self.warp_install(),
            "vaultCreate" => {
                let password = v["password"].as_str().unwrap_or("");
                let repeat = v["repeat"].as_str().unwrap_or("");
                self.vault_create(password, repeat);
            }
            "vaultUnlock" => {
                let password = v["password"].as_str().unwrap_or("");
                self.vault_unlock(password);
            }
            "vaultLock" => self.vault_lock("Сейф заблокирован."),
            "vaultSave" => {
                let id = v["id"].as_u64().unwrap_or(0);
                let origin = v["origin"].as_str().unwrap_or("").to_string();
                let title = v["title"].as_str().unwrap_or("").to_string();
                let username = v["username"].as_str().unwrap_or("").to_string();
                let password = v["password"].as_str().unwrap_or("").to_string();
                self.vault_save_entry(id, &origin, &title, &username, &password);
            }
            "vaultDelete" => {
                if let Some(id) = v["id"].as_u64() {
                    self.vault_delete(id);
                }
            }
            "vaultFill" => {
                if let Some(id) = v["id"].as_u64() {
                    self.vault_fill(id);
                }
            }
            "vaultFillCurrent" => self.vault_fill_current(),
            "vaultExport" => self.vault_export(),
            "vaultImport" => self.vault_import(),
            "vaultReset" => self.vault_reset(),
            "aiSend" => {
                let text = v["text"].as_str().unwrap_or("").to_string();
                self.ai_send(&text);
            }
            "aiContext" => self.ai_context_grab(),
            "aiClear" => self.ai_clear(),
            "aiClearContext" => self.ai_clear_context(),
            "spaceRename" => {
                if let Some(id) = v["id"].as_u64() {
                    let name = v["name"].as_str().unwrap_or("").to_string();
                    self.workspace_rename(id, &name);
                }
            }
            "spaceIcon" => {
                if let Some(id) = v["id"].as_u64() {
                    self.workspace_cycle_icon(id);
                }
            }
            "spaceDelete" => {
                if let Some(id) = v["id"].as_u64() {
                    self.workspace_delete(id);
                }
            }
            "reset" => self.reset_settings(),
            "resetSection" => {
                if let Some(section) = v["section"].as_str() {
                    self.reset_settings_section(&section);
                }
            }
            "siteProtect" => self.toggle_site_protection(),
            "sitePerm" => {
                let name = v["name"].as_str().unwrap_or("").to_string();
                let on = v["on"].as_bool().unwrap_or(false);
                self.toggle_site_perm(&name, on);
            }
            "clearSite" => self.clear_site_data(),
            "checkSite" => self.quick_check(),
            "discardTab" => {
                if let Some(id) = v["id"].as_u64() {
                    self.discard_by_id(id as usize);
                }
            }
            "discardIdle" => self.discard_idle_now(),
            "zoom" => {
                let d = v["delta"].as_f64().unwrap_or(0.0);
                self.zoom_by(d);
            }
            "dlFolder" => self.open_downloads_folder(),
            "download" => {
                let id = v["id"].as_u64().unwrap_or(0);
                let action = v["action"].as_str().unwrap_or("");
                self.download_action(id, action);
            }
            "closeDownloads" => {
                self.panel = None;
                self.layout();
                self.refresh();
            }
            "focusBar" => {
                let _ = self.chrome.focus();
                let _ = self.chrome.evaluate_script("window.focusBar && window.focusBar();");
            }
            _ => {}
        }
    }
}
