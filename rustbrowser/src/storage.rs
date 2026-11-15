use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Версия схемы `store.json`. Старые файлы мигрируют при загрузке.
pub const STORE_VERSION: u32 = 3;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub ts: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct AiMessage {
    pub role: String,
    pub content: String,
}

/// Вкладка, которую надо вернуть при следующем запуске.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct SessionTab {
    pub url: String,
    pub title: String,
    pub pinned: bool,
    /// Название группы вкладок (пусто — без группы).
    pub group: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    /// "ddg" — DuckDuckGo (по умолчанию), "google" — Google, "own" — своя выдача ORVYN.
    pub search_engine: String,
    /// Стартовая страница (Alt+Home).
    pub homepage: String,
    /// "dark" или "light".
    pub theme: String,
    /// Снежинки на новой вкладке.
    pub snow: bool,
    /// Мягкое свечение.
    pub glow: bool,
    /// Плавные анимации интерфейса.
    pub anim: bool,
    /// Восстанавливать вкладки при запуске.
    pub restore_session: bool,
    /// Memory Saver: выгружать неактивные вкладки.
    pub memory_saver: bool,
    /// Через сколько минут бездействия выгружать вкладку (0 — только по лимиту RAM).
    pub idle_minutes: u32,
    /// Лимит памяти браузера в МБ (0 — не ограничивать).
    pub ram_limit_mb: u32,
    /// Аппаратное ускорение (применяется после перезапуска).
    pub hw_accel: bool,
    /// Ограничивать фоновые процессы движка (после перезапуска).
    pub bg_limit: bool,
    /// Разрешения сайтов. По умолчанию всё запрещено.
    pub perm_notifications: bool,
    pub perm_geolocation: bool,
    pub perm_camera: bool,
    pub perm_microphone: bool,
    /// Музыка: открывать музыкальные сайты через прокси.
    pub proxy_on: bool,
    /// "socks5" или "http".
    pub proxy_kind: String,
    pub proxy_host: String,
    pub proxy_port: String,
    /// ORVYN SECURITY: главный выключатель защиты от рекламы и трекеров.
    pub protection_on: bool,
    /// Предупреждать о подозрительных переходах (punycode, IP вместо домена).
    pub redirect_guard: bool,
    /// Придерживать опасные загрузки до подтверждения.
    pub download_guard: bool,
    /// Показывать оценку JS-памяти по вкладкам в мониторе.
    pub tab_metrics: bool,
    /// Не создавать webview для фоновых вкладок, пока их не откроют.
    pub lazy_tabs: bool,
    /// ORVYN CLEAN: что чистить по кнопке и по расписанию.
    pub clean_history: bool,
    pub clean_cache: bool,
    pub clean_cookies: bool,
    /// Автоматическая очистка при запуске.
    pub auto_clean: bool,
    /// Интервал автоочистки в днях.
    pub auto_clean_days: u32,
    /// Предупреждать, если закрываешь окно с несколькими вкладками.
    pub close_warn: bool,
    /// ORVYN BACKUP: что включать в копию и как восстанавливать.
    pub backup_bookmarks: bool,
    pub backup_settings: bool,
    pub backup_history: bool,
    pub backup_session: bool,
    pub backup_spaces: bool,
    /// Включать в копию сохранённые наборы вкладок.
    pub backup_sets: bool,
    pub backup_replace: bool,
    /// ORVYN CUSTOMIZATION: внешний вид.
    pub accent: String,
    pub accent2: String,
    /// "rounded", "pill", "square".
    pub tab_shape: String,
    /// Радиус кнопок и карточек, 0–18.
    pub ui_radius: u32,
    /// "segoe", "system", "arial", "georgia", "consolas".
    pub ui_font: String,
    /// Прозрачность панелей и полос, 60–100.
    pub ui_opacity: u32,
    /// Размытие фона, 0–24.
    pub ui_blur: u32,
    /// "none", "aurora", "grid", "dots", "paper".
    pub wallpaper: String,
    /// Прятать панель инструментов (Ctrl+Shift+B).
    pub toolbar_hidden: bool,
    /// Боковая панель слева, а не справа.
    pub panel_left: bool,
    /// Отдельная строка адресной строки под кнопками.
    pub address_row: bool,
    /// ORVYN WARP: подключать WARP при запуске ORVYN (только с согласия).
    pub warp_auto: bool,
    /// Язык, на который переводить страницы (код Google Translate).
    pub translate_lang: String,
    /// ORVYN AI включён.
    pub ai_on: bool,
    /// Сохранять историю диалога в store.json.
    pub ai_save: bool,
    /// Жесты правой кнопкой мыши: назад/вперёд и прокрутка.
    pub mouse_gestures: bool,
    /// Стиль интерфейса: "chrome" (по умолчанию) или "orvyn".
    pub ui_style: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            search_engine: "ddg".to_string(),
            homepage: "orvyn://newtab".to_string(),
            theme: "dark".to_string(),
            snow: true,
            glow: true,
            anim: true,
            restore_session: true,
            memory_saver: true,
            idle_minutes: 30,
            ram_limit_mb: 2048,
            hw_accel: true,
            bg_limit: true,
            perm_notifications: false,
            perm_geolocation: false,
            perm_camera: false,
            perm_microphone: false,
            proxy_on: false,
            proxy_kind: "socks5".to_string(),
            proxy_host: String::new(),
            proxy_port: String::new(),
            protection_on: true,
            redirect_guard: true,
            download_guard: true,
            tab_metrics: true,
            lazy_tabs: true,
            clean_history: false,
            clean_cache: true,
            clean_cookies: false,
            auto_clean: false,
            auto_clean_days: 7,
            close_warn: true,
            backup_bookmarks: true,
            backup_settings: true,
            backup_history: true,
            backup_session: true,
            backup_spaces: true,
            backup_sets: true,
            backup_replace: false,
            accent: "#4fd1ff".to_string(),
            accent2: "#8b7cff".to_string(),
            tab_shape: "rounded".to_string(),
            ui_radius: 10,
            ui_font: "segoe".to_string(),
            ui_opacity: 100,
            ui_blur: 0,
            wallpaper: "none".to_string(),
            toolbar_hidden: false,
            panel_left: false,
            address_row: false,
            warp_auto: false,
            translate_lang: "ru".to_string(),
            ai_on: true,
            ai_save: true,
            mouse_gestures: true,
            ui_style: "chrome".to_string(),
        }
    }
}

impl Settings {
    /// Приводит оформление к безопасным значениям после импорта/старой копии.
    pub fn sanitize_appearance(&mut self) {
        fn hex(s: &str, fallback: &str) -> String {
            let t = s.trim();
            if t.len() == 7 && t.starts_with('#') && t[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                t.to_ascii_lowercase()
            } else {
                fallback.to_string()
            }
        }
        self.theme = match self.theme.as_str() {
            "light" => "light".into(),
            "amoled" => "amoled".into(),
            _ => "dark".into(),
        };
        self.accent = hex(&self.accent, "#4fd1ff");
        self.accent2 = hex(&self.accent2, "#8b7cff");
        if !matches!(self.tab_shape.as_str(), "rounded" | "pill" | "square") {
            self.tab_shape = "rounded".to_string();
        }
        self.ui_radius = self.ui_radius.min(18);
        if !matches!(
            self.ui_font.as_str(),
            "segoe" | "system" | "arial" | "georgia" | "consolas"
        ) {
            self.ui_font = "segoe".to_string();
        }
        if self.ui_style != "orvyn" {
            self.ui_style = "chrome".to_string();
        }
        self.ui_opacity = self.ui_opacity.clamp(60, 100);
        self.ui_blur = self.ui_blur.min(24);
        if !matches!(
            self.wallpaper.as_str(),
            "none" | "aurora" | "grid" | "dots" | "paper"
        ) {
            self.wallpaper = "none".to_string();
        }
    }

    /// Разбирает настройки прокси в (`socks5`|`http`, хост, порт).
    pub fn proxy_parts(&self) -> Option<(String, String, String)> {
        let host = self.proxy_host.trim();
        let port: u16 = self.proxy_port.trim().parse().ok()?;
        if host.is_empty()
            || port == 0
            || host.len() > 255
            || host.chars().any(char::is_whitespace)
            || host.contains('/')
        {
            return None;
        }
        let kind = if self.proxy_kind.eq_ignore_ascii_case("http") {
            "http"
        } else {
            "socks5"
        };
        Some((kind.to_string(), host.to_string(), port.to_string()))
    }

    /// Готов ли прокси к работе (включён и адрес корректный).
    pub fn proxy_ready(&self) -> bool {
        self.proxy_on && self.proxy_parts().is_some()
    }

    pub fn light(&self) -> bool {
        self.theme == "light"
    }

    /// Приводит движок поиска к одному из трёх допустимых значений.
    pub fn engine(&self) -> &str {
        match self.search_engine.as_str() {
            "google" => "google",
            "own" => "own",
            _ => "ddg",
        }
    }
}

/// Приводит введённый стартовый адрес к виду, который понимает браузер.
pub fn normalize_home(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return "orvyn://newtab".to_string();
    }
    if t.starts_with("orvyn://") || t.starts_with("http://") || t.starts_with("https://")
        || t.starts_with("file://")
    {
        return t.to_string();
    }
    if t.contains(char::is_whitespace) || !t.contains('.') {
        return "orvyn://newtab".to_string();
    }
    format!("https://{t}")
}

/// Отдельное правило защиты для сайта. Храним только исключения из общего правила.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SiteRule {
    pub host: String,
    pub protection: bool,
    /// Биты разрешений для сайта (1 уведомления, 2 гео, 4 камера, 8 микрофон).
    /// `None` — использовать общие настройки.
    pub perms: Option<u8>,
}

impl Default for SiteRule {
    fn default() -> Self {
        Self {
            host: String::new(),
            protection: true,
            perms: None,
        }
    }
}

/// Отдельное рабочее пространство: свой набор вкладок.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Workspace {
    pub id: u64,
    pub name: String,
    /// "star", "work", "study", "game", "home", "globe".
    pub icon: String,
    pub tabs: Vec<SessionTab>,
    pub active: usize,
}

/// Сохранённый набор вкладок: открывается поверх текущих, без смены пространства.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct SavedSet {
    pub name: String,
    pub tabs: Vec<SessionTab>,
    pub created: u64,
}

/// Сохранённый образец оформления.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct ThemeProfile {
    /// Версия формата профиля. 2 — с режимом AMOLED, старые файлы читаются.
    #[serde(default)]
    pub version: u32,
    pub name: String,
    pub theme: String,
    pub accent: String,
    pub accent2: String,
    pub tab_shape: String,
    pub ui_radius: u32,
    pub ui_font: String,
    pub ui_opacity: u32,
    pub ui_blur: u32,
    pub wallpaper: String,
}

impl Default for ThemeProfile {
    fn default() -> Self {
        Self {
            version: 2,
            name: String::new(),
            theme: "dark".to_string(),
            accent: "#4fd1ff".to_string(),
            accent2: "#8b7cff".to_string(),
            tab_shape: "rounded".to_string(),
            ui_radius: 10,
            ui_font: "segoe".to_string(),
            ui_opacity: 100,
            ui_blur: 0,
            wallpaper: "none".to_string(),
        }
    }
}

/// Готовые пресеты оформления: ID для интерфейса, имя для подписи.
pub const THEME_PRESETS: &[(&str, &str)] = &[
    ("dark-minimal", "Dark Minimal"),
    ("light-minimal", "Light Minimal"),
    ("cyberpunk", "Cyberpunk"),
    ("gaming", "Gaming"),
    ("glass", "Glass"),
    ("amoled", "AMOLED"),
    ("colorful", "Colorful"),
    ("maximal", "Maximal"),
    ("classic", "Classic"),
    ("custom", "Custom"),
];

/// Проверяет цвет вида `#rrggbb`, иначе возвращает запасной.
fn hex_color(s: &str, fallback: &str) -> String {
    let t = s.trim();
    if t.len() == 7 && t.starts_with('#') && t[1..].chars().all(|c| c.is_ascii_hexdigit()) {
        t.to_ascii_lowercase()
    } else {
        fallback.to_string()
    }
}

/// Приводит импортированный профиль к допустимым значениям.
///
/// Старые файлы без `version` читаются как v1 и получают актуальную версию.
pub fn normalize_theme(p: &mut ThemeProfile) {
    p.version = 2;
    p.theme = match p.theme.as_str() {
        "light" => "light".to_string(),
        "amoled" => "amoled".to_string(),
        _ => "dark".to_string(),
    };
    p.accent = hex_color(&p.accent, "#4fd1ff");
    p.accent2 = hex_color(&p.accent2, "#8b7cff");
    if !matches!(p.tab_shape.as_str(), "rounded" | "pill" | "square") {
        p.tab_shape = "rounded".to_string();
    }
    p.ui_radius = p.ui_radius.min(18);
    if !matches!(
        p.ui_font.as_str(),
        "segoe" | "system" | "arial" | "georgia" | "consolas"
    ) {
        p.ui_font = "segoe".to_string();
    }
    p.ui_opacity = p.ui_opacity.clamp(60, 100);
    p.ui_blur = p.ui_blur.min(24);
    if !matches!(
        p.wallpaper.as_str(),
        "none" | "aurora" | "grid" | "dots" | "paper"
    ) {
        p.wallpaper = "none".to_string();
    }
    p.name = p.name.trim().chars().take(40).collect();
}

/// Готовый профиль оформления. Значения проходят ту же проверку, что и импорт тем.
pub fn theme_preset(id: &str) -> Option<ThemeProfile> {
    #[allow(clippy::too_many_arguments)]
    fn make(
        name: &str,
        theme: &str,
        accent: &str,
        accent2: &str,
        shape: &str,
        radius: u32,
        font: &str,
        opacity: u32,
        blur: u32,
        wallpaper: &str,
    ) -> ThemeProfile {
        ThemeProfile {
            version: 2,
            name: name.to_string(),
            theme: theme.to_string(),
            accent: accent.to_string(),
            accent2: accent2.to_string(),
            tab_shape: shape.to_string(),
            ui_radius: radius,
            ui_font: font.to_string(),
            ui_opacity: opacity,
            ui_blur: blur,
            wallpaper: wallpaper.to_string(),
        }
    }
    Some(match id {
        "dark-minimal" => make(
            "Dark Minimal", "dark", "#8ab4f8", "#5f6368", "square", 4, "segoe", 100, 0, "none",
        ),
        "light-minimal" => make(
            "Light Minimal", "light", "#0b57d0", "#5f6368", "square", 4, "segoe", 100, 0, "none",
        ),
        "cyberpunk" => make(
            "Cyberpunk", "dark", "#00f0ff", "#ff00a8", "rounded", 8, "consolas", 100, 0, "grid",
        ),
        "gaming" => make(
            "Gaming", "dark", "#ff4655", "#7c4dff", "rounded", 10, "segoe", 100, 6, "aurora",
        ),
        "glass" => make(
            "Glass", "dark", "#9ad8ff", "#b39dff", "pill", 14, "segoe", 86, 20, "aurora",
        ),
        "amoled" => make(
            "AMOLED", "amoled", "#00e5a0", "#00b0ff", "square", 2, "segoe", 100, 0, "none",
        ),
        "colorful" => make(
            "Colorful", "light", "#ff5c8a", "#7c4dff", "pill", 16, "georgia", 100, 0, "dots",
        ),
        "maximal" => make(
            "Maximal", "dark", "#ffd166", "#ff6b81", "rounded", 18, "georgia", 95, 12, "paper",
        ),
        "classic" => make(
            "Classic", "light", "#0e8fce", "#6a5cff", "rounded", 10, "segoe", 100, 0, "none",
        ),
        "custom" => make(
            "Custom", "dark", "#4fd1ff", "#8b7cff", "rounded", 10, "segoe", 100, 0, "none",
        ),
        _ => return None,
    })
}

/// Разделы настроек для интерфейса: ID и подпись.
pub const SETTINGS_SECTIONS: &[(&str, &str)] = &[
    ("search", "Поиск и старт"),
    ("look", "Внешний вид"),
    ("memory", "Память и вкладки"),
    ("security", "Безопасность"),
    ("privacy", "Приватность"),
    ("ai", "ORVYN AI"),
    ("sites", "Сайты и система"),
    ("extra", "Прочее"),
];

/// Сбрасывает только один раздел настроек, остальные не трогает.
///
/// Нужен, когда человек поэкспериментировал с оформлением и хочет вернуть
/// только его, не теряя поиск, сейф и остальные настройки.
pub fn reset_section(s: &mut Settings, section: &str) -> bool {
    let d = Settings::default();
    match section {
        "search" => {
            s.search_engine = d.search_engine;
            s.homepage = d.homepage;
        }
        "look" => {
            s.theme = d.theme;
            s.snow = d.snow;
            s.glow = d.glow;
            s.anim = d.anim;
            s.accent = d.accent;
            s.accent2 = d.accent2;
            s.tab_shape = d.tab_shape;
            s.ui_radius = d.ui_radius;
            s.ui_font = d.ui_font;
            s.ui_opacity = d.ui_opacity;
            s.ui_blur = d.ui_blur;
            s.wallpaper = d.wallpaper;
            s.toolbar_hidden = d.toolbar_hidden;
            s.panel_left = d.panel_left;
            s.address_row = d.address_row;
            s.ui_style = d.ui_style;
        }
        "memory" => {
            s.restore_session = d.restore_session;
            s.memory_saver = d.memory_saver;
            s.idle_minutes = d.idle_minutes;
            s.ram_limit_mb = d.ram_limit_mb;
            s.hw_accel = d.hw_accel;
            s.bg_limit = d.bg_limit;
            s.close_warn = d.close_warn;
            s.mouse_gestures = d.mouse_gestures;
        }
        "security" => {
            s.protection_on = d.protection_on;
            s.redirect_guard = d.redirect_guard;
            s.download_guard = d.download_guard;
            s.tab_metrics = d.tab_metrics;
            s.lazy_tabs = d.lazy_tabs;
        }
        "privacy" => {
            s.perm_notifications = d.perm_notifications;
            s.perm_geolocation = d.perm_geolocation;
            s.perm_camera = d.perm_camera;
            s.perm_microphone = d.perm_microphone;
            s.clean_history = d.clean_history;
            s.clean_cache = d.clean_cache;
            s.clean_cookies = d.clean_cookies;
            s.auto_clean = d.auto_clean;
            s.auto_clean_days = d.auto_clean_days;
            s.backup_bookmarks = d.backup_bookmarks;
            s.backup_settings = d.backup_settings;
            s.backup_history = d.backup_history;
            s.backup_session = d.backup_session;
            s.backup_spaces = d.backup_spaces;
            s.backup_sets = d.backup_sets;
            s.backup_replace = d.backup_replace;
        }
        "ai" => {
            s.ai_on = d.ai_on;
            s.ai_save = d.ai_save;
        }
        "sites" => {
            s.proxy_on = d.proxy_on;
            s.proxy_kind = d.proxy_kind;
            s.proxy_host = d.proxy_host;
            s.proxy_port = d.proxy_port;
            s.warp_auto = d.warp_auto;
            s.translate_lang = d.translate_lang;
        }
        // В «Прочем» только карточка разработчиков и общий сброс — сбрасывать нечего.
        "extra" => {}
        _ => return false,
    }
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Store {
    /// Версия схемы. 0 — старый файл без поля.
    pub version: u32,
    pub history: Vec<HistoryEntry>,
    pub bookmarks: Vec<Bookmark>,
    pub settings: Settings,
    pub session: Vec<SessionTab>,
    pub session_active: usize,
    /// Правила защиты по сайтам (только отличия от общего поведения).
    pub site_rules: Vec<SiteRule>,
    /// Когда последний раз работала автоочистка (секунды Unix, 0 — никогда).
    pub last_clean: u64,
    /// Рабочие пространства со своими наборами вкладок.
    pub workspaces: Vec<Workspace>,
    /// Какое пространство открыто сейчас (None — обычная сессия).
    pub workspace_active: Option<u64>,
    /// Сохранённые профили оформления.
    pub themes: Vec<ThemeProfile>,
    /// Сохранённые наборы вкладок (открываются поверх текущей сессии).
    pub saved_sets: Vec<SavedSet>,
    /// История ORVYN AI (если включено сохранение).
    pub ai_history: Vec<AiMessage>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            version: STORE_VERSION,
            history: Vec::new(),
            bookmarks: Vec::new(),
            settings: Settings::default(),
            session: Vec::new(),
            session_active: 0,
            site_rules: Vec::new(),
            last_clean: 0,
            workspaces: Vec::new(),
            workspace_active: None,
            themes: Vec::new(),
            saved_sets: Vec::new(),
            ai_history: Vec::new(),
        }
    }
}

impl Store {
    /// Включена ли защита для конкретного хоста с учётом мастер-выключателя.
    pub fn protection_for(&self, host: &str) -> bool {
        if !self.settings.protection_on {
            return false;
        }
        if host.is_empty() {
            return false;
        }
        self.site_rules
            .iter()
            .find(|r| r.host.eq_ignore_ascii_case(host))
            .map(|r| r.protection)
            .unwrap_or(true)
    }

    /// Запоминает исключение для сайта. `true` убирает исключение,
    /// если у правила нет других данных (разрешений).
    pub fn set_protection(&mut self, host: &str, on: bool) {
        let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        if host.is_empty() {
            return;
        }
        if let Some(rule) = self.site_rules.iter_mut().find(|r| r.host == host) {
            rule.protection = on;
            if on && rule.perms.is_none() {
                self.site_rules.retain(|r| r.host != host);
            }
            return;
        }
        if !on {
            self.site_rules.push(SiteRule {
                host,
                protection: false,
                perms: None,
            });
        }
    }

    /// Биты разрешений сайта: своё правило или общие настройки.
    pub fn perm_bits_for(&self, host: &str, global: u8) -> u8 {
        self.site_rules
            .iter()
            .find(|r| r.host.eq_ignore_ascii_case(host))
            .and_then(|r| r.perms)
            .unwrap_or(global)
    }

    /// Включает/выключает одно разрешение для сайта. `base` — общие биты,
    /// с которых начинается своё правило.
    pub fn set_site_perm(&mut self, host: &str, bit: u8, on: bool, base: u8) {
        let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        if host.is_empty() || bit == 0 {
            return;
        }
        let idx = self.site_rules.iter().position(|r| r.host == host);
        let current = idx
            .and_then(|i| self.site_rules[i].perms)
            .unwrap_or(base);
        let next = if on { current | bit } else { current & !bit };
        if let Some(i) = idx {
            self.site_rules[i].perms = Some(next);
        } else {
            self.site_rules.push(SiteRule {
                host,
                protection: true,
                perms: Some(next),
            });
        }
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Название группы вкладок: без лишних пробелов и не длиннее 24 символов.
pub fn clean_group(name: &str) -> String {
    name.trim().chars().take(24).collect()
}

/// Проверяет код языка Google Translate, чтобы он не сломал адрес.
pub fn clean_lang(code: &str) -> String {
    let t = code.trim();
    if (2..=12).contains(&t.len())
        && t.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
    {
        t.to_string()
    } else {
        "ru".to_string()
    }
}

fn data_root() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn copy_profile(src: &PathBuf, dst: &PathBuf) {
    if !src.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(dst);
    for name in ["store.json", "vault.json", "userscripts"] {
        let from = src.join(name);
        let to = dst.join(name);
        if from.is_dir() {
            let _ = std::fs::create_dir_all(&to);
            if let Ok(rd) = std::fs::read_dir(&from) {
                for f in rd.flatten() {
                    let _ = std::fs::copy(f.path(), to.join(f.file_name()));
                }
            }
        } else if from.exists() && !to.exists() {
            let _ = std::fs::copy(&from, &to);
        }
    }
}

/// Запасной путь, если переименовать папку профиля не удалось.
fn copy_dir_all(src: &PathBuf, dst: &PathBuf) {
    let _ = std::fs::create_dir_all(dst);
    let Ok(rd) = std::fs::read_dir(src) else { return };
    for entry in rd.flatten() {
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_all(&from, &to);
        } else {
            let _ = std::fs::copy(&from, &to);
        }
    }
}

/// Переносит старые внутренние адреса ORVYN на схему ORVYN.
fn migrate_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("orbit://") {
        format!("orvyn://{rest}")
    } else if let Some(rest) = url.strip_prefix("orbit:") {
        format!("orvyn:{rest}")
    } else if let Some(rest) = url.strip_prefix("rustbrowser://") {
        format!("orvyn://{rest}")
    } else {
        url.to_string()
    }
}

fn migrate_tabs(tabs: &mut [SessionTab]) {
    for t in tabs {
        t.url = migrate_url(&t.url);
    }
}

fn base_dir() -> PathBuf {
    let root = data_root();
    let p = root.join("Orvyn");

    // Переносим весь профиль из ORVYN целиком: настройки, сейф, куки и сессии.
    if !p.exists() {
        let old = root.join("Orbit");
        if old.exists() {
            if std::fs::rename(&old, &p).is_err() {
                copy_dir_all(&old, &p);
            }
        } else {
            for legacy in ["Endervasya", "RustBrowser"] {
                let src = root.join(legacy);
                if src.exists() {
                    copy_profile(&src, &p);
                    break;
                }
            }
        }
    }

    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn data_dir() -> PathBuf {
    base_dir()
}

fn store_path() -> PathBuf {
    let mut p = base_dir();
    p.push("store.json");
    p
}

fn backup_path() -> PathBuf {
    let mut p = base_dir();
    p.push("store.backup.json");
    p
}

fn session_path() -> PathBuf {
    base_dir().join("session.json")
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
struct SessionFile {
    tabs: Vec<SessionTab>,
    active: usize,
}

/// Читает отдельный снимок сессии, если он есть.
pub fn read_session_file() -> Option<(Vec<SessionTab>, usize)> {
    let raw = std::fs::read_to_string(session_path()).ok()?;
    let mut file: SessionFile = match serde_json::from_str(&raw) {
        Ok(f) => f,
        Err(e) => {
            crate::logging::error(&format!("session.json повреждён: {e}"));
            return None;
        }
    };
    file.tabs.retain(|t| !t.url.is_empty());
    if file.tabs.is_empty() {
        return None;
    }
    let active = file.active.min(file.tabs.len().saturating_sub(1));
    Some((file.tabs, active))
}

/// Пишет снимок сессии отдельно от настроек: файл маленький и не тянет за собой историю.
pub fn write_session_file(tabs: &[SessionTab], active: usize) {
    let file = SessionFile {
        tabs: tabs.to_vec(),
        active,
    };
    let Ok(json) = serde_json::to_string(&file) else {
        return;
    };
    if let Err(e) = write_atomic(&session_path(), json.as_bytes()) {
        crate::logging::error(&format!("не удалось сохранить сессию: {e}"));
    }
}

fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("tmp");
    let result = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Путь к маркеру незавершённого запуска.
pub fn marker_path() -> PathBuf {
    base_dir().join("run.marker")
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct RunMarker {
    pub pid: u32,
    pub started: u64,
    pub version: String,
}

/// Ставит маркер запуска и возвращает маркер предыдущего запуска, если он остался.
/// Оставшийся маркер значит, что прошлый процесс не завершился штатно.
pub fn begin_run() -> Option<RunMarker> {
    let p = marker_path();
    let previous = std::fs::read_to_string(&p)
        .ok()
        .and_then(|raw| serde_json::from_str::<RunMarker>(&raw).ok());
    let marker = RunMarker {
        pid: std::process::id(),
        started: now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    if let Ok(json) = serde_json::to_string(&marker) {
        if let Err(e) = std::fs::write(&p, json) {
            crate::logging::warn(&format!("не удалось записать маркер запуска: {e}"));
        }
    }
    previous
}

/// Убирает маркер при штатном выходе.
pub fn end_run() {
    if let Err(e) = std::fs::remove_file(marker_path()) {
        if e.kind() != std::io::ErrorKind::NotFound {
            crate::logging::warn(&format!("не удалось удалить маркер запуска: {e}"));
        }
    }
}

fn read_store(path: &std::path::Path) -> Option<Store> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                crate::logging::error(&format!("не прочитан {}: {e}", path.display()));
            }
            return None;
        }
    };
    match serde_json::from_str::<Store>(&raw) {
        Ok(s) => Some(s),
        Err(e) => {
            crate::logging::error(&format!("повреждён {}: {e}", path.display()));
            None
        }
    }
}

pub fn load() -> Store {
    let mut store = load_from(&store_path(), &backup_path());
    normalize(&mut store);
    if let Err(e) = crate::db::init() {
        crate::logging::error(&format!("SQLite недоступна, работаю на JSON: {e}"));
    } else {
        match crate::db::import_from(&store) {
            Ok((h, b)) if h + b > 0 => crate::logging::info(&format!(
                "база создана: импортировано истории {h}, закладок {b}"
            )),
            Ok(_) => {}
            Err(e) => crate::logging::error(&format!("импорт в базу: {e}")),
        }
        match crate::db::load_history(2000) {
            Ok(v) => store.history = v,
            Err(e) => crate::logging::error(&format!("история не прочитана из базы: {e}")),
        }
        match crate::db::load_bookmarks() {
            Ok(v) => store.bookmarks = v,
            Err(e) => crate::logging::error(&format!("закладки не прочитаны из базы: {e}")),
        }
        if crate::db::ready() {
            if let Ok((h, b)) = crate::db::counts() {
                crate::logging::info(&format!("база: истории {h}, закладок {b}"));
            }
        }
    }
    match read_session_file() {
        Some((tabs, active)) => {
            store.session = tabs;
            store.session_active = active;
        }
        None => {
            if !store.session.is_empty() {
                write_session_file(&store.session, store.session_active);
            }
        }
    }
    save(&store);
    store
}

fn load_from(p: &std::path::Path, b: &std::path::Path) -> Store {
    let store = match read_store(p) {
        Some(s) => {
            // Свежая копия на случай повреждения основного файла.
            if let Err(e) = std::fs::copy(p, b) {
                crate::logging::warn(&format!("не удалось обновить резервную копию: {e}"));
            }
            s
        }
        None => match read_store(b) {
            Some(s) => {
                crate::logging::warn("store.json не прочитан, восстановлен из store.backup.json");
                s
            }
            None => {
                if p.exists() || b.exists() {
                    crate::logging::error("хранилище не восстановлено, создан пустой профиль");
                }
                Store::default()
            }
        },
    };
    store
}

fn normalize(store: &mut Store) {
    if store.version < STORE_VERSION {
        // v1 → v2: поле версии и общие миграции старых данных.
        // v2 → v3: правила защиты по сайтам.
        store.version = STORE_VERSION;
    }
    // Переименование ORVYN → ORVYN: старые внутренние адреса остаются рабочими.
    store.settings.homepage = migrate_url(&store.settings.homepage);
    for h in &mut store.history {
        h.url = migrate_url(&h.url);
    }
    for b in &mut store.bookmarks {
        b.url = migrate_url(&b.url);
    }
    migrate_tabs(&mut store.session);
    for w in &mut store.workspaces {
        migrate_tabs(&mut w.tabs);
    }
    for s in &mut store.saved_sets {
        migrate_tabs(&mut s.tabs);
    }
    store.site_rules.retain(|r| {
        !r.host.is_empty()
            && r.host.len() <= 255
            && !r.host.contains(char::is_whitespace)
            && !r.host.contains('/')
    });
    for r in &mut store.site_rules {
        r.host = r.host.trim_end_matches('.').to_ascii_lowercase();
        if let Some(b) = r.perms {
            r.perms = Some(b & 0x0f);
        }
    }
    store.site_rules.sort_by(|a, b| a.host.cmp(&b.host));
    store.site_rules.dedup_by(|a, b| a.host == b.host);
    store.workspaces.retain(|w| w.id > 0 && !w.name.trim().is_empty());
    for w in &mut store.workspaces {
        w.name = w.name.trim().chars().take(40).collect();
        if !matches!(w.icon.as_str(), "work" | "study" | "game" | "home" | "globe" | "star") {
            w.icon = "star".to_string();
        }
        w.tabs.retain(|t| !t.url.is_empty());
        for t in &mut w.tabs {
            t.group = clean_group(&t.group);
        }
    }
    if let Some(id) = store.workspace_active {
        if !store.workspaces.iter().any(|w| w.id == id) {
            store.workspace_active = None;
        }
    }
    store.settings.homepage = normalize_home(&store.settings.homepage);
    if !matches!(store.settings.theme.as_str(), "light" | "dark" | "amoled") {
        store.settings.theme = "dark".to_string();
    }
    store.settings.idle_minutes = store.settings.idle_minutes.min(240);
    store.settings.ram_limit_mb = match store.settings.ram_limit_mb {
        0 => 0,
        n => n.clamp(256, 16384),
    };
    store.settings.search_engine = store.settings.engine().to_string();
    store.settings.translate_lang = clean_lang(&store.settings.translate_lang);
    store.settings.sanitize_appearance();
    store.themes.retain(|t| !t.name.trim().is_empty());
    for t in &mut store.themes {
        t.name = t.name.trim().chars().take(40).collect();
        let mut s = store.settings.clone();
        s.theme = t.theme.clone();
        s.accent = t.accent.clone();
        s.accent2 = t.accent2.clone();
        s.tab_shape = t.tab_shape.clone();
        s.ui_radius = t.ui_radius;
        s.ui_font = t.ui_font.clone();
        s.ui_opacity = t.ui_opacity;
        s.ui_blur = t.ui_blur;
        s.wallpaper = t.wallpaper.clone();
        s.sanitize_appearance();
        t.theme = s.theme;
        t.accent = s.accent;
        t.accent2 = s.accent2;
        t.tab_shape = s.tab_shape;
        t.ui_radius = s.ui_radius;
        t.ui_font = s.ui_font;
        t.ui_opacity = s.ui_opacity;
        t.ui_blur = s.ui_blur;
        t.wallpaper = s.wallpaper;
    }
    store.themes.sort_by(|a, b| a.name.cmp(&b.name));
    store.themes.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    store.saved_sets.retain(|s| !s.name.trim().is_empty());
    for s in &mut store.saved_sets {
        s.name = s.name.trim().chars().take(40).collect();
        s.tabs.retain(|t| !t.url.is_empty());
        for t in &mut s.tabs {
            t.group = clean_group(&t.group);
        }
    }
    store
        .saved_sets
        .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    store
        .saved_sets
        .dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    if store.saved_sets.len() > 100 {
        store.saved_sets.truncate(100);
    }
    store.session.retain(|t| !t.url.is_empty());
    for t in &mut store.session {
        t.group = clean_group(&t.group);
    }
    store.ai_history.retain(|m| {
        matches!(m.role.as_str(), "user" | "assistant") && !m.content.trim().is_empty()
    });
    for m in &mut store.ai_history {
        if m.content.chars().count() > 24_000 {
            m.content = m.content.chars().take(24_000).collect();
        }
    }
    if store.ai_history.len() > 40 {
        let cut = store.ai_history.len() - 40;
        store.ai_history.drain(0..cut);
    }
}

pub fn save(store: &Store) {
    let p = store_path();
    // История и закладки живут в SQLite, сессия — в session.json.
    // В store.json остаются только настройки и небольшие списки.
    let mut out = store.clone();
    if crate::db::ready() {
        out.history.clear();
        out.bookmarks.clear();
    }
    out.session.clear();
    out.session_active = 0;
    let json = match serde_json::to_string_pretty(&out) {
        Ok(j) => j,
        Err(e) => {
            crate::logging::error(&format!("настройки не сериализованы: {e}"));
            return;
        }
    };
    if let Err(e) = write_atomic(&p, json.as_bytes()) {
        crate::logging::error(&format!("не удалось сохранить store.json: {e}"));
        // Последняя попытка без атомарной замены, чтобы не потерять изменения.
        if let Err(e2) = std::fs::write(&p, &json) {
            crate::logging::error(&format!("прямая запись store.json тоже не удалась: {e2}"));
        }
    }
}

pub fn add_history(store: &mut Store, url: &str, title: &str) {
    if url.is_empty()
        || url.starts_with("about:")
        || url.starts_with("data:")
        || url.starts_with("blob:")
        || url.starts_with("orvyn://")
        || url.starts_with("orbit://")
        || url.starts_with("rustbrowser://")
        || url.starts_with("chrome-error:")
    {
        return;
    }
    if let Some(last) = store.history.last_mut() {
        if last.url == url {
            if !title.is_empty() && last.title != title {
                last.title = title.to_string();
                crate::db::add_history(url, title, last.ts);
                save(store);
            }
            return;
        }
    }
    let clean_title = if title.is_empty() || title == "Новая вкладка" {
        url.to_string()
    } else {
        title.to_string()
    };
    let ts = now();
    crate::db::add_history(url, &clean_title, ts);
    store.history.push(HistoryEntry {
        url: url.to_string(),
        title: clean_title,
        ts,
    });
    if store.history.len() > 2000 {
        let cut = store.history.len() - 2000;
        store.history.drain(0..cut);
    }
    save(store);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_v1() {
        let s = Settings::default();
        assert_eq!(s.engine(), "ddg");
        assert_eq!(s.theme, "dark");
        assert!(s.restore_session);
        assert!(s.memory_saver);
        assert!(!s.perm_camera);
        assert!(s.protection_on);
        assert!(s.redirect_guard);
        assert!(s.download_guard);
    }

    #[test]
    fn site_rules_default_on_and_exceptions_work() {
        let mut st = Store::default();
        assert!(st.protection_for("example.com"));
        st.set_protection("Example.COM", false);
        assert!(!st.protection_for("example.com"));
        assert_eq!(st.site_rules.len(), 1);
        st.set_protection("example.com", true);
        assert!(st.protection_for("example.com"));
        assert!(st.site_rules.is_empty());
        st.settings.protection_on = false;
        assert!(!st.protection_for("example.com"));
    }

    #[test]
    fn site_permissions_override_global() {
        let mut st = Store::default();
        assert_eq!(st.perm_bits_for("example.com", 1), 1);
        st.set_site_perm("Example.com", 4, true, 1);
        assert_eq!(st.perm_bits_for("example.com", 1), 5);
        st.set_site_perm("example.com", 1, false, 1);
        assert_eq!(st.perm_bits_for("example.com", 1), 4);
        // Выключение защиты не должно стирать разрешения сайта.
        st.set_protection("example.com", false);
        st.set_protection("example.com", true);
        assert_eq!(st.perm_bits_for("example.com", 0), 4);
        assert_eq!(st.site_rules.len(), 1);
    }

    #[test]
    fn homepage_is_normalized() {
        assert_eq!(normalize_home(""), "orvyn://newtab");
        assert_eq!(normalize_home("example.com"), "https://example.com");
        assert_eq!(normalize_home("orvyn://newtab"), "orvyn://newtab");
        assert_eq!(normalize_home("какой-то текст"), "orvyn://newtab");
    }

    #[test]
    fn old_orbit_urls_are_migrated() {
        assert_eq!(migrate_url("orbit://newtab"), "orvyn://newtab");
        assert_eq!(migrate_url("orbit://search?q=test"), "orvyn://search?q=test");
        assert_eq!(migrate_url("rustbrowser://newtab"), "orvyn://newtab");
        assert_eq!(migrate_url("https://example.com"), "https://example.com");
    }

    #[test]
    fn store_version_is_current() {
        let st = Store::default();
        assert_eq!(st.version, STORE_VERSION);
        let json = serde_json::to_string(&st).unwrap();
        let back: Store = serde_json::from_str(&json).unwrap();
        assert_eq!(back.version, STORE_VERSION);
    }

    #[test]
    fn section_reset_touches_only_that_section() {
        let mut s = Settings::default();
        s.theme = "light".to_string();
        s.accent = "#000000".to_string();
        s.search_engine = "google".to_string();
        s.ai_on = !Settings::default().ai_on;
        s.proxy_host = "1.2.3.4".to_string();

        assert!(reset_section(&mut s, "look"));
        assert_eq!(s.theme, "dark");
        assert_eq!(s.accent, "#4fd1ff");
        assert_eq!(s.search_engine, "google");
        assert_eq!(s.proxy_host, "1.2.3.4");
        assert_eq!(s.ai_on, !Settings::default().ai_on);

        assert!(reset_section(&mut s, "search"));
        assert_eq!(s.search_engine, Settings::default().search_engine);

        assert!(!reset_section(&mut s, "unknown"));
    }

    #[test]
    fn theme_v2_accepts_amoled_and_upgrades_old_files() {
        let mut p: ThemeProfile =
            serde_json::from_str(r##"{"name":"Old","theme":"light","accent":"#FFFFFF"}"##).unwrap();
        assert_eq!(p.version, 0);
        normalize_theme(&mut p);
        assert_eq!(p.version, 2);
        assert_eq!(p.theme, "light");
        assert_eq!(p.accent, "#ffffff");

        p.theme = "amoled".to_string();
        normalize_theme(&mut p);
        assert_eq!(p.theme, "amoled");

        p.theme = "neon".to_string();
        normalize_theme(&mut p);
        assert_eq!(p.theme, "dark");
    }

    #[test]
    fn theme_presets_are_valid() {
        assert_eq!(THEME_PRESETS.len(), 10);
        for (id, name) in THEME_PRESETS {
            let p = theme_preset(id).unwrap_or_else(|| panic!("нет пресета {id}"));
            assert_eq!(p.name.as_str(), *name);
            assert!(matches!(p.theme.as_str(), "dark" | "light" | "amoled"));
            for c in [&p.accent, &p.accent2] {
                assert!(c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit()));
            }
            assert!(matches!(p.tab_shape.as_str(), "rounded" | "pill" | "square"));
            assert!(p.ui_radius <= 18);
            assert!(matches!(p.ui_font.as_str(), "segoe" | "system" | "arial" | "georgia" | "consolas"));
            assert!((60..=100).contains(&p.ui_opacity));
            assert!(p.ui_blur <= 24);
            assert!(matches!(
                p.wallpaper.as_str(),
                "none" | "aurora" | "grid" | "dots" | "paper"
            ));
        }
        assert!(theme_preset("nope").is_none());
    }

    #[test]
    fn session_roundtrip() {
        let mut st = Store::default();
        st.session.push(SessionTab {
            url: "https://example.com".into(),
            title: "E".into(),
            pinned: true,
            group: "Учёба".into(),
        });
        st.session_active = 0;
        let json = serde_json::to_string(&st).unwrap();
        let back: Store = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session.len(), 1);
        assert!(back.session[0].pinned);
        assert_eq!(back.session[0].group, "Учёба");
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let unique = format!(
            "orvyn-test-{tag}-{}-{}",
            std::process::id(),
            now()
        );
        let p = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn corrupt_primary_recovers_from_backup() {
        let dir = temp_dir("recover");
        let primary = dir.join("store.json");
        let backup = dir.join("store.backup.json");
        let mut st = Store::default();
        st.bookmarks.push(Bookmark {
            url: "https://example.com".into(),
            title: "Example".into(),
        });
        std::fs::write(&backup, serde_json::to_string(&st).unwrap()).unwrap();
        std::fs::write(&primary, "{ broken json").unwrap();
        let loaded = load_from(&primary, &backup);
        assert_eq!(loaded.bookmarks.len(), 1);
        assert_eq!(loaded.bookmarks[0].url, "https://example.com");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_primary_uses_backup() {
        let dir = temp_dir("missing");
        let primary = dir.join("store.json");
        let backup = dir.join("store.backup.json");
        let mut st = Store::default();
        st.history.push(HistoryEntry {
            url: "https://example.org".into(),
            title: "Org".into(),
            ts: 1,
        });
        std::fs::write(&backup, serde_json::to_string(&st).unwrap()).unwrap();
        let loaded = load_from(&primary, &backup);
        assert_eq!(loaded.history.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_profile_when_nothing_readable() {
        let dir = temp_dir("empty");
        let primary = dir.join("store.json");
        let backup = dir.join("store.backup.json");
        std::fs::write(&primary, "not json").unwrap();
        std::fs::write(&backup, "also not json").unwrap();
        let loaded = load_from(&primary, &backup);
        assert!(loaded.history.is_empty());
        assert!(loaded.bookmarks.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
