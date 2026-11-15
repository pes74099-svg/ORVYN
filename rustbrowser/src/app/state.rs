//! Сборка состояния интерфейса: одно место, где Rust отдаёт данные в ui.html.
//!
//! Вынесено из app.rs для разгрузки основного файла.

use super::*;

impl App {
    pub(super) fn state(&self) -> Value {
        let tabs: Vec<Value> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                json!({
                    "id": t.id,
                    "title": t.title,
                    "url": t.url,
                    "active": i == self.active,
                    "loading": t.loading,
                    "pinned": t.pinned,
                    "incognito": t.incognito,
                    "muted": t.muted,
                    "sleeping": t.webview.is_none(),
                    "group": t.group,
                    "heap": (t.heap_mb * 10.0).round() / 10.0,
                    "idle": t.last_active.elapsed().as_secs() / 60,
                    "discardable": self.can_discard(i),
                })
            })
            .collect();

        let mut history: Vec<&storage::HistoryEntry> =
            self.store.history.iter().rev().take(150).collect();
        history.reverse();

        let cur = self.current_tab();
        let dl: Vec<Value> = self
            .downloads
            .iter()
            .map(|d| {
                json!({
                    "id": d.id,
                    "name": d.name,
                    "url": d.url,
                    "path": d.path,
                    "bytes": d.bytes,
                    "total": d.total,
                    "speed": d.speed,
                    "state": d.state,
                    "reason": d.reason,
                })
            })
            .collect();
        let sleeping = self.tabs.iter().filter(|t| t.webview.is_none()).count();
        let dl_active = self
            .downloads
            .iter()
            .filter(|d| d.state == "progress" || d.state == "paused" || d.state == "blocked")
            .count();

        let cur_url = cur.map(|t| t.url.clone()).unwrap_or_default();
        let cur_host = security::host_of(&cur_url).unwrap_or_default();
        let internal = cur_url.is_empty()
            || cur_url.starts_with("orvyn://")
            || cur_url.starts_with("rustbrowser://");
        let protected = self.store.protection_for(&cur_host);
        let perm_bits = self.store.perm_bits_for(&cur_host, self.perms.mask());
        let https = cur_url.starts_with("https://");
        let warning = security::suspicious_reason(&cur_url);
        let level = if internal {
            "idle"
        } else if warning.is_some() {
            "warn"
        } else if !protected {
            "off"
        } else if !https {
            "http"
        } else {
            "ok"
        };
        let sec_label = match level {
            "ok" => "Защита включена, соединение HTTPS",
            "http" => "Соединение без HTTPS",
            "off" => "Защита для этого сайта выключена",
            "warn" => "Адрес выглядит подозрительно",
            _ => "Внутренняя страница ORVYN",
        };
        let site_blocked = self.blocked_site.get(&cur_host).copied().unwrap_or(0);
        let mut top: Vec<(&String, &u32)> = self.blocked_domains.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let top_blocked: Vec<Value> = top
            .into_iter()
            .take(5)
            .map(|(h, n)| json!({ "host": h, "n": n }))
            .collect();
        let idle_limit = self.store.settings.idle_minutes as u64;
        let suggest_discard = idle_limit > 0
            && self.tabs.iter().enumerate().any(|(i, t)| {
                self.can_discard(i)
                    && t.last_active.elapsed() >= Duration::from_secs(idle_limit * 60)
            });
        let spaces: Vec<Value> = self
            .store
            .workspaces
            .iter()
            .map(|w| {
                json!({
                    "id": w.id,
                    "name": w.name,
                    "icon": w.icon,
                    "count": w.tabs.len(),
                    "active": self.store.workspace_active == Some(w.id),
                })
            })
            .collect();

        let sets: Vec<Value> = self
            .store
            .saved_sets
            .iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "count": s.tabs.len(),
                    "created": s.created,
                    "titles": s.tabs.iter().take(4).map(|t| t.title.clone()).collect::<Vec<_>>(),
                })
            })
            .collect();

        json!({
            "tabs": tabs,
            "url": cur.map(|t| t.url.clone()).unwrap_or_default(),
            "title": cur.map(|t| t.title.clone()).unwrap_or_default(),
            "canBack": cur.and_then(|t| t.webview.as_ref()).and_then(|w| w.can_go_back().ok()).unwrap_or(false),
            "canForward": cur.and_then(|t| t.webview.as_ref()).and_then(|w| w.can_go_forward().ok()).unwrap_or(false),
            "loading": cur.map(|t| t.loading).unwrap_or(false),
            "bookmarked": cur.map(|t| self.is_bookmarked(&t.url)).unwrap_or(false),
            "incognito": cur.map(|t| t.incognito).unwrap_or(false),
            "muted": cur.map(|t| t.muted).unwrap_or(false),
            "blocked": self.blocked,
            "siteBlocked": site_blocked,
            "topBlocked": top_blocked,
            "security": {
                "level": level,
                "label": sec_label,
                "host": cur_host,
                "https": https,
                "protected": protected,
                "internal": internal,
                "warning": warning,
                "perms": {
                    "notif": perm_bits & 1 != 0,
                    "geo": perm_bits & 2 != 0,
                    "cam": perm_bits & 4 != 0,
                    "mic": perm_bits & 8 != 0,
                },
            },
            "protectionOn": self.store.settings.protection_on,
            "redirectGuard": self.store.settings.redirect_guard,
            "downloadGuard": self.store.settings.download_guard,
            "mouseGestures": self.store.settings.mouse_gestures,
            "tabMetrics": self.store.settings.tab_metrics,
            "lazyTabs": self.store.settings.lazy_tabs,
            "lastFreed": (self.last_freed * 10.0).round() / 10.0,
            "suggestDiscard": suggest_discard,
            "cleanSize": self.clean_size,
            "cleanScanning": self.clean_scanning,
            "usage": self.usage.iter().map(|(h, n)| json!({ "host": h, "bytes": n })).collect::<Vec<_>>(),
            "usageScanning": self.usage_scanning,
            "lastClean": self.store.last_clean,
            "cleanHistory": self.store.settings.clean_history,
            "cleanCache": self.store.settings.clean_cache,
            "cleanCookies": self.store.settings.clean_cookies,
            "autoClean": self.store.settings.auto_clean,
            "autoCleanDays": self.store.settings.auto_clean_days,
            "qrUrl": self.qr.as_ref().map(|q| q.0.clone()).unwrap_or_default(),
            "qrSvg": self.qr.as_ref().map(|q| q.1.clone()).unwrap_or_default(),
            "closeWarn": self.store.settings.close_warn,
            "workspaces": spaces,
            "workspaceActive": self.store.workspace_active.unwrap_or(0),
            "sets": sets,
            "bkBookmarks": self.store.settings.backup_bookmarks,
            "bkSettings": self.store.settings.backup_settings,
            "bkHistory": self.store.settings.backup_history,
            "bkSession": self.store.settings.backup_session,
            "bkSpaces": self.store.settings.backup_spaces,
            "bkSets": self.store.settings.backup_sets,
            "bkReplace": self.store.settings.backup_replace,
            "accent": self.store.settings.accent,
            "accent2": self.store.settings.accent2,
            "tabShape": self.store.settings.tab_shape,
            "uiRadius": self.store.settings.ui_radius,
            "uiFont": self.store.settings.ui_font,
            "uiStyle": self.store.settings.ui_style,
            "uiOpacity": self.store.settings.ui_opacity,
            "uiBlur": self.store.settings.ui_blur,
            "wallpaper": self.store.settings.wallpaper,
            "toolbarHidden": self.store.settings.toolbar_hidden,
            "panelLeft": self.store.settings.panel_left,
            "addressRow": self.store.settings.address_row,
            "warpAvailable": warp_cli_path().is_some(),
            "warpStatus": self.warp_status,
            "warpSince": self.warp_since.map(|t| t.elapsed().as_secs()).unwrap_or(0),
            "warpBusy": self.warp_busy,
            "warpError": self.warp_error,
            "warpAuto": self.store.settings.warp_auto,
            "translateLang": self.store.settings.translate_lang,
            "vaultExists": self.vault_exists,
            "recovery": self.recovery,
            "vaultError": self.vault_error,
            "vaultOrigin": self
                .current_tab()
                .and_then(|t| security::origin_of(&t.url))
                .unwrap_or_default(),
            "vault": self.vault.as_ref().map(|v| json!({
                "count": v.entries.len(),
                "entries": v.entries.clone(),
            })),
            "importBrowsers": self.import_targets.iter().map(|b| json!({
                "id": b.id,
                "name": b.name,
                "profile": b.profile.display().to_string(),
                "bookmarks": b.has_bookmarks,
                "history": b.has_history,
            })).collect::<Vec<_>>(),
            "aiOn": self.store.settings.ai_on,
            "aiSave": self.store.settings.ai_save,
            "aiBusy": self.ai_busy,
            "aiError": self.ai_error,
            "aiModel": self.ai_model,
            "aiMessages": self.ai_history.clone(),
            "aiContextChars": self.ai_context.as_ref().map(|s| s.chars().count()).unwrap_or(0),
            "startupMs": self.startup_ms,
            "themes": self.store.themes,
            "settingsSections": storage::SETTINGS_SECTIONS
                .iter()
                .map(|(id, name)| json!({ "id": id, "name": name }))
                .collect::<Vec<_>>(),
            "themePresets": storage::THEME_PRESETS
                .iter()
                .map(|(id, name)| json!({ "id": id, "name": name }))
                .collect::<Vec<_>>(),
            "themeJson": self.appearance_json(),
            "bookmarks": self.store.bookmarks,
            "history": history,
            "panel": self.panel,
            "maximized": self.maximized,
            "zoom": cur.map(|t| t.zoom).unwrap_or(1.0),
            "theme": self.store.settings.theme,
            "snow": self.store.settings.snow,
            "glow": self.store.settings.glow,
            "anim": self.store.settings.anim,
            "proxyOn": self.store.settings.proxy_on,
            "proxyReady": self.store.settings.proxy_ready(),
            "proxyKind": self.store.settings.proxy_kind,
            "proxyHost": self.store.settings.proxy_host,
            "proxyPort": self.store.settings.proxy_port,
            "proxyStatus": self.proxy_status,
            "searchEngine": self.store.settings.engine(),
            "homepage": self.store.settings.homepage,
            "restoreSession": self.store.settings.restore_session,
            "memorySaver": self.store.settings.memory_saver,
            "idleMinutes": self.store.settings.idle_minutes,
            "ramLimit": self.store.settings.ram_limit_mb,
            "hwAccel": self.store.settings.hw_accel,
            "bgLimit": self.store.settings.bg_limit,
            "permNotifications": self.store.settings.perm_notifications,
            "permGeolocation": self.store.settings.perm_geolocation,
            "permCamera": self.store.settings.perm_camera,
            "permMicrophone": self.store.settings.perm_microphone,
            "status": self.status,
            "downloads": dl,
            "dlActive": dl_active,
            "closedCount": self.closed.len(),
            "sleeping": sleeping,
            "ram": (self.ram_mb * 10.0).round() / 10.0,
            "cpu": (self.cpu_pct * 10.0).round() / 10.0,
            "procs": self.procs,
        })
    }

    pub(super) fn refresh(&self) {
        let js = format!("window.render && window.render({});", self.state());
        let _ = self.chrome.evaluate_script(&js);
    }

    pub(super) fn reload_newtab(&mut self) {
        let html = self.newtab_html();
        if let Some(t) = self.tabs.get_mut(self.active) {
            if t.url == "orvyn://newtab" || t.url.is_empty() {
                if let Some(wv) = t.webview.as_ref() {
                    let _ = wv.load_html(&html);
                }
            }
        }
    }
}
