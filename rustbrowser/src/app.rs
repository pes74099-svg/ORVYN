use crate::{
    adblock, ai, backup, capture, cleaner, downloads, import, monitor, music, search, security,
    storage, update, vault,
};
use zeroize::Zeroize;

mod chrome;
mod download_ui;
mod events;
mod importer;
mod pages;
mod passwords;
mod state;

use std::collections::{HashMap, HashSet};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::{Icon, Window, WindowBuilder};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2Environment, ICoreWebView2Environment8, ICoreWebView2Profile2,
    ICoreWebView2Profile4, ICoreWebView2_13, ICoreWebView2_2, ICoreWebView2_8,
    COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE, COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE,
    COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_CAMERA,
    COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION, COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
    COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS, COREWEBVIEW2_PERMISSION_STATE_ALLOW,
    COREWEBVIEW2_PERMISSION_STATE_DENY,
};
use webview2_com::{
    CallDevToolsProtocolMethodCompletedHandler, ClearBrowsingDataCompletedHandler,
    PermissionRequestedEventHandler, SetPermissionStateCompletedHandler,
};
use windows::core::{Interface, PCWSTR, PWSTR};
use wry::{
    MemoryUsageLevel, NewWindowResponse, PageLoadEvent, Rect, WebContext, WebView, WebViewBuilder,
    WebViewBuilderExtWindows, WebViewExtWindows,
};

const CHROME_H: f64 = 92.0;
const TABBAR_H: f64 = 38.0;
const TOOLBAR_H: f64 = 54.0;
const TOOLBAR_ROW_H: f64 = 88.0;
const BOOKMARKS_H: f64 = 32.0;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Процентное кодирование для адреса страницы в Google Translate.
fn google_translate_url(url: &str, lang: &str) -> String {
    let lang = storage::clean_lang(lang);
    if let Some((_, rest)) = url.split_once("://") {
        let (authority, tail) = match rest.find(|c| c == '/' || c == '?' || c == '#') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        };
        if !authority.is_empty() && !authority.contains(':') && !authority.contains('@') {
            let host = authority.to_ascii_lowercase().replace('.', "-");
            let path = if tail.starts_with('/') {
                tail.to_string()
            } else if tail.is_empty() {
                "/".to_string()
            } else {
                format!("/{tail}")
            };
            let sep = if path.contains('?') { '&' } else { '?' };
            return format!(
                "https://{host}.translate.goog{path}{sep}_x_tr_sl=auto&_x_tr_tl={lang}&_x_tr_hl=ru"
            );
        }
    }
    format!(
        "https://translate.google.com/translate?hl=ru&sl=auto&tl={lang}&u={}",
        url_encode(url)
    )
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
const NEWTAB_HTML: &str = include_str!("../assets/newtab.html");
const UI_HTML: &str = include_str!("../assets/ui.html");
const FIND_JS: &str = include_str!("../assets/findbar.js");
const READER_JS: &str = include_str!("../assets/reader.js");

/// Пишет в stderr, не падая, если консоли нет (GUI-приложение).
fn log_err(msg: &str) {
    crate::logging::error(msg);
}

fn browser_args(s: &storage::Settings) -> String {
    let mut a = String::from(
        "--disable-features=msEdgeSidebarV2,msEdgeShoppingAssistant,msEdgeSearchBar,msEdgeVisualSearch,msEdgeCollections,msEdgeWebCapture,msEdgeTranslate,msEdgeAutofill,CalculatedNativeWinOcclusion,msWebView2BrowserHitTransparent,Translate,MediaRouter,OptimizationHints,OptimizationGuideModelDownloading,AutofillServerCommunication,InterestFeedContentSuggestions,PaintHolding,SegmentationPlatform,NetworkTimeServiceQuerying,CertificateTransparencyComponentUpdater,HeavyAdPrivacyMitigations,DecodeTilesInBackground \
         --disable-component-update --disable-component-extensions-with-background-pages \
         --disable-default-apps --disable-client-side-phishing-detection --disable-breakpad \
         --disable-crash-reporter --disable-domain-reliability --no-first-run \
         --no-default-browser-check --noerrdialogs --no-pings --metrics-recording-only \
         --disk-cache-size=67108864 --media-cache-size=33554432 \
         --js-flags=--max-old-space-size=128 --autoplay-policy=no-user-gesture-required",
    );
    if s.bg_limit {
        a.push_str(
            " --disable-extensions --disable-sync --disable-background-networking \
             --disable-gpu-shader-disk-cache --enable-low-end-device-mode \
             --renderer-process-limit=2 --process-per-site \
             --force-gpu-mem-available-mb=64 --gpu-program-cache-size-kb=2048",
        );
    }
    if !s.hw_accel {
        a.push_str(" --disable-gpu --disable-gpu-compositing");
    }
    a
}

/// Битовая маска разрешений сайтов: общие настройки + исключения по сайтам.
#[derive(Clone)]
struct Perms {
    bits: Arc<AtomicU8>,
    site: Arc<std::sync::Mutex<HashMap<String, u8>>>,
}

impl Perms {
    fn new(s: &storage::Settings) -> Self {
        let p = Self {
            bits: Arc::new(AtomicU8::new(0)),
            site: Arc::new(std::sync::Mutex::new(HashMap::new())),
        };
        p.update(s);
        p
    }

    fn update(&self, s: &storage::Settings) {
        let mut b = 0u8;
        if s.perm_notifications {
            b |= 1;
        }
        if s.perm_geolocation {
            b |= 2;
        }
        if s.perm_camera {
            b |= 4;
        }
        if s.perm_microphone {
            b |= 8;
        }
        self.bits.store(b, Ordering::Relaxed);
    }

    fn update_sites(&self, rules: &[storage::SiteRule]) {
        let mut map = HashMap::new();
        for r in rules {
            if let Some(b) = r.perms {
                map.insert(r.host.clone(), b & 0x0f);
            }
        }
        if let Ok(mut m) = self.site.lock() {
            *m = map;
        }
    }

    fn mask(&self) -> u8 {
        self.bits.load(Ordering::Relaxed)
    }

    fn bit_com(kind: i32) -> u8 {
        if kind == COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS.0 as i32 {
            1
        } else if kind == COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION.0 as i32 {
            2
        } else if kind == COREWEBVIEW2_PERMISSION_KIND_CAMERA.0 as i32 {
            4
        } else if kind == COREWEBVIEW2_PERMISSION_KIND_MICROPHONE.0 as i32 {
            8
        } else {
            0
        }
    }

    fn allows_bits(&self, host: &str, bit: u8) -> bool {
        if bit == 0 {
            return false;
        }
        if let Ok(m) = self.site.lock() {
            if let Some(b) = m.get(&host.to_ascii_lowercase()) {
                return b & bit != 0;
            }
        }
        self.mask() & bit != 0
    }
}

/// Подписывается на запросы разрешений WebView2 и решает их по правилам сайта.
fn attach_permissions(wv: &WebView, perms: Perms) {
    let core = wv.webview();
    let handler = PermissionRequestedEventHandler::create(Box::new(move |_wv, args| {
        let Some(args) = args else { return Ok(()) };
        let mut kind =
            webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_PERMISSION_KIND(0);
        let _ = unsafe { args.PermissionKind(&mut kind) };
        let mut uri = PWSTR::null();
        let _ = unsafe { args.Uri(&mut uri) };
        let url = if uri.is_null() {
            String::new()
        } else {
            unsafe { uri.to_string().unwrap_or_default() }
        };
        let host = security::host_of(&url).unwrap_or_default();
        let allow = perms.allows_bits(&host, Perms::bit_com(kind.0));
        let state = if allow {
            COREWEBVIEW2_PERMISSION_STATE_ALLOW
        } else {
            COREWEBVIEW2_PERMISSION_STATE_DENY
        };
        let _ = unsafe { args.SetState(state) };
        Ok(())
    }));
    let mut token = 0i64;
    let _ = unsafe { core.add_PermissionRequested(&handler, &mut token) };
}

/// Включает или выключает звук вкладки на уровне WebView2.
fn set_core_muted(core: &ICoreWebView2, muted: bool) -> bool {
    let Ok(w8) = core.cast::<ICoreWebView2_8>() else { return false };
    unsafe { w8.SetIsMuted(muted) }.is_ok()
}

fn set_origin_perm(
    core: &ICoreWebView2,
    origin: &str,
    kind: COREWEBVIEW2_PERMISSION_KIND,
    allow: bool,
) {
    let Ok(w13) = core.cast::<ICoreWebView2_13>() else { return };
    let Ok(profile) = (unsafe { w13.Profile() }) else { return };
    let Ok(p4) = profile.cast::<ICoreWebView2Profile4>() else { return };
    let wide: Vec<u16> = origin.encode_utf16().chain(std::iter::once(0)).collect();
    let handler = SetPermissionStateCompletedHandler::create(Box::new(|_hr| Ok(())));
    let state = if allow {
        COREWEBVIEW2_PERMISSION_STATE_ALLOW
    } else {
        COREWEBVIEW2_PERMISSION_STATE_DENY
    };
    unsafe {
        let _ = p4.SetPermissionState(kind, PCWSTR(wide.as_ptr()), state, &handler);
    }
}

/// Записывает состояние разрешений сайта в профиль WebView2,
/// чтобы правило действовало сразу и после перезапуска.
fn apply_bits_to_core(core: &ICoreWebView2, origin: &str, bits: u8) {
    let kinds = [
        (1u8, COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS),
        (2u8, COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION),
        (4u8, COREWEBVIEW2_PERMISSION_KIND_CAMERA),
        (8u8, COREWEBVIEW2_PERMISSION_KIND_MICROPHONE),
    ];
    for (bit, kind) in kinds {
        set_origin_perm(core, origin, kind, bits & bit != 0);
    }
}

#[derive(Debug)]
enum UserEvent {
    Chrome(String),
    Page(usize, String),
    Loaded { id: usize, url: String, done: bool },
    Title { id: usize, title: String },
    NewWindow { url: String, proxied: bool, incognito: bool },
    SearchDone { id: usize, q: String, html: String },
    ProxyTest(String),
    BadSite { id: usize, url: String, reason: Option<String> },
    Download(downloads::Msg),
    Usage(String, u64),
    CleanSize(u64),
    ShotSize { id: usize, width: f64, height: f64 },
    Saved { kind: String, path: String, id: usize },
    SaveError(String),
    WarpStatus(String, String),
    WarpAction(Result<String, String>),
    AiReply(Result<ai::Reply, String>),
    AiPageText(String),
    Update(Result<update::Release, String>),
    Quit,
}

#[derive(Clone, Debug, PartialEq)]
enum TabKind {
    Web,
    Search(String),
}

enum Target {
    Url(String),
    Search(String),
}

struct Tab {
    id: usize,
    webview: Option<WebView>,
    title: String,
    url: String,
    loading: bool,
    zoom: f64,
    kind: TabKind,
    proxied: bool,
    pinned: bool,
    incognito: bool,
    muted: bool,
    last_active: Instant,
    /// Оценка JS-памяти страницы в МБ (performance.memory).
    heap_mb: f64,
    /// Название группы вкладок (пусто — без группы).
    group: String,
}

struct ClosedTab {
    url: String,
    title: String,
    kind: TabKind,
    incognito: bool,
    group: String,
}

struct DownloadItem {
    id: u64,
    url: String,
    name: String,
    path: String,
    bytes: u64,
    total: i64,
    speed: u64,
    state: String,
    /// Почему загрузка придержана (пусто — обычная загрузка).
    reason: String,
}

struct App {
    window: Window,
    maximized: bool,
    chrome: WebView,
    web_context: WebContext,
    music_context: Option<(String, WebContext)>,
    tabs: Vec<Tab>,
    active: usize,
    next_id: usize,
    store: storage::Store,
    panel: Option<String>,
    blocked: u32,
    /// Заблокировано на конкретной странице (ключ — хост страницы).
    blocked_site: HashMap<String, u32>,
    /// Какие сторонние домены чаще всего резались (для панели щита).
    blocked_domains: HashMap<String, u32>,
    last_blocked: Instant,
    inject: String,
    proxy_status: String,
    proxy: EventLoopProxy<UserEvent>,
    args: String,
    guard: security::Guard,
    perms: Perms,
    downloads: Vec<DownloadItem>,
    last_dl_ui: Instant,
    closed: Vec<ClosedTab>,
    status: String,
    /// Сколько вкладок восстановлено после аварийного завершения.
    recovery: usize,
    ram_mb: f64,
    cpu_pct: f32,
    procs: u32,
    last_sample: monitor::Sample,
    last_sample_at: Instant,
    last_saver: Instant,
    /// Время последнего снимка сессии (для восстановления после сбоя).
    last_snapshot: Instant,
    /// Отпечаток вкладок на момент снимка.
    last_session_sig: String,
    restoring: bool,
    /// Общая проверка опасных загрузок (меняется без пересоздания вкладок).
    dl_guard: Arc<AtomicBool>,
    last_heap: Instant,
    /// Замер до выгрузки: (время, сколько МБ было).
    freed_pending: Option<(Instant, f64)>,
    last_freed: f64,
    clean_size: u64,
    clean_scanning: bool,
    usage: Vec<(String, u64)>,
    usage_pending: usize,
    usage_scanning: bool,
    /// URL и готовый SVG QR-кода для панели.
    qr: Option<(String, String)>,
    warp_status: String,
    warp_error: String,
    warp_busy: bool,
    warp_since: Option<Instant>,
    warp_last_check: Instant,
    warp_auto_pending: bool,
    startup_ms: u64,
    /// Открытый сейф паролей (None — закрыт).
    vault: Option<vault::Vault>,
    vault_exists: bool,
    vault_error: String,
    vault_last_use: Instant,
    /// Найденные профили других браузеров для импорта.
    import_targets: Vec<import::Browser>,
    /// История ORVYN AI.
    ai_history: Vec<storage::AiMessage>,
    ai_context: Option<String>,
    ai_busy: bool,
    ai_error: String,
    ai_model: String,
    /// Проверка обновлений через GitHub Releases.
    update_checking: bool,
    update_result: Option<Result<update::Release, String>>,
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect {
        position: LogicalPosition::new(x, y).into(),
        size: LogicalSize::new(w.max(1.0), h.max(1.0)).into(),
    }
}

fn encode_query(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn is_local_host(s: &str) -> bool {
    let host = s.split('/').next().unwrap_or(s);
    let no_port = match host.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') && !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => h,
        _ => host,
    };
    let bare = no_port.trim_start_matches('[').trim_end_matches(']');
    bare == "localhost"
        || bare == "::1"
        || bare.parse::<std::net::Ipv4Addr>().is_ok()
        || (no_port.starts_with('[') && no_port.ends_with(']'))
}

fn warp_cli_path() -> Option<PathBuf> {
    let base = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".to_string());
    let p = PathBuf::from(base)
        .join("Cloudflare")
        .join("Cloudflare WARP")
        .join("warp-cli.exe");
    p.exists().then_some(p)
}

fn warp_run(args: &[&str]) -> Result<String, String> {
    let exe = warp_cli_path().ok_or_else(|| "Cloudflare WARP не установлен".to_string())?;
    let mut cmd = Command::new(exe);
    cmd.arg("--accept-tos").args(args);
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().map_err(|e| format!("не удалось запустить warp-cli: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let msg = if err.is_empty() {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        } else {
            err
        };
        Err(if msg.is_empty() {
            "warp-cli вернул ошибку".to_string()
        } else {
            msg
        })
    }
}

/// Проверка доступности прокси: просто TCP-коннект с таймаутом.
fn probe_proxy(host: &str, port: &str) -> String {
    use std::net::{TcpStream, ToSocketAddrs};
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let Ok(addrs) = format!("{host}:{port}").to_socket_addrs() else {
        return "Не удалось разобрать адрес прокси.".to_string();
    };
    for addr in addrs {
        if TcpStream::connect_timeout(&addr, Duration::from_secs(4)).is_ok() {
            return format!("Прокси отвечает ({addr}).");
        }
    }
    "Прокси не отвечает. Проверь адрес, порт и что сервер включён.".to_string()
}

/// Создаёт webview вкладки. Вынесено из App, чтобы не путаться с заимствованиями
/// контекстов (обычный и музыкальный прокси-профиль).
#[allow(clippy::too_many_arguments)]
fn spawn_webview(
    window: &Window,
    ctx: &mut WebContext,
    env: Option<ICoreWebView2Environment>,
    args: &str,
    incognito: bool,
    proxied: bool,
    url: Option<&str>,
    html: Option<&str>,
    bounds: Rect,
    inject: &str,
    proxy: EventLoopProxy<UserEvent>,
    guard: security::Guard,
    perms: Perms,
    dl_guard: Arc<AtomicBool>,
    id: usize,
    dark: bool,
) -> wry::Result<WebView> {
    let p1 = proxy.clone();
    let p2 = proxy.clone();
    let p3 = proxy.clone();
    let p4 = proxy.clone();
    let p5 = proxy.clone();
    let g = guard.clone();

    let mut b = WebViewBuilder::new_with_web_context(ctx);
    if let Some(e) = env {
        b = b.with_environment(e);
    } else {
        b = b.with_additional_browser_args(args.to_string());
    }
    if incognito {
        b = b.with_incognito(true);
    }
    let bg = if dark {
        (13, 15, 19, 255)
    } else {
        (244, 247, 251, 255)
    };
    let b = b
        .with_bounds(bounds)
        .with_initialization_script(inject)
        .with_devtools(true)
        .with_background_color(bg)
        .with_navigation_handler(move |u| {
            if g.blocks(&u) {
                let _ = p5.send_event(UserEvent::BadSite { id, url: u, reason: None });
                false
            } else if let Some(reason) = g.warn(&u) {
                let _ = p5.send_event(UserEvent::BadSite {
                    id,
                    url: u.clone(),
                    reason: Some(reason),
                });
                false
            } else {
                true
            }
        })
        .with_new_window_req_handler(move |u, _f| {
            let _ = p1.send_event(UserEvent::NewWindow {
                url: u,
                proxied,
                incognito,
            });
            NewWindowResponse::Deny
        })
        .with_on_page_load_handler(move |ev, u| {
            let done = matches!(ev, PageLoadEvent::Finished);
            let _ = p2.send_event(UserEvent::Loaded { id, url: u, done });
        })
        .with_document_title_changed_handler(move |t| {
            let _ = p3.send_event(UserEvent::Title { id, title: t });
        })
        .with_ipc_handler(move |req| {
            let _ = p4.send_event(UserEvent::Page(id, req.body().clone()));
        });

    let b = match url {
        Some(u) if !u.is_empty() => b.with_url(u),
        _ => b.with_html(html.unwrap_or("")),
    };
    let wv = b.build_as_child(window)?;
    attach_permissions(&wv, perms);
    let sink: downloads::Sink = Arc::new(move |msg| {
        let _ = proxy.send_event(UserEvent::Download(msg));
    });
    downloads::attach(&wv, sink, dl_guard);
    Ok(wv)
}

impl App {
    fn new(
        window: Window,
        proxy: EventLoopProxy<UserEvent>,
        args: String,
        store: storage::Store,
    ) -> wry::Result<Self> {
        adblock::ensure_userscripts();
        let sf = window.scale_factor();
        let size = window.inner_size();
        let w = size.width as f64 / sf;
        let dark = store.settings.theme != "light";

        let mut web_context = WebContext::new(Some(storage::data_dir().join("webview")));
        let p = proxy.clone();
        let chrome = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_bounds(rect(0.0, 0.0, w, CHROME_H))
            .with_html(UI_HTML)
            .with_additional_browser_args(args.clone())
            .with_devtools(true)
            .with_background_color(if dark {
                (7, 10, 16, 255)
            } else {
                (244, 247, 251, 255)
            })
            .with_ipc_handler(move |req| {
                let _ = p.send_event(UserEvent::Chrome(req.body().clone()));
            })
            .build_as_child(&window)?;

        let status = if store.settings.proxy_ready() {
            "Прокси включён для музыки.".to_string()
        } else {
            String::new()
        };
        let perms = Perms::new(&store.settings);
        perms.update_sites(&store.site_rules);
        let guard = security::Guard::default();
        guard.set_strict(store.settings.redirect_guard);
        let dl_guard = Arc::new(AtomicBool::new(store.settings.download_guard));
        let inject = adblock::inject_script(store.settings.protection_on, store.settings.mouse_gestures);
        let ai_history = store.ai_history.clone();

        // Тихая проверка обновлений при старте: сеть в отдельном потоке, UI не ждёт.
        {
            let p = proxy.clone();
            std::thread::spawn(move || {
                let _ = p.send_event(UserEvent::Update(update::check()));
            });
        }
        Ok(Self {
            window,
            maximized: false,
            chrome,
            web_context,
            music_context: None,
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            store,
            panel: None,
            blocked: 0,
            blocked_site: HashMap::new(),
            blocked_domains: HashMap::new(),
            last_blocked: Instant::now(),
            inject,
            proxy_status: status,
            proxy,
            args,
            guard,
            perms,
            downloads: Vec::new(),
            last_dl_ui: Instant::now(),
            closed: Vec::new(),
            status: String::new(),
            recovery: 0,
            ram_mb: 0.0,
            cpu_pct: 0.0,
            procs: 0,
            last_sample: monitor::Sample::default(),
            last_sample_at: Instant::now(),
            last_saver: Instant::now(),
            last_snapshot: Instant::now(),
            last_session_sig: String::new(),
            restoring: false,
            dl_guard,
            last_heap: Instant::now(),
            freed_pending: None,
            last_freed: 0.0,
            clean_size: 0,
            clean_scanning: false,
            usage: Vec::new(),
            usage_pending: 0,
            usage_scanning: false,
            qr: None,
            warp_status: "Unknown".to_string(),
            warp_error: String::new(),
            warp_busy: false,
            warp_since: None,
            warp_last_check: Instant::now() - Duration::from_secs(60),
            warp_auto_pending: false,
            startup_ms: 0,
            vault: None,
            vault_exists: vault::exists(),
            vault_error: String::new(),
            vault_last_use: Instant::now(),
            import_targets: Vec::new(),
            ai_history,
            ai_context: None,
            ai_busy: false,
            ai_error: String::new(),
            ai_model: ai::MODEL_LABEL.to_string(),
            update_checking: false,
            update_result: None,
        })
    }

    fn logical_size(&self) -> (f64, f64) {
        let sf = self.window.scale_factor();
        let s = self.window.inner_size();
        (s.width as f64 / sf, s.height as f64 / sf)
    }

    fn content_rect(&self) -> Rect {
        let (w, h) = self.logical_size();
        let top = self.chrome_height();
        rect(0.0, top, w, (h - top).max(1.0))
    }

    fn chrome_height(&self) -> f64 {
        if self.panel.is_some() {
            return self.logical_size().1;
        }
        let mut h = TABBAR_H;
        if !self.store.settings.toolbar_hidden {
            h += if self.store.settings.address_row {
                TOOLBAR_ROW_H
            } else {
                TOOLBAR_H
            };
        }
        if self.bookmarks_bar_visible() {
            h += BOOKMARKS_H;
        }
        h
    }

    /// Полоса закладок показывается только в Chrome-стиле и при наличии закладок.
    fn bookmarks_bar_visible(&self) -> bool {
        self.store.settings.ui_style != "orvyn"
            && self.store.settings.bookmarks_bar
            && !self.store.settings.toolbar_hidden
            && !self.store.bookmarks.is_empty()
    }

    fn layout(&self) {
        let (w, _h) = self.logical_size();
        let chrome_h = self.chrome_height();
        let _ = self.chrome.set_bounds(rect(0.0, 0.0, w, chrome_h));
        let cr = self.content_rect();
        for (i, t) in self.tabs.iter().enumerate() {
            let Some(wv) = t.webview.as_ref() else { continue };
            let active = i == self.active;
            let _ = wv.set_bounds(cr);
            let _ = wv.set_visible(active && self.panel.is_none());
            let _ = wv.set_memory_usage_level(if active {
                MemoryUsageLevel::Normal
            } else {
                MemoryUsageLevel::Low
            });
        }
    }

    fn newtab_html(&self) -> String {
        let s = &self.store.settings;
        let font = match s.ui_font.as_str() {
            "system" => "system-ui, \"Segoe UI\", sans-serif",
            "arial" => "Arial, sans-serif",
            "georgia" => "Georgia, serif",
            "consolas" => "Consolas, monospace",
            _ => "\"Segoe UI\", system-ui, sans-serif",
        };
        let wall = if s.wallpaper == "none" {
            String::new()
        } else {
            format!("wall-{}", s.wallpaper)
        };
        NEWTAB_HTML
            .replace("{{SNOW}}", if s.snow { "true" } else { "false" })
            .replace("{{GLOW}}", if s.glow { "true" } else { "false" })
            .replace("{{ANIM}}", if s.anim { "true" } else { "false" })
            .replace("{{THEME}}", &s.theme)
            .replace("{{ACCENT}}", &s.accent)
            .replace("{{ACCENT2}}", &s.accent2)
            .replace("{{FONT}}", font)
            .replace("{{WALL}}", &wall)
    }

    fn new_tab(&mut self, url: Option<String>, activate: bool) {
        self.new_tab_full(url, activate, false, false, false);
    }

    fn new_incognito(&mut self) {
        self.new_tab_full(None, true, false, false, true);
    }

    fn prepare_music_ctx(&mut self) -> bool {
        if !self.store.settings.proxy_ready() {
            return false;
        }
        let tag = music::proxy_tag(&self.store.settings);
        let stale = self
            .music_context
            .as_ref()
            .map(|(t, _)| t != &tag)
            .unwrap_or(true);
        if stale {
            let dir = storage::data_dir().join(format!("webview-music-{tag}"));
            self.music_context = Some((tag, WebContext::new(Some(dir))));
        }
        true
    }

    fn new_tab_full(
        &mut self,
        url: Option<String>,
        activate: bool,
        force_proxy: bool,
        pinned: bool,
        incognito: bool,
    ) {
        let id = self.next_id;
        self.next_id += 1;

        let raw_url = url.clone();
        let newtab = self.newtab_html();
        let auto_music = raw_url.as_deref().map(music::is_music_url).unwrap_or(false);
        let initial_url = raw_url.clone().unwrap_or_else(|| "orvyn://newtab".to_string());
        if !activate && self.store.settings.lazy_tabs && !auto_music && !incognito {
            let pos = if pinned {
                self.tabs
                    .iter()
                    .rposition(|t| t.pinned)
                    .map(|i| i + 1)
                    .unwrap_or(0)
            } else {
                self.tabs.len()
            };
            self.tabs.insert(
                pos,
                Tab {
                    id,
                    webview: None,
                    title: "Новая вкладка".to_string(),
                    url: initial_url,
                    loading: false,
                    zoom: 1.0,
                    kind: TabKind::Web,
                    proxied: false,
                    pinned,
                    incognito: false,
                    muted: false,
                    last_active: Instant::now(),
                    heap_mb: 0.0,
                    group: String::new(),
                },
            );
            self.layout();
            self.save_session();
            self.refresh();
            return;
        }
        let want_proxy = !incognito
            && (force_proxy || auto_music)
            && self.store.settings.proxy_ready();
        let proxied = want_proxy && self.prepare_music_ctx();

        let bounds = self.content_rect();
        let env = self.chrome.environment();
        let args = self.args.clone();
        let inject = self.inject.clone();
        let proxy = self.proxy.clone();
        let guard = self.guard.clone();
        let perms = self.perms.clone();
        let dl_guard = self.dl_guard.clone();
        let dark = !self.store.settings.light();
        let target = raw_url.as_deref().filter(|u| !u.is_empty() && *u != "orvyn://newtab");

        let built = if proxied {
            let parts = self.store.settings.proxy_parts();
            let extra = match parts {
                Some((k, h, p)) => format!("{args} --proxy-server={k}://{h}:{p}"),
                None => args.clone(),
            };
            let ctx = &mut self.music_context.as_mut().unwrap().1;
            spawn_webview(
                &self.window, ctx, None, &extra, incognito, true, target, Some(&newtab), bounds,
                &inject, proxy, guard, perms, dl_guard, id, dark,
            )
        } else {
            spawn_webview(
                &self.window,
                &mut self.web_context,
                Some(env),
                &args,
                incognito,
                false,
                target,
                Some(&newtab),
                bounds,
                &inject,
                proxy,
                guard,
                perms,
                dl_guard,
                id,
                dark,
            )
        };

        let wv = match built {
            Ok(w) => w,
            Err(e) => {
                log_err(&format!("не удалось создать вкладку: {e}"));
                return;
            }
        };

        let initial_url = raw_url.unwrap_or_else(|| "orvyn://newtab".to_string());
        let tab = Tab {
            id,
            webview: Some(wv),
            title: "Новая вкладка".to_string(),
            url: initial_url,
            loading: true,
            zoom: 1.0,
            kind: TabKind::Web,
            proxied,
            pinned,
            incognito,
            muted: false,
            last_active: Instant::now(),
            heap_mb: 0.0,
            group: String::new(),
        };

        let pos = if pinned {
            self.tabs
                .iter()
                .rposition(|t| t.pinned)
                .map(|i| i + 1)
                .unwrap_or(0)
        } else {
            self.tabs.len()
        };
        self.tabs.insert(pos, tab);
        if activate || self.tabs.len() == 1 {
            let idx = self.tabs.iter().position(|t| t.id == id).unwrap_or(0);
            self.select(idx);
        } else {
            self.layout();
        }
        if !self.restoring {
            self.save_session();
        }
    }

    /// Восстанавливает webview выгруженной вкладки.
    fn ensure_loaded(&mut self, idx: usize) {
        if self.tabs.get(idx).map(|t| t.webview.is_some()).unwrap_or(false) {
            return;
        }
        let Some(t) = self.tabs.get(idx) else { return };
        if let TabKind::Search(_) = t.kind {
            return;
        }
        let id = t.id;
        let url = t.url.clone();
        let zoom = t.zoom;
        let incognito = t.incognito;
        let mut proxied = t.proxied && self.store.settings.proxy_ready();
        if proxied && !self.prepare_music_ctx() {
            proxied = false;
        }
        if let Some(t) = self.tabs.get_mut(idx) {
            t.proxied = proxied;
        }

        let newtab = self.newtab_html();
        let target = if url.is_empty() || url == "orvyn://newtab" {
            None
        } else {
            Some(url.as_str())
        };
        let bounds = self.content_rect();
        let env = self.chrome.environment();
        let args = self.args.clone();
        let inject = self.inject.clone();
        let proxy = self.proxy.clone();
        let guard = self.guard.clone();
        let perms = self.perms.clone();
        let dl_guard = self.dl_guard.clone();
        let dark = !self.store.settings.light();

        let built = if proxied {
            let parts = self.store.settings.proxy_parts();
            let extra = match parts {
                Some((k, h, p)) => format!("{args} --proxy-server={k}://{h}:{p}"),
                None => args.clone(),
            };
            let ctx = &mut self.music_context.as_mut().unwrap().1;
            spawn_webview(
                &self.window, ctx, None, &extra, incognito, true, target, Some(&newtab), bounds,
                &inject, proxy, guard, perms, dl_guard, id, dark,
            )
        } else {
            spawn_webview(
                &self.window,
                &mut self.web_context,
                Some(env),
                &args,
                incognito,
                false,
                target,
                Some(&newtab),
                bounds,
                &inject,
                proxy,
                guard,
                perms,
                dl_guard,
                id,
                dark,
            )
        };

        match built {
            Ok(wv) => {
                let _ = wv.zoom(zoom);
                let muted = self.tabs.get(idx).map(|t| t.muted).unwrap_or(false);
                if muted {
                    set_core_muted(&wv.webview(), true);
                }
                if let Some(t) = self.tabs.get_mut(idx) {
                    t.webview = Some(wv);
                    t.loading = true;
                }
            }
            Err(e) => {
                log_err(&format!("не удалось разбудить вкладку: {e}"));
                if let Some(t) = self.tabs.get_mut(idx) {
                    t.loading = false;
                }
            }
        }
    }

    fn select(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        self.active = idx;
        self.panel = None;
        self.ensure_loaded(idx);
        if let Some(t) = self.tabs.get_mut(idx) {
            t.last_active = Instant::now();
        }
        self.layout();
        if let Some(t) = self.tabs.get(idx) {
            if let Some(wv) = t.webview.as_ref() {
                let _ = wv.focus();
                let _ = wv.zoom(t.zoom);
            }
        }
        self.refresh();
    }

    fn close_tab(&mut self, id: usize) {
        let Some(pos) = self.tabs.iter().position(|t| t.id == id) else { return };
        let active_id = self.tabs.get(self.active).map(|t| t.id);
        let tab = self.tabs.remove(pos);
        if tab.webview.is_some() || tab.url.starts_with("http") {
            self.closed.push(ClosedTab {
                url: tab.url.clone(),
                title: tab.title.clone(),
                kind: tab.kind.clone(),
                incognito: tab.incognito,
                group: tab.group.clone(),
            });
            if self.closed.len() > 25 {
                self.closed.remove(0);
            }
        }
        if let Some(wv) = tab.webview.as_ref() {
            let _ = wv.set_visible(false);
        }
        drop(tab);
        if self.tabs.is_empty() {
            self.new_tab(None, true);
            return;
        }
        self.active = active_id
            .and_then(|aid| self.tabs.iter().position(|t| t.id == aid))
            .unwrap_or_else(|| self.active.min(self.tabs.len() - 1));
        self.select(self.active);
        self.save_session();
    }

    fn reopen_closed(&mut self) {
        let Some(c) = self.closed.pop() else { return };
        let before = self.next_id;
        match c.kind {
            TabKind::Search(q) => {
                self.new_tab_full(None, true, false, false, c.incognito);
                self.start_search(&q);
            }
            TabKind::Web => {
                let url = if c.url.is_empty() { None } else { Some(c.url.clone()) };
                self.new_tab_full(url, true, false, false, c.incognito);
            }
        }
        if let Some(t) = self.tabs.iter_mut().find(|t| t.id == before) {
            t.group = c.group;
        }
        let _ = c.title;
    }

    fn toggle_pin(&mut self, id: usize) {
        let Some(pos) = self.tabs.iter().position(|t| t.id == id) else { return };
        let active_id = self.tabs.get(self.active).map(|t| t.id);
        let mut tab = self.tabs.remove(pos);
        tab.pinned = !tab.pinned;
        let pinned = tab.pinned;
        let pos = if pinned {
            self.tabs
                .iter()
                .rposition(|t| t.pinned)
                .map(|i| i + 1)
                .unwrap_or(0)
        } else {
            self.tabs.len()
        };
        self.tabs.insert(pos, tab);
        self.active = active_id
            .and_then(|aid| self.tabs.iter().position(|t| t.id == aid))
            .unwrap_or(0);
        self.layout();
        self.refresh();
        self.save_session();
    }

    fn duplicate_tab(&mut self, id: usize) {
        let Some(t) = self.tabs.iter().find(|t| t.id == id) else { return };
        let url = t.url.clone();
        let incognito = t.incognito;
        let proxied = t.proxied;
        let group = t.group.clone();
        let before = self.next_id;
        self.new_tab_full(Some(url), true, proxied, false, incognito);
        if let Some(t) = self.tabs.iter_mut().find(|t| t.id == before) {
            t.group = group;
        }
        self.status = "Вкладка продублирована.".to_string();
        self.refresh();
    }

    /// Нужно ли спросить перед закрытием окна.
    fn should_warn_close(&self) -> bool {
        self.store.settings.close_warn
            && (self.tabs.len() > 1
                || self
                    .downloads
                    .iter()
                    .any(|d| d.state == "progress" || d.state == "paused" || d.state == "blocked"))
    }

    fn show_close_warning(&mut self) {
        self.panel = Some("close".to_string());
        self.layout();
        self.refresh();
    }

    fn request_close(&mut self) {
        if self.should_warn_close() {
            self.show_close_warning();
        } else {
            let _ = self.proxy.send_event(UserEvent::Quit);
        }
    }

    fn toggle_maximize(&mut self) {
        let maximize = !self.window.is_maximized();
        self.maximized = maximize;
        self.window.set_maximized(maximize);
        self.refresh();
    }

    fn sync_maximized(&mut self) {
        let maximized = self.window.is_maximized();
        if maximized != self.maximized {
            self.maximized = maximized;
            self.refresh();
        }
    }

    fn toggle_mute(&mut self, id: usize) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == id) else { return };
        if self.tabs[idx].webview.is_none() {
            self.status = "Вкладка спит. Открой её и повтори.".to_string();
            self.refresh();
            return;
        }
        let muted = !self.tabs[idx].muted;
        let ok = self.tabs[idx]
            .webview
            .as_ref()
            .map(|w| set_core_muted(&w.webview(), muted))
            .unwrap_or(false);
        if ok {
            self.tabs[idx].muted = muted;
            self.status = if muted {
                "Звук вкладки выключен.".to_string()
            } else {
                "Звук вкладки включён.".to_string()
            };
        } else {
            self.status = "Не удалось переключить звук вкладки.".to_string();
        }
        self.refresh();
    }

    fn find_in_page(&mut self) {
        if let Some(wv) = self.current_tab().and_then(|t| t.webview.as_ref()) {
            let _ = wv.evaluate_script(FIND_JS);
        } else {
            self.status = "Сначала открой страницу.".to_string();
        }
        self.panel = None;
        self.layout();
        self.refresh();
    }

    fn open_downloads_folder(&self) {
        let dir = capture::downloads_dir();
        let _ = std::process::Command::new("explorer").arg(dir).spawn();
    }

    fn show_qr(&mut self) {
        let Some(url) = self.current_tab().map(|t| t.url.clone()) else {
            self.status = "Сначала открой страницу.".to_string();
            self.refresh();
            return;
        };
        if url.is_empty() || url.starts_with("orvyn://") || url == "about:blank" {
            self.status = "Для внутренней страницы QR не нужен.".to_string();
            self.refresh();
            return;
        }
        let svg = qrcode::QrCode::new(url.as_bytes()).ok().map(|code| {
            code.render::<qrcode::render::svg::Color>()
                .min_dimensions(240, 240)
                .dark_color(qrcode::render::svg::Color("#0b1019"))
                .light_color(qrcode::render::svg::Color("#ffffff"))
                .build()
        });
        match svg {
            Some(svg) => {
                self.qr = Some((url, svg));
                self.panel = Some("qr".to_string());
                self.layout();
                self.status = "QR-код готов. Наведи камеру телефона.".to_string();
            }
            None => {
                self.status = "Не удалось собрать QR-код: адрес слишком длинный.".to_string();
            }
        }
        self.refresh();
    }

    fn reader_mode(&mut self) {
        if let Some(wv) = self.current_tab().and_then(|t| t.webview.as_ref()) {
            let _ = wv.evaluate_script(READER_JS);
        } else {
            self.status = "Сначала открой страницу.".to_string();
            self.refresh();
        }
        self.panel = None;
        self.layout();
        self.refresh();
    }

    fn translate_page(&mut self, lang: &str) {
        let Some(url) = self.current_tab().map(|t| t.url.clone()) else {
            self.status = "Сначала открой страницу.".to_string();
            self.refresh();
            return;
        };
        if !url.starts_with("http://") && !url.starts_with("https://") {
            self.status = "Перевод работает только для обычных сайтов.".to_string();
            self.refresh();
            return;
        }
        let lang = storage::clean_lang(lang);
        if self.store.settings.translate_lang != lang {
            self.store.settings.translate_lang = lang.clone();
            storage::save(&self.store);
        }
        let link = google_translate_url(&url, &lang);
        self.panel = None;
        self.new_tab_full(Some(link), true, false, false, false);
        self.status = format!("Открываю перевод ({lang}) в Google Translate.");
        self.refresh();
    }

    fn copy_selection(&mut self) {
        let Some(wv) = self.current_tab().and_then(|t| t.webview.as_ref()) else {
            self.status = "Сначала открой страницу.".to_string();
            self.refresh();
            return;
        };
        let _ = wv.evaluate_script(
            r#"(() => {
  const text = String(window.getSelection ? window.getSelection().toString() : "").trim();
  const toast = (m) => {
    let d = document.getElementById("__orvyn_toast");
    if (!d) {
      d = document.createElement("div");
      d.id = "__orvyn_toast";
      d.style.cssText = "position:fixed;left:50%;bottom:26px;transform:translateX(-50%);z-index:2147483647;padding:9px 14px;border-radius:10px;background:#111826;color:#e9eefb;border:1px solid #2a3850;font:13px system-ui;box-shadow:0 10px 30px rgba(0,0,0,.5);opacity:0;transition:opacity .15s";
      document.documentElement.appendChild(d);
    }
    d.textContent = m;
    d.style.opacity = "1";
    clearTimeout(window.__orvynToastT);
    window.__orvynToastT = setTimeout(() => { d.style.opacity = "0"; }, 1600);
  };
  if (!text) { toast("Нет выделенного текста"); return; }
  const ok = () => toast("Выделенное скопировано");
  const fallback = () => {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.cssText = "position:fixed;left:-9999px;top:0";
    document.body.appendChild(ta);
    ta.select();
    try { document.execCommand("copy"); } catch (e) {}
    ta.remove();
    ok();
  };
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(text).then(ok, fallback);
  } else fallback();
})();"#,
        );
        self.panel = None;
        self.layout();
        self.refresh();
    }

    fn copy_links(&mut self) {
        let Some(wv) = self.current_tab().and_then(|t| t.webview.as_ref()) else {
            self.status = "Сначала открой страницу.".to_string();
            self.refresh();
            return;
        };
        let _ = wv.evaluate_script(
            r#"(() => {
  const seen = new Set();
  const lines = [];
  for (const a of document.querySelectorAll("a[href]")) {
    const href = a.href;
    if (!/^https?:/i.test(href) || seen.has(href)) continue;
    seen.add(href);
    const label = (a.textContent || "").replace(/\s+/g, " ").trim().slice(0, 120);
    lines.push((label ? label + " — " : "") + href);
  }
  const text = lines.join("\n");
  const toast = (m) => {
    let d = document.getElementById("__orvyn_toast");
    if (!d) {
      d = document.createElement("div");
      d.id = "__orvyn_toast";
      d.style.cssText = "position:fixed;left:50%;bottom:26px;transform:translateX(-50%);z-index:2147483647;padding:9px 14px;border-radius:10px;background:#111826;color:#e9eefb;border:1px solid #2a3850;font:13px system-ui;box-shadow:0 10px 30px rgba(0,0,0,.5);opacity:0;transition:opacity .15s";
      document.documentElement.appendChild(d);
    }
    d.textContent = m;
    d.style.opacity = "1";
    clearTimeout(window.__orvynToastT);
    window.__orvynToastT = setTimeout(() => { d.style.opacity = "0"; }, 1600);
  };
  if (!lines.length) { toast("На странице нет ссылок"); return; }
  const ok = () => toast("Скопировано ссылок: " + lines.length);
  const fallback = () => {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.cssText = "position:fixed;left:-9999px;top:0";
    document.body.appendChild(ta);
    ta.select();
    try { document.execCommand("copy"); } catch (e) {}
    ta.remove();
    ok();
  };
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(text).then(ok, fallback);
  } else fallback();
})();"#,
        );
        self.panel = None;
        self.layout();
        self.refresh();
    }

    /// Просит страницу сообщить размеры, затем снимает её целиком.
    fn screenshot(&mut self) {
        let Some((id, wv)) = self
            .current_tab()
            .and_then(|t| t.webview.as_ref().map(|w| (t.id, w)))
        else {
            self.status = "Сначала открой страницу.".to_string();
            self.refresh();
            return;
        };
        let p = self.proxy.clone();
        let _ = wv.evaluate_script_with_callback(
            "({width: Math.max(document.documentElement.scrollWidth, window.innerWidth, 1), height: Math.max(document.documentElement.scrollHeight, window.innerHeight, 1)})",
            move |raw| {
                let Ok(v) = serde_json::from_str::<Value>(&raw) else { return };
                let width = v.get("width").and_then(|x| x.as_f64()).unwrap_or(0.0);
                let height = v.get("height").and_then(|x| x.as_f64()).unwrap_or(0.0);
                if width > 0.0 && height > 0.0 {
                    let _ = p.send_event(UserEvent::ShotSize { id, width, height });
                }
            },
        );
        self.status = "Готовлю скриншот страницы…".to_string();
        self.panel = None;
        self.layout();
        self.refresh();
    }

    fn capture_page(&self, id: usize, width: f64, height: f64) {
        let Some(wv) = self
            .tabs
            .iter()
            .find(|t| t.id == id)
            .and_then(|t| t.webview.as_ref())
        else {
            return;
        };
        let core = wv.webview();
        let w = width.clamp(1.0, 30000.0);
        let h = height.clamp(1.0, 30000.0);
        let params = json!({
            "format": "png",
            "captureBeyondViewport": true,
            "clip": { "x": 0, "y": 0, "width": w, "height": h, "scale": 1 }
        })
        .to_string();
        let method: Vec<u16> = "Page.captureScreenshot\0".encode_utf16().collect();
        let wide: Vec<u16> = params.encode_utf16().chain(std::iter::once(0)).collect();
        let p = self.proxy.clone();
        let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
            move |_hr, json| {
                let parsed = serde_json::from_str::<Value>(&json).ok();
                let data = parsed
                    .as_ref()
                    .and_then(|v| v.get("data"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match capture::save_png(data) {
                    Ok(path) => {
                        let _ = p.send_event(UserEvent::Saved {
                            kind: "Скриншот".to_string(),
                            path: path.display().to_string(),
                            id,
                        });
                    }
                    Err(e) => {
                        let _ = p.send_event(UserEvent::SaveError(format!(
                            "Не удалось сохранить скриншот: {e}"
                        )));
                    }
                }
                Ok(())
            },
        ));
        let _ = unsafe {
            core.CallDevToolsProtocolMethod(PCWSTR(method.as_ptr()), PCWSTR(wide.as_ptr()), &handler)
        };
    }

    /// Сохраняет страницу в один файл MHTML для офлайн-просмотра.
    fn save_page(&mut self) {
        let Some((id, wv)) = self
            .current_tab()
            .and_then(|t| t.webview.as_ref().map(|w| (t.id, w)))
        else {
            self.status = "Сначала открой страницу.".to_string();
            self.refresh();
            return;
        };
        let core = wv.webview();
        let method: Vec<u16> = "Page.captureSnapshot\0".encode_utf16().collect();
        let params = json!({ "format": "mhtml" }).to_string();
        let wide: Vec<u16> = params.encode_utf16().chain(std::iter::once(0)).collect();
        let p = self.proxy.clone();
        let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
            move |_hr, json| {
                let parsed = serde_json::from_str::<Value>(&json).ok();
                let data = parsed
                    .as_ref()
                    .and_then(|v| v.get("data"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match capture::save_mhtml(data) {
                    Ok(path) => {
                        let _ = p.send_event(UserEvent::Saved {
                            kind: "Страница сохранена".to_string(),
                            path: path.display().to_string(),
                            id,
                        });
                    }
                    Err(e) => {
                        let _ = p.send_event(UserEvent::SaveError(format!(
                            "Не удалось сохранить страницу: {e}"
                        )));
                    }
                }
                Ok(())
            },
        ));
        let _ = unsafe {
            core.CallDevToolsProtocolMethod(PCWSTR(method.as_ptr()), PCWSTR(wide.as_ptr()), &handler)
        };
        self.status = "Сохраняю страницу…".to_string();
        self.panel = None;
        self.layout();
        self.refresh();
    }

    /// Небольшое сообщение поверх страницы.
    fn page_toast(&self, id: usize, msg: &str) {
        let Some(wv) = self
            .tabs
            .iter()
            .find(|t| t.id == id)
            .and_then(|t| t.webview.as_ref())
        else {
            return;
        };
        let text = serde_json::to_string(msg).unwrap_or_else(|_| "\"\"".to_string());
        let _ = wv.evaluate_script(&format!(
            r#"(() => {{
  let d = document.getElementById("__orvyn_toast");
  if (!d) {{
    d = document.createElement("div");
    d.id = "__orvyn_toast";
    d.style.cssText = "position:fixed;left:50%;bottom:26px;transform:translateX(-50%);z-index:2147483647;max-width:80vw;padding:9px 14px;border-radius:10px;background:#111826;color:#e9eefb;border:1px solid #2a3850;font:13px system-ui;box-shadow:0 10px 30px rgba(0,0,0,.5);opacity:0;transition:opacity .15s;overflow:hidden;text-overflow:ellipsis;white-space:nowrap";
    document.documentElement.appendChild(d);
  }}
  d.textContent = {text};
  d.style.opacity = "1";
  clearTimeout(window.__orvynToastT);
  window.__orvynToastT = setTimeout(() => {{ d.style.opacity = "0"; }}, 2800);
}})();"#
        ));
    }

    fn discard(&mut self, idx: usize) -> bool {
        let active_id = self.tabs.get(self.active).map(|x| x.id).unwrap_or(0);
        let Some(t) = self.tabs.get_mut(idx) else { return false };
        if t.id == active_id {
            return false;
        }
        let Some(wv) = t.webview.take() else { return false };
        let _ = wv.set_visible(false);
        drop(wv);
        t.loading = false;
        true
    }

    fn can_discard(&self, idx: usize) -> bool {
        if idx == self.active {
            return false;
        }
        let Some(t) = self.tabs.get(idx) else { return false };
        if t.webview.is_none() || t.pinned || t.incognito {
            return false;
        }
        if t.kind != TabKind::Web {
            return false;
        }
        if music::is_music_url(&t.url) {
            return false;
        }
        !t.url.is_empty() && t.url != "orvyn://newtab"
    }

    /// Просит каждую загруженную вкладку прислать оценку JS-памяти.
    /// Дешёвая операция: выполняется только при открытом мониторе.
    fn collect_heap(&self) {
        if !self.store.settings.tab_metrics {
            return;
        }
        for t in &self.tabs {
            let Some(wv) = t.webview.as_ref() else { continue };
            let _ = wv.evaluate_script(
                "try { var m = performance.memory; var v = m ? m.usedJSHeapSize / 1048576 : 0; \
                 window.ipc.postMessage(JSON.stringify({ rb: \"heap\", mb: Math.round(v * 10) / 10 })); } catch (e) {}",
            );
        }
    }

    /// Применяет правило защиты сайта к уже загруженной вкладке.
    fn apply_site_policy(&self, id: usize) {
        let Some(t) = self.tabs.iter().find(|t| t.id == id) else { return };
        let Some(wv) = t.webview.as_ref() else { return };
        let Some(host) = security::host_of(&t.url) else { return };
        let on = self.store.protection_for(&host);
        let gestures = self.store.settings.mouse_gestures;
        let _ = wv.evaluate_script(&format!(
            "window.__orvynProtectSet && window.__orvynProtectSet({on}); \
             window.__orvynGesturesSet && window.__orvynGesturesSet({gestures});"
        ));
        if let Some(bits) = self
            .store
            .site_rules
            .iter()
            .find(|r| r.host.eq_ignore_ascii_case(&host))
            .and_then(|r| r.perms)
        {
            if let Some(origin) = security::origin_of(&t.url) {
                apply_bits_to_core(&wv.webview(), &origin, bits);
            }
        }
    }

    fn apply_all_site_policies(&self) {
        for t in &self.tabs {
            self.apply_site_policy(t.id);
        }
    }

    fn toggle_site_protection(&mut self) {
        let Some(t) = self.current_tab() else { return };
        let Some(host) = security::host_of(&t.url) else {
            self.status = "У этой страницы нет сайта для правила.".to_string();
            return;
        };
        let current = self.store.protection_for(&host);
        self.store.set_protection(&host, !current);
        storage::save(&self.store);
        self.apply_all_site_policies();
        self.status = if !current {
            format!("Защита включена для {host}.")
        } else {
            format!("Защита выключена для {host}.")
        };
        self.refresh();
    }

    fn toggle_site_perm(&mut self, name: &str, on: bool) {
        let Some(t) = self.current_tab() else { return };
        let Some(host) = security::host_of(&t.url) else {
            self.status = "У этой страницы нет сайта для разрешений.".to_string();
            return;
        };
        let bit = match name {
            "notif" => 1,
            "geo" => 2,
            "cam" => 4,
            "mic" => 8,
            _ => return,
        };
        let base = self.perms.mask();
        self.store.set_site_perm(&host, bit, on, base);
        storage::save(&self.store);
        self.perms.update_sites(&self.store.site_rules);
        let bits = self.store.perm_bits_for(&host, base);
        for t in &self.tabs {
            if security::host_of(&t.url).as_deref() == Some(host.as_str()) {
                if let (Some(wv), Some(origin)) =
                    (t.webview.as_ref(), security::origin_of(&t.url))
                {
                    apply_bits_to_core(&wv.webview(), &origin, bits);
                }
            }
        }
        let human = match name {
            "notif" => "уведомления",
            "geo" => "геолокация",
            "cam" => "камера",
            _ => "микрофон",
        };
        self.status = format!(
            "{human} для {host}: {}",
            if on { "разрешено" } else { "запрещено" }
        );
        self.refresh();
    }

    fn discard_by_id(&mut self, id: usize) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == id) else { return };
        if !self.can_discard(idx) {
            self.status = "Эту вкладку нельзя выгрузить: она активная, закреплена или играет музыку.".to_string();
            self.refresh();
            return;
        }
        self.sample_stats();
        let before = self.ram_mb;
        if self.discard(idx) {
            self.freed_pending = Some((Instant::now(), before));
            self.status = "Вкладка выгружена. Считаю освобождённую память…".to_string();
        }
        self.refresh();
    }

    fn discard_idle_now(&mut self) {
        let idle = self.store.settings.idle_minutes as u64;
        let max = Duration::from_secs(idle.max(1) * 60);
        let now = Instant::now();
        let mut targets = Vec::new();
        for i in 0..self.tabs.len() {
            if self.can_discard(i) && now.duration_since(self.tabs[i].last_active) >= max {
                targets.push(i);
            }
        }
        if targets.is_empty() {
            self.status = "Нет неактивных вкладок для выгрузки.".to_string();
            self.refresh();
            return;
        }
        self.sample_stats();
        let before = self.ram_mb;
        let mut n = 0;
        for i in targets.into_iter().rev() {
            if self.discard(i) {
                n += 1;
            }
        }
        if n > 0 {
            self.freed_pending = Some((Instant::now(), before));
            self.status = format!("Выгружено вкладок: {n}. Считаю освобождённую память…");
        }
        self.refresh();
    }

    /// Удаляет cookies, localStorage, IndexedDB и кэш текущего сайта.
    fn clear_site_data(&mut self) {
        let Some((url, core)) = self
            .current_tab()
            .and_then(|t| t.webview.as_ref().map(|w| (t.url.clone(), w.webview())))
        else {
            self.status = "Сайт ещё не загружен.".to_string();
            self.refresh();
            return;
        };
        let Some(origin) = security::origin_of(&url) else {
            self.status = "У этой страницы нет данных сайта.".to_string();
            self.refresh();
            return;
        };
        let host = security::host_of(&url).unwrap_or_default();
        let params = json!({ "origin": origin, "storageTypes": "all" }).to_string();
        let method: Vec<u16> = "Storage.clearDataForOrigin\0".encode_utf16().collect();
        let wide: Vec<u16> = params.encode_utf16().chain(std::iter::once(0)).collect();
        let handler =
            CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_hr, _json| Ok(())));
        let ok = unsafe {
            core.CallDevToolsProtocolMethod(PCWSTR(method.as_ptr()), PCWSTR(wide.as_ptr()), &handler)
        }
        .is_ok();

        let before = self.store.history.len();
        self.store
            .history
            .retain(|h| security::host_of(&h.url).as_deref() != Some(host.as_str()));
        if self.store.history.len() != before {
            storage::save(&self.store);
        }
        self.status = if ok {
            format!("Данные {host} удалены. История по этому сайту тоже очищена.")
        } else {
            format!("Не удалось удалить данные {host}.")
        };
        self.refresh();
    }

    /// Короткая проверка открытого сайта: HTTPS, подозрительность, защита, счётчик.
    fn quick_check(&mut self) {
        let Some(t) = self.current_tab() else { return };
        let url = t.url.clone();
        let host = security::host_of(&url).unwrap_or_default();
        if host.is_empty() || url.starts_with("orvyn://") {
            self.status = "Это внутренняя страница ORVYN, проверять нечего.".to_string();
            self.refresh();
            return;
        }
        let mut parts = Vec::new();
        parts.push(if url.starts_with("https://") {
            "HTTPS есть".to_string()
        } else {
            "без HTTPS".to_string()
        });
        if security::is_punycode_host(&host) {
            parts.push("punycode-домен".to_string());
        }
        if security::is_public_ip_host(&host) {
            parts.push("открыт по IP".to_string());
        }
        if security::is_bad_host(&host) {
            parts.push("в списке угроз".to_string());
        }
        if !self.store.protection_for(&host) {
            parts.push("защита выключена".to_string());
        }
        let n = self.blocked_site.get(&host).copied().unwrap_or(0);
        if n > 0 {
            parts.push(format!("заблокировано {n}"));
        }
        self.status = format!("Проверка {host}: {}.", parts.join(", "));
        self.refresh();
    }

    /// Собирает сохраняемый набор вкладок: (вкладки, позиция активной).
    fn current_session_tabs(&self) -> (Vec<storage::SessionTab>, usize) {
        let active_id = self.tabs.get(self.active).map(|t| t.id);
        let mut session = Vec::new();
        let mut active_pos = 0usize;
        for t in &self.tabs {
            if t.incognito || matches!(t.kind, TabKind::Search(_)) {
                continue;
            }
            if t.url.is_empty() {
                continue;
            }
            if t.id == active_id.unwrap_or(0) {
                active_pos = session.len();
            }
            session.push(storage::SessionTab {
                url: t.url.clone(),
                title: t.title.clone(),
                pinned: t.pinned,
                group: t.group.clone(),
            });
        }
        (session, active_pos)
    }

    fn save_session(&mut self) {
        let (full, active_pos) = self.current_session_tabs();
        storage::write_session_file(&full, active_pos);
        if let Some(id) = self.store.workspace_active {
            if let Some(w) = self.store.workspaces.iter_mut().find(|w| w.id == id) {
                w.tabs = full.clone();
                w.active = active_pos;
            }
        }
        self.store.session = if self.store.settings.restore_session {
            full
        } else {
            Vec::new()
        };
        self.store.session_active = active_pos;
        storage::save(&self.store);
    }

    /// Короткий отпечаток вкладок, чтобы не переписывать store.json без изменений.
    fn session_signature(&self) -> String {
        let mut s = String::with_capacity(self.tabs.len() * 48);
        s.push_str(&self.active.to_string());
        s.push('|');
        for t in &self.tabs {
            s.push_str(&t.id.to_string());
            s.push(',');
            s.push(if t.pinned { '1' } else { '0' });
            s.push(',');
            s.push(if t.incognito { '1' } else { '0' });
            s.push(',');
            s.push_str(&t.group);
            s.push(',');
            s.push_str(&t.url);
            s.push('\n');
        }
        s
    }

    /// Сообщает, что прошлый запуск не завершился штатно.
    fn report_crash(&mut self, recovered: usize) {
        crate::logging::warn(&format!(
            "прошлый запуск завершился аварийно, вкладок в сохранённой сессии: {recovered}"
        ));
        self.status = if recovered == 0 {
            "Прошлый запуск завершился аварийно. Вкладок для восстановления не было.".to_string()
        } else {
            format!("Прошлый запуск завершился аварийно. Восстановлено вкладок: {recovered}.")
        };
        self.panel = Some("recovered".to_string());
        self.recovery = recovered;
        self.layout();
        self.refresh();
    }

    fn workspace_new(&mut self, name: &str, icon: &str) {
        let name = name.trim().chars().take(40).collect::<String>();
        if name.is_empty() {
            self.status = "Дай пространству название.".to_string();
            self.refresh();
            return;
        }
        let icon = match icon {
            "work" | "study" | "game" | "home" | "globe" => icon,
            _ => "star",
        };
        let (tabs, active) = self.current_session_tabs();
        let id = self.store.workspaces.iter().map(|w| w.id).max().unwrap_or(0) + 1;
        self.store.workspaces.push(storage::Workspace {
            id,
            name: name.clone(),
            icon: icon.to_string(),
            tabs,
            active,
        });
        self.store.workspace_active = Some(id);
        storage::save(&self.store);
        self.status = format!("Пространство «{name}» создано из текущих вкладок.");
        self.refresh();
    }

    fn workspace_save(&mut self) {
        let Some(id) = self.store.workspace_active else {
            self.status = "Сначала открой рабочее пространство.".to_string();
            self.refresh();
            return;
        };
        let (tabs, active) = self.current_session_tabs();
        if let Some(w) = self.store.workspaces.iter_mut().find(|w| w.id == id) {
            w.tabs = tabs;
            w.active = active;
            self.status = format!("Пространство «{}» сохранено.", w.name);
        }
        storage::save(&self.store);
        self.refresh();
    }

    fn workspace_open(&mut self, id: u64) {
        if self.store.workspace_active == Some(id) {
            self.status = "Это пространство уже открыто.".to_string();
            self.refresh();
            return;
        }
        self.save_session();
        let Some(ws) = self.store.workspaces.iter().find(|w| w.id == id).cloned() else {
            return;
        };
        for t in &mut self.tabs {
            if let Some(wv) = t.webview.take() {
                let _ = wv.set_visible(false);
                drop(wv);
            }
        }
        self.tabs.clear();
        self.active = 0;
        self.closed.clear();
        self.store.workspace_active = Some(id);
        if ws.tabs.is_empty() {
            self.new_tab(None, true);
        } else {
            for s in &ws.tabs {
                let tab_id = self.next_id;
                self.next_id += 1;
                let pos = if s.pinned {
                    self.tabs
                        .iter()
                        .rposition(|t| t.pinned)
                        .map(|i| i + 1)
                        .unwrap_or(0)
                } else {
                    self.tabs.len()
                };
                self.tabs.insert(
                    pos,
                    Tab {
                        id: tab_id,
                        webview: None,
                        title: if s.title.is_empty() {
                            "Новая вкладка".into()
                        } else {
                            s.title.clone()
                        },
                        url: s.url.clone(),
                        loading: false,
                        zoom: 1.0,
                        kind: TabKind::Web,
                        proxied: false,
                        pinned: s.pinned,
                        incognito: false,
                        muted: false,
                        last_active: Instant::now(),
                        heap_mb: 0.0,
                        group: s.group.clone(),
                    },
                );
            }
            let idx = ws.active.min(self.tabs.len().saturating_sub(1));
            self.select(idx);
        }
        storage::save(&self.store);
        self.status = format!("Открыто пространство «{}».", ws.name);
        self.panel = None;
        self.layout();
        self.refresh();
    }

    fn workspace_rename(&mut self, id: u64, name: &str) {
        let name = name.trim().chars().take(40).collect::<String>();
        if name.is_empty() {
            return;
        }
        if let Some(w) = self.store.workspaces.iter_mut().find(|w| w.id == id) {
            w.name = name;
        }
        storage::save(&self.store);
        self.refresh();
    }

    fn workspace_cycle_icon(&mut self, id: u64) {
        const ICONS: [&str; 6] = ["star", "work", "study", "game", "home", "globe"];
        if let Some(w) = self.store.workspaces.iter_mut().find(|w| w.id == id) {
            let i = ICONS.iter().position(|x| *x == w.icon).unwrap_or(0);
            w.icon = ICONS[(i + 1) % ICONS.len()].to_string();
        }
        storage::save(&self.store);
        self.refresh();
    }

    fn workspace_delete(&mut self, id: u64) {
        self.store.workspaces.retain(|w| w.id != id);
        if self.store.workspace_active == Some(id) {
            self.store.workspace_active = None;
        }
        storage::save(&self.store);
        self.status = "Рабочее пространство удалено. Вкладки остались открытыми.".to_string();
        self.refresh();
    }

    fn set_tab_group(&mut self, id: usize, name: &str) {
        let name = storage::clean_group(name);
        if !self.tabs.iter().any(|t| t.id == id) {
            return;
        }
        if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
            t.group = name.clone();
        }
        self.save_session();
        self.status = if name.is_empty() {
            "Метка группы снята.".to_string()
        } else {
            format!("Вкладка в группе «{name}».")
        };
        self.refresh();
    }

    fn rename_group(&mut self, old: &str, new: &str) {
        let old = storage::clean_group(old);
        let new = storage::clean_group(new);
        if old.is_empty() {
            return;
        }
        if new.is_empty() {
            self.clear_group(&old);
            return;
        }
        let mut n = 0usize;
        for t in &mut self.tabs {
            if t.group.eq_ignore_ascii_case(&old) {
                t.group = new.clone();
                n += 1;
            }
        }
        if n > 0 {
            self.save_session();
        }
        self.status = format!("Группа «{old}» переименована в «{new}»: {n} вкладок.");
        self.refresh();
    }

    fn clear_group(&mut self, name: &str) {
        let name = storage::clean_group(name);
        if name.is_empty() {
            return;
        }
        let mut n = 0usize;
        for t in &mut self.tabs {
            if t.group.eq_ignore_ascii_case(&name) {
                t.group.clear();
                n += 1;
            }
        }
        if n > 0 {
            self.save_session();
        }
        self.status = format!("С группы «{name}» снято меток: {n}.");
        self.refresh();
    }

    fn focus_group(&mut self, name: &str) {
        let name = storage::clean_group(name);
        if let Some(idx) = self
            .tabs
            .iter()
            .position(|t| t.group.eq_ignore_ascii_case(&name))
        {
            self.select(idx);
        } else {
            self.status = format!("В группе «{name}» нет вкладок.");
            self.refresh();
        }
    }

    fn saved_set_save(&mut self, name: &str) {
        let name = name.trim().chars().take(40).collect::<String>();
        if name.is_empty() {
            self.status = "Дай набору название.".to_string();
            self.refresh();
            return;
        }
        let (tabs, _) = self.current_session_tabs();
        if tabs.is_empty() {
            self.status = "Нет вкладок, которые можно сохранить.".to_string();
            self.refresh();
            return;
        }
        let n = tabs.len();
        let set = storage::SavedSet {
            name: name.clone(),
            tabs,
            created: storage::now(),
        };
        if let Some(old) = self
            .store
            .saved_sets
            .iter_mut()
            .find(|s| s.name.eq_ignore_ascii_case(&name))
        {
            *old = set;
        } else {
            self.store.saved_sets.push(set);
        }
        storage::save(&self.store);
        self.status = format!("Набор «{name}» сохранён: {n} вкладок.");
        self.panel = Some("sets".to_string());
        self.layout();
        self.refresh();
    }

    fn saved_set_open(&mut self, name: &str) {
        let Some(set) = self
            .store
            .saved_sets
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
            .cloned()
        else {
            return;
        };
        let before = self.next_id;
        for s in &set.tabs {
            let id = self.next_id;
            self.new_tab_full(Some(s.url.clone()), false, false, s.pinned, false);
            if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                t.group = s.group.clone();
                if !s.title.is_empty() {
                    t.title = s.title.clone();
                }
            }
        }
        if let Some(idx) = self.tabs.iter().position(|t| t.id >= before) {
            self.select(idx);
        }
        self.status = format!("Набор «{}» открыт: {} вкладок.", set.name, set.tabs.len());
        self.refresh();
    }

    fn saved_set_rename(&mut self, old: &str, new: &str) {
        let new = new.trim().chars().take(40).collect::<String>();
        if new.is_empty() {
            return;
        }
        if self
            .store
            .saved_sets
            .iter()
            .any(|s| !s.name.eq_ignore_ascii_case(old) && s.name.eq_ignore_ascii_case(&new))
        {
            self.status = format!("Набор «{new}» уже есть.");
            self.refresh();
            return;
        }
        if let Some(s) = self
            .store
            .saved_sets
            .iter_mut()
            .find(|s| s.name.eq_ignore_ascii_case(old))
        {
            s.name = new.clone();
        }
        storage::save(&self.store);
        self.status = format!("Набор переименован в «{new}».");
        self.refresh();
    }

    fn saved_set_delete(&mut self, name: &str) {
        self.store
            .saved_sets
            .retain(|s| !s.name.eq_ignore_ascii_case(name));
        storage::save(&self.store);
        self.status = "Набор вкладок удалён.".to_string();
        self.refresh();
    }

    fn ai_persist(&mut self) {
        if self.store.settings.ai_save {
            self.store.ai_history = self.ai_history.clone();
        } else {
            self.store.ai_history.clear();
        }
        storage::save(&self.store);
    }

    fn ai_send(&mut self, text: &str) {
        if !self.store.settings.ai_on {
            self.ai_error = "ORVYN AI выключен в настройках.".to_string();
            self.panel = Some("ai".to_string());
            self.layout();
            self.refresh();
            return;
        }
        if self.ai_busy {
            return;
        }
        let mut text = text.trim().chars().take(8_000).collect::<String>();
        if text.is_empty() {
            return;
        }
        self.ai_history.push(storage::AiMessage {
            role: "user".to_string(),
            content: text.clone(),
        });
        if self.ai_history.len() > 40 {
            let cut = self.ai_history.len() - 40;
            self.ai_history.drain(0..cut);
        }
        self.ai_persist();
        self.ai_busy = true;
        self.ai_error.clear();
        self.panel = Some("ai".to_string());
        self.layout();
        self.status = "ORVYN AI думает…".to_string();
        let messages = self.ai_request_messages();
        let p = self.proxy.clone();
        std::thread::spawn(move || {
            let res = ai::ask(&messages);
            let _ = p.send_event(UserEvent::AiReply(res));
        });
        text.clear();
        self.refresh();
    }

    fn ai_request_messages(&self) -> Vec<ai::Message> {
        let mut out = vec![ai::Message {
            role: "system".to_string(),
            content: "Ты ORVYN AI — быстрый встроенный помощник браузера ORVYN. Отвечай кратко и по делу на языке пользователя; код и шаги — только когда просят. Не выдумывай факты и не проси пароли.".to_string(),
        }];
        if let Some(ctx) = &self.ai_context {
            out.push(ai::Message {
                role: "system".to_string(),
                content: format!(
                    "Пользователь явно разрешил использовать текст открытой страницы как контекст. Вот этот текст:\n\n{ctx}"
                ),
            });
        }
        let start = self.ai_history.len().saturating_sub(18);
        for m in &self.ai_history[start..] {
            out.push(ai::Message {
                role: m.role.clone(),
                content: m.content.clone(),
            });
        }
        out
    }

    fn ai_context_grab(&mut self) {
        let Some(wv) = self.current_tab().and_then(|t| t.webview.as_ref()) else {
            self.ai_error = "Сначала открой страницу.".to_string();
            self.refresh();
            return;
        };
        let p = self.proxy.clone();
        let _ = wv.evaluate_script_with_callback(
            "document.body ? document.body.innerText.slice(0, 10000) : ''",
            move |raw| {
                let text = serde_json::from_str::<String>(&raw).unwrap_or(raw);
                let _ = p.send_event(UserEvent::AiPageText(text));
            },
        );
        self.status = "Беру текст страницы…".to_string();
        self.refresh();
    }

    fn ai_clear(&mut self) {
        self.ai_history.clear();
        self.ai_error.clear();
        self.store.ai_history.clear();
        storage::save(&self.store);
        self.status = "История ORVYN AI очищена.".to_string();
        self.refresh();
    }

    fn ai_clear_context(&mut self) {
        self.ai_context = None;
        self.status = "Контекст страницы убран.".to_string();
        self.refresh();
    }

    fn theme_profile(&self, name: &str) -> storage::ThemeProfile {
        let s = &self.store.settings;
        storage::ThemeProfile {
            version: 2,
            name: name.to_string(),
            theme: s.theme.clone(),
            accent: s.accent.clone(),
            accent2: s.accent2.clone(),
            tab_shape: s.tab_shape.clone(),
            ui_radius: s.ui_radius,
            ui_font: s.ui_font.clone(),
            ui_opacity: s.ui_opacity,
            ui_blur: s.ui_blur,
            wallpaper: s.wallpaper.clone(),
        }
    }

    fn appearance_json(&self) -> String {
        serde_json::to_string_pretty(&self.theme_profile("ORVYN theme")).unwrap_or_default()
    }

    fn apply_theme_profile(&mut self, p: &storage::ThemeProfile) {
        let s = &mut self.store.settings;
        s.theme = p.theme.clone();
        s.accent = p.accent.clone();
        s.accent2 = p.accent2.clone();
        s.tab_shape = p.tab_shape.clone();
        s.ui_radius = p.ui_radius;
        s.ui_font = p.ui_font.clone();
        s.ui_opacity = p.ui_opacity;
        s.ui_blur = p.ui_blur;
        s.wallpaper = p.wallpaper.clone();
        s.sanitize_appearance();
        self.reload_newtab();
        self.layout();
        self.refresh();
    }

    fn theme_save(&mut self, name: &str) {
        let name = name.trim().chars().take(40).collect::<String>();
        if name.is_empty() {
            self.status = "Дай профилю название.".to_string();
            self.refresh();
            return;
        }
        let p = self.theme_profile(&name);
        if let Some(old) = self
            .store
            .themes
            .iter_mut()
            .find(|x| x.name.eq_ignore_ascii_case(&name))
        {
            *old = p;
        } else {
            self.store.themes.push(p);
        }
        storage::save(&self.store);
        self.status = format!("Профиль «{name}» сохранён.");
        self.refresh();
    }

    fn theme_preset_apply(&mut self, id: &str) {
        let Some(p) = storage::theme_preset(id) else {
            self.status = "Неизвестный пресет оформления.".to_string();
            self.refresh();
            return;
        };
        self.apply_theme_profile(&p);
        storage::save(&self.store);
        self.status = format!(
            "Пресет «{}» применён. Сохрани как профиль, если хочешь оставить его в списке.",
            p.name
        );
        self.refresh();
    }

    fn theme_apply(&mut self, name: &str) {
        let Some(p) = self
            .store
            .themes
            .iter()
            .find(|x| x.name.eq_ignore_ascii_case(name))
            .cloned()
        else {
            return;
        };
        self.apply_theme_profile(&p);
        storage::save(&self.store);
        self.status = format!("Профиль «{}» применён.", p.name);
        self.refresh();
    }

    fn theme_delete(&mut self, name: &str) {
        self.store.themes.retain(|x| !x.name.eq_ignore_ascii_case(name));
        storage::save(&self.store);
        self.status = "Профиль оформления удалён.".to_string();
        self.refresh();
    }

    fn theme_reset(&mut self) {
        let d = storage::Settings::default();
        let s = &mut self.store.settings;
        s.theme = d.theme;
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
        self.reload_newtab();
        self.layout();
        storage::save(&self.store);
        self.status = "Оформление сброшено.".to_string();
        self.refresh();
    }

    fn theme_export(&mut self) {
        let mut p = self.theme_profile("ORVYN theme");
        p.name = "ORVYN theme".to_string();
        let Some(path) = rfd::FileDialog::new()
            .set_title("Сохранить оформление ORVYN")
            .set_file_name("orvyn-theme.json")
            .add_filter("ORVYN theme", &["json"])
            .save_file()
        else {
            self.status = "Экспорт оформления отменён.".to_string();
            self.refresh();
            return;
        };
        self.status = match serde_json::to_string_pretty(&p)
            .map_err(|e| e.to_string())
            .and_then(|text| std::fs::write(&path, text).map_err(|e| e.to_string()))
        {
            Ok(()) => format!("Оформление сохранено: {}", path.display()),
            Err(e) => format!("Не удалось сохранить оформление: {e}"),
        };
        self.refresh();
    }

    fn theme_import(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Открыть оформление ORVYN")
            .add_filter("ORVYN theme", &["json"])
            .pick_file()
        else {
            self.status = "Импорт оформления отменён.".to_string();
            self.refresh();
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                self.status = format!("Не удалось прочитать файл: {e}");
                self.refresh();
                return;
            }
        };
        let mut p: storage::ThemeProfile = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(e) => {
                self.status = format!("Это не файл оформления ORVYN: {e}");
                self.refresh();
                return;
            }
        };
        p.name = "Импорт".to_string();
        let name = format!("Импорт {}", storage::now() % 10000);
        p.name = name;
        storage::normalize_theme(&mut p);
        self.apply_theme_profile(&p);
        if !self.store.themes.iter().any(|x| x.name == p.name) {
            self.store.themes.push(p.clone());
        }
        storage::save(&self.store);
        self.status = format!("Оформление «{}» применено.", p.name);
        self.refresh();
    }

    fn warp_check(&mut self) {
        if warp_cli_path().is_none() {
            self.warp_status = "Missing".to_string();
            self.warp_error = "Cloudflare WARP не установлен".to_string();
            self.refresh();
            return;
        }
        self.warp_last_check = Instant::now();
        let p = self.proxy.clone();
        std::thread::spawn(move || {
            match warp_run(&["-j", "status"]) {
                Ok(out) => {
                    let v: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
                    let raw = v
                        .get("status")
                        .and_then(|x| x.as_str())
                        .unwrap_or_else(|| out.trim())
                        .to_string();
                    let reason = v
                        .get("reason")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    let label = match raw.as_str() {
                        "Connected" | "Disconnected" | "Connecting" | "Disconnecting" => raw,
                        other if other.contains("Connected") => "Connected".to_string(),
                        other if other.contains("Disconnected") => "Disconnected".to_string(),
                        _ => "Unknown".to_string(),
                    };
                    let _ = p.send_event(UserEvent::WarpStatus(label, reason));
                }
                Err(e) => {
                    let _ = p.send_event(UserEvent::WarpStatus("Error".to_string(), e));
                }
            }
        });
    }

    fn warp_action(&mut self, action: &str) {
        if warp_cli_path().is_none() {
            self.warp_error = "Cloudflare WARP не установлен".to_string();
            self.warp_status = "Missing".to_string();
            self.refresh();
            return;
        }
        if self.warp_busy {
            return;
        }
        self.warp_busy = true;
        self.warp_error.clear();
        self.status = match action {
            "connect" => "Подключаю WARP…".to_string(),
            "disconnect" => "Отключаю WARP…".to_string(),
            "reconnect" => "Переподключаю WARP…".to_string(),
            _ => String::new(),
        };
        let action = action.to_string();
        let p = self.proxy.clone();
        std::thread::spawn(move || {
            let res = match action.as_str() {
                "connect" => warp_run(&["connect"]).map(|_| "WARP подключён.".to_string()),
                "disconnect" => warp_run(&["disconnect"]).map(|_| "WARP отключён.".to_string()),
                "reconnect" => {
                    let _ = warp_run(&["disconnect"]);
                    std::thread::sleep(Duration::from_millis(900));
                    warp_run(&["connect"]).map(|_| "WARP переподключён.".to_string())
                }
                _ => Err("неизвестное действие WARP".to_string()),
            };
            let _ = p.send_event(UserEvent::WarpAction(res));
        });
        self.refresh();
    }

    fn warp_open(&self) {
        let base = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".to_string());
        let exe = PathBuf::from(base)
            .join("Cloudflare")
            .join("Cloudflare WARP")
            .join("Cloudflare WARP.exe");
        if exe.exists() {
            let _ = Command::new(exe).spawn();
        } else {
            self.warp_install();
        }
    }

    fn warp_install(&self) {
        let _ = Command::new("explorer")
            .arg("https://1.1.1.1/warp/")
            .spawn();
    }

    fn auto_warp_if_due(&mut self) {
        if !self.store.settings.warp_auto || warp_cli_path().is_none() {
            return;
        }
        self.warp_auto_pending = true;
        self.warp_check();
    }

    fn drop_all_tabs(&mut self) {
        for t in &mut self.tabs {
            if let Some(wv) = t.webview.take() {
                let _ = wv.set_visible(false);
                drop(wv);
            }
        }
        self.tabs.clear();
        self.active = 0;
        self.closed.clear();
    }

    fn backup_export(&mut self) {
        let s = &self.store.settings;
        let mut b = backup::Backup::new();
        if s.backup_bookmarks {
            b.bookmarks = Some(self.store.bookmarks.clone());
        }
        if s.backup_settings {
            b.settings = Some(self.store.settings.clone());
        }
        if s.backup_history {
            b.history = Some(self.store.history.clone());
        }
        if s.backup_session {
            b.session = Some(self.store.session.clone());
        }
        if s.backup_spaces {
            b.workspaces = Some(self.store.workspaces.clone());
        }
        if s.backup_sets {
            b.saved_sets = Some(self.store.saved_sets.clone());
        }
        if b.is_empty() {
            self.status = "Отметь хотя бы один пункт для копии.".to_string();
            self.refresh();
            return;
        }
        let name = format!("orvyn-backup-{}.json", storage::now());
        let Some(path) = rfd::FileDialog::new()
            .set_title("Сохранить резервную копию ORVYN")
            .set_file_name(&name)
            .add_filter("ORVYN JSON", &["json"])
            .save_file()
        else {
            self.status = "Экспорт отменён.".to_string();
            self.refresh();
            return;
        };
        self.status = match backup::write(&path, &b) {
            Ok(()) => format!("Копия сохранена: {}", path.display()),
            Err(e) => format!("Не удалось сохранить копию: {e}"),
        };
        self.refresh();
    }

    fn backup_import(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Открыть резервную копию ORVYN")
            .add_filter("ORVYN JSON", &["json"])
            .pick_file()
        else {
            self.status = "Импорт отменён.".to_string();
            self.refresh();
            return;
        };
        let b = match backup::read(&path) {
            Ok(b) => b,
            Err(e) => {
                self.status = format!("Не удалось прочитать копию: {e}");
                self.refresh();
                return;
            }
        };
        let replace = self.store.settings.backup_replace;
        let mut parts: Vec<String> = Vec::new();

        if let Some(bm) = b.bookmarks {
            if replace {
                let n = bm.len();
                self.store.bookmarks = bm;
                parts.push(format!("закладки: {n}"));
            } else {
                let mut n = 0usize;
                for x in bm {
                    if !self.store.bookmarks.iter().any(|y| y.url == x.url) {
                        self.store.bookmarks.push(x);
                        n += 1;
                    }
                }
                parts.push(format!("закладки: +{n}"));
            }
        }

        if let Some(h) = b.history {
            if replace {
                let n = h.len();
                self.store.history = h;
                parts.push(format!("история: {n}"));
            } else {
                let mut n = 0usize;
                for x in h {
                    if !self
                        .store
                        .history
                        .iter()
                        .any(|y| y.url == x.url && y.ts == x.ts)
                    {
                        self.store.history.push(x);
                        n += 1;
                    }
                }
                parts.push(format!("история: +{n}"));
            }
            self.store.history.retain(|e| !e.url.is_empty());
            self.store.history.sort_by_key(|e| e.ts);
            if self.store.history.len() > 2000 {
                let cut = self.store.history.len() - 2000;
                self.store.history.drain(0..cut);
            }
        }

        // История и закладки живут в SQLite: переносим итог импорта.
        crate::db::replace_bookmarks(&self.store.bookmarks);
        crate::db::replace_history(&self.store.history);

        if let Some(ws) = b.workspaces {
            if replace {
                let n = ws.len();
                self.store.workspaces = ws;
                self.store.workspace_active = None;
                parts.push(format!("пространства: {n}"));
            } else {
                let mut next = self.store.workspaces.iter().map(|w| w.id).max().unwrap_or(0) + 1;
                let mut n = 0usize;
                for mut w in ws {
                    if let Some(old) = self
                        .store
                        .workspaces
                        .iter_mut()
                        .find(|x| x.name.eq_ignore_ascii_case(&w.name))
                    {
                        for t in w.tabs {
                            if !old.tabs.iter().any(|x| x.url == t.url) {
                                old.tabs.push(t);
                            }
                        }
                    } else {
                        w.id = next;
                        next += 1;
                        self.store.workspaces.push(w);
                        n += 1;
                    }
                }
                parts.push(format!("пространства: +{n}"));
            }
        }

        if let Some(sets) = b.saved_sets {
            if replace {
                let n = sets.len();
                self.store.saved_sets = sets;
                parts.push(format!("наборы вкладок: {n}"));
            } else {
                let mut n = 0usize;
                for s in sets {
                    if let Some(old) = self
                        .store
                        .saved_sets
                        .iter_mut()
                        .find(|x| x.name.eq_ignore_ascii_case(&s.name))
                    {
                        for t in s.tabs {
                            if !old.tabs.iter().any(|x| x.url == t.url) {
                                old.tabs.push(t);
                            }
                        }
                    } else {
                        self.store.saved_sets.push(s);
                        n += 1;
                    }
                }
                parts.push(format!("наборы вкладок: +{n}"));
            }
        }

        if let Some(st) = b.settings {
            self.store.settings = st;
            self.apply_imported_settings();
            parts.push("настройки".to_string());
        }

        if let Some(sess) = b.session {
            if replace && !sess.is_empty() {
                self.drop_all_tabs();
                self.store.workspace_active = None;
                self.load_session_tabs(sess, 0);
                parts.push("вкладки открыты".to_string());
            } else if !replace {
                let mut n = 0usize;
                for t in sess {
                    if !self.store.session.iter().any(|x| x.url == t.url) {
                        self.store.session.push(t);
                        n += 1;
                    }
                }
                parts.push(format!("сессия: +{n}"));
            }
        }

        storage::save(&self.store);
        self.status = if parts.is_empty() {
            "В копии не было отмеченных данных.".to_string()
        } else {
            format!("Восстановлено: {}.", parts.join(", "))
        };
        self.panel = Some("backup".to_string());
        self.layout();
        self.refresh();
    }

    fn apply_imported_settings(&mut self) {
        self.store.settings.translate_lang =
            storage::clean_lang(&self.store.settings.translate_lang);
        self.inject = adblock::inject_script(
            self.store.settings.protection_on,
            self.store.settings.mouse_gestures,
        );
        self.guard.set_strict(self.store.settings.redirect_guard);
        self.dl_guard
            .store(self.store.settings.download_guard, Ordering::Relaxed);
        self.perms.update(&self.store.settings);
        self.perms.update_sites(&self.store.site_rules);
        self.apply_all_site_policies();
        self.reload_newtab();
    }

    fn restore_session(&mut self) -> bool {
        if !self.store.settings.restore_session {
            return false;
        }
        let saved = self.store.session.clone();
        if saved.is_empty() {
            return false;
        }
        let active = self.store.session_active;
        self.load_session_tabs(saved, active);
        true
    }

    fn load_session_tabs(&mut self, saved: Vec<storage::SessionTab>, active: usize) -> bool {
        self.restoring = true;
        for s in saved {
            if s.url.is_empty() {
                continue;
            }
            let id = self.next_id;
            self.next_id += 1;
            let pos = if s.pinned {
                self.tabs
                    .iter()
                    .rposition(|t| t.pinned)
                    .map(|i| i + 1)
                    .unwrap_or(0)
            } else {
                self.tabs.len()
            };
            self.tabs.insert(
                pos,
                Tab {
                    id,
                    webview: None,
                    title: if s.title.is_empty() { "Новая вкладка".into() } else { s.title },
                    url: s.url,
                    loading: false,
                    zoom: 1.0,
                    kind: TabKind::Web,
                    proxied: false,
                    pinned: s.pinned,
                    incognito: false,
                    muted: false,
                    last_active: Instant::now(),
                    heap_mb: 0.0,
                    group: s.group.clone(),
                },
            );
        }
        self.restoring = false;
        if self.tabs.is_empty() {
            return false;
        }
        let idx = active.min(self.tabs.len() - 1);
        self.select(idx);
        true
    }

    fn resolve(&self, text: &str) -> Target {
        let t = text.trim();
        if t.is_empty() {
            return Target::Url(self.store.settings.homepage.clone());
        }
        let lower = t.to_ascii_lowercase();
        for (prefix, host) in [
            ("!g ", "https://www.google.com/search?q="),
            ("!d ", "https://duckduckgo.com/?q="),
            ("!y ", "https://yandex.ru/search/?text="),
            ("!b ", "https://www.bing.com/search?q="),
            ("!w ", "https://ru.wikipedia.org/w/index.php?search="),
        ] {
            if lower.starts_with(prefix) {
                let q = t[prefix.len()..].trim();
                if !q.is_empty() {
                    return Target::Url(format!("{host}{}", encode_query(q)));
                }
            }
        }
        for scheme in [
            "http://",
            "https://",
            "file://",
            "about:",
            "data:",
            "view-source:",
            "orvyn://",
            "orbit:",
            "rustbrowser://",
        ] {
            if lower.starts_with(scheme) {
                let fixed = if lower.starts_with("rustbrowser://") {
                    format!("orvyn://{}", &t["rustbrowser://".len()..])
                } else if lower.starts_with("orbit://") {
                    format!("orvyn://{}", &t["orbit://".len()..])
                } else if lower.starts_with("orbit:") {
                    format!("orvyn:{}", &t["orbit:".len()..])
                } else {
                    t.to_string()
                };
                return Target::Url(fixed);
            }
        }
        if t.contains(char::is_whitespace) {
            return Target::Search(t.to_string());
        }
        if is_local_host(t) {
            return Target::Url(format!("http://{t}"));
        }
        let host = t.split('/').next().unwrap_or(t);
        let tld = host.rsplit('.').next().unwrap_or("");
        let has_tld = host.contains('.')
            && tld.len() >= 2
            && tld.chars().all(|c| c.is_ascii_alphabetic());
        if has_tld {
            Target::Url(format!("https://{t}"))
        } else {
            Target::Search(t.to_string())
        }
    }

    fn navigate(&mut self, text: &str) {
        self.panel = None;
        match self.resolve(text) {
            Target::Url(url) => {
                let is_music = music::is_music_url(&url);
                let proxy_ready = self.store.settings.proxy_ready();
                let current_proxied = self
                    .tabs
                    .get(self.active)
                    .map(|t| t.proxied)
                    .unwrap_or(false);
                let current_blank = self
                    .tabs
                    .get(self.active)
                    .map(|t| t.url == "orvyn://newtab" || t.url.is_empty())
                    .unwrap_or(false);
                let current_incognito = self
                    .tabs
                    .get(self.active)
                    .map(|t| t.incognito)
                    .unwrap_or(false);
                let old_id = self.tabs.get(self.active).map(|t| t.id);

                if (is_music && proxy_ready && !current_proxied && !current_incognito)
                    || (current_proxied && !is_music && !url.starts_with("orvyn://"))
                {
                    let proxied = is_music && proxy_ready;
                    self.new_tab_full(Some(url), true, proxied, false, current_incognito);
                    if current_blank {
                        if let Some(id) = old_id {
                            self.close_tab(id);
                        }
                    }
                    self.layout();
                    self.refresh();
                    return;
                }

                self.ensure_loaded(self.active);
                let newtab = if url == "orvyn://newtab" {
                    Some(self.newtab_html())
                } else {
                    None
                };
                if let Some(t) = self.tabs.get_mut(self.active) {
                    t.kind = TabKind::Web;
                    t.loading = true;
                    if let Some(wv) = t.webview.as_ref() {
                        if let Some(html) = newtab {
                            t.url = url;
                            t.title = "Новая вкладка".to_string();
                            let _ = wv.load_html(&html);
                        } else {
                            let _ = wv.load_url(&url);
                        }
                    }
                }
                self.layout();
                self.refresh();
            }
            Target::Search(q) => {
                self.layout();
                self.start_search(&q);
            }
        }
    }

    fn start_search(&mut self, q: &str) {
        self.start_search_at(self.active, q);
    }

    fn start_search_at(&mut self, mut idx: usize, q: &str) {
        if self.tabs.get(idx).map(|t| t.proxied).unwrap_or(false) {
            self.new_tab_full(None, true, false, false, false);
            idx = self.active;
        }
        let Some(id) = self.tabs.get(idx).map(|t| t.id) else { return };
        self.ensure_loaded(idx);

        let engine = self.store.settings.engine().to_string();
        if engine != "own" {
            let url = if engine == "google" {
                format!("https://www.google.com/search?q={}", encode_query(q))
            } else {
                format!("https://duckduckgo.com/?q={}", encode_query(q))
            };
            if let Some(t) = self.tabs.get_mut(idx) {
                t.kind = TabKind::Web;
                t.url = url.clone();
                t.title = format!("Поиск: {q}");
                t.loading = true;
                if let Some(wv) = t.webview.as_ref() {
                    let _ = wv.load_url(&url);
                }
            }
            self.panel = None;
            self.layout();
            self.refresh();
            return;
        }

        let url = format!("orvyn://search?q={}", encode_query(q));
        if let Some(t) = self.tabs.get_mut(idx) {
            t.kind = TabKind::Search(q.to_string());
            t.url = url.clone();
            t.title = format!("Поиск: {q}");
            t.loading = true;
            if let Some(wv) = t.webview.as_ref() {
                let _ = wv.load_html(&search::loading_page(q));
            }
        }
        self.refresh();

        let proxy = self.proxy.clone();
        let query = q.to_string();
        let light = self.store.settings.light();
        std::thread::spawn(move || {
            let html = match search::search(&query) {
                Ok(hits) => search::results_page(&query, &hits, None, light),
                Err(e) => search::results_page(&query, &[], Some(&e), light),
            };
            let _ = proxy.send_event(UserEvent::SearchDone { id, q: query, html });
        });
    }

    fn cycle(&mut self, delta: i64) {
        if self.tabs.is_empty() {
            return;
        }
        let n = self.tabs.len() as i64;
        let next = ((self.active as i64 + delta) % n + n) % n;
        self.select(next as usize);
    }

    fn reload_active(&mut self) {
        let Some(t) = self.tabs.get(self.active) else { return };
        if let TabKind::Search(q) = t.kind.clone() {
            self.start_search(&q);
            return;
        }
        if let Some(wv) = t.webview.as_ref() {
            let _ = wv.reload();
        } else {
            self.ensure_loaded(self.active);
            self.layout();
        }
    }

    fn is_bookmarked(&self, url: &str) -> bool {
        self.store.bookmarks.iter().any(|b| b.url == url)
    }

    fn current_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    fn toggle_bookmark(&mut self) {
        let Some(tab) = self.current_tab() else { return };
        let url = tab.url.clone();
        let title = tab.title.clone();
        if url.is_empty() || url.starts_with("orvyn://") || url.starts_with("rustbrowser://") {
            return;
        }
        if self.is_bookmarked(&url) {
            self.store.bookmarks.retain(|b| b.url != url);
            crate::db::remove_bookmark(&url);
        } else {
            crate::db::upsert_bookmark(&url, &title);
            self.store.bookmarks.push(storage::Bookmark { url, title });
        }
        storage::save(&self.store);
        self.refresh();
    }

    fn webview_pids(&self) -> Vec<u32> {
        let env = self.chrome.environment();
        let Ok(e8) = env.cast::<ICoreWebView2Environment8>() else {
            return Vec::new();
        };
        let Ok(col) = (unsafe { e8.GetProcessInfos() }) else {
            return Vec::new();
        };
        let mut count = 0u32;
        let _ = unsafe { col.Count(&mut count) };
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            if let Ok(pi) = unsafe { col.GetValueAtIndex(i) } {
                let mut pid = 0i32;
                if unsafe { pi.ProcessId(&mut pid) }.is_ok() && pid > 0 {
                    out.push(pid as u32);
                }
            }
        }
        out
    }

    fn sample_stats(&mut self) {
        let now = Instant::now();
        let s = monitor::sample(&self.webview_pids());
        let dt = now.duration_since(self.last_sample_at);
        if s.cpu_ticks > 0 {
            self.cpu_pct = monitor::cpu_percent(self.last_sample.cpu_ticks, s.cpu_ticks, dt);
        }
        self.ram_mb = s.ram_bytes as f64 / (1024.0 * 1024.0);
        self.procs = s.procs;
        self.last_sample = s;
        self.last_sample_at = now;
    }

    fn discard_idle(&mut self) {
        if !self.store.settings.memory_saver {
            return;
        }
        let idle = self.store.settings.idle_minutes;
        let limit = self.store.settings.ram_limit_mb;
        let now = Instant::now();

        if limit > 0 {
            self.sample_stats();
            if self.ram_mb > limit as f64 {
                let mut best: Option<(usize, Instant)> = None;
                for i in 0..self.tabs.len() {
                    if !self.can_discard(i) {
                        continue;
                    }
                    let at = self.tabs[i].last_active;
                    if best.map(|(_, b)| at < b).unwrap_or(true) {
                        best = Some((i, at));
                    }
                }
                if let Some((i, _)) = best {
                    self.discard(i);
                    self.refresh();
                }
            }
        }

        if idle > 0 {
            let max = Duration::from_secs(idle as u64 * 60);
            let mut any = false;
            for i in 0..self.tabs.len() {
                if self.can_discard(i) && now.duration_since(self.tabs[i].last_active) >= max {
                    any |= self.discard(i);
                }
            }
            if any {
                self.refresh();
            }
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        if self.panel.as_deref() == Some("monitor")
            && now.duration_since(self.last_sample_at) >= Duration::from_millis(900)
        {
            self.sample_stats();
            self.refresh();
        }
        if self.panel.as_deref() == Some("monitor")
            && self.store.settings.tab_metrics
            && now.duration_since(self.last_heap) >= Duration::from_millis(2500)
        {
            self.last_heap = now;
            self.collect_heap();
        }
        if self.panel.as_deref() == Some("warp") {
            if !self.warp_busy
                && now.duration_since(self.warp_last_check) >= Duration::from_secs(4)
            {
                self.warp_check();
            }
            self.refresh();
        }
        if self.vault.is_some() && now.duration_since(self.vault_last_use) >= Duration::from_secs(600)
        {
            self.vault = None;
            self.status = "Сейф заблокирован после 10 минут бездействия.".to_string();
            if self.panel.as_deref() == Some("passwords") {
                self.refresh();
            }
        }
        if let Some((at, before)) = self.freed_pending {
            if now.duration_since(at) >= Duration::from_millis(2200) {
                self.sample_stats();
                let freed = (before - self.ram_mb).max(0.0);
                self.last_freed = freed;
                self.status = format!(
                    "Освобождено примерно {:.0} МБ. Сейчас браузер занимает {:.0} МБ.",
                    freed, self.ram_mb
                );
                self.freed_pending = None;
                self.refresh();
            }
        }
        if now.duration_since(self.last_saver) >= Duration::from_secs(15) {
            self.last_saver = now;
            self.discard_idle();
        }
        if now.duration_since(self.last_snapshot) >= Duration::from_secs(20) {
            self.last_snapshot = now;
            let sig = self.session_signature();
            if sig != self.last_session_sig {
                self.last_session_sig = sig;
                self.save_session();
            }
        }
    }

    fn apply_toggle(&mut self, name: &str, v: bool) {
        let s = &mut self.store.settings;
        match name {
            "snow" => s.snow = v,
            "glow" => s.glow = v,
            "anim" => s.anim = v,
            "restore" => s.restore_session = v,
            "memSaver" => s.memory_saver = v,
            "hw" => s.hw_accel = v,
            "bg" => s.bg_limit = v,
            "permNotif" => s.perm_notifications = v,
            "permGeo" => s.perm_geolocation = v,
            "permCam" => s.perm_camera = v,
            "permMic" => s.perm_microphone = v,
            "protection" => s.protection_on = v,
            "redirectGuard" => s.redirect_guard = v,
            "dlGuard" => s.download_guard = v,
            "tabMetrics" => s.tab_metrics = v,
            "lazyTabs" => s.lazy_tabs = v,
            "autoClean" => s.auto_clean = v,
            "cleanHistory" => s.clean_history = v,
            "cleanCache" => s.clean_cache = v,
            "cleanCookies" => s.clean_cookies = v,
            "closeWarn" => s.close_warn = v,
            "bkBookmarks" => s.backup_bookmarks = v,
            "bkSettings" => s.backup_settings = v,
            "bkHistory" => s.backup_history = v,
            "bkSession" => s.backup_session = v,
            "bkSpaces" => s.backup_spaces = v,
            "bkSets" => s.backup_sets = v,
            "bkReplace" => s.backup_replace = v,
            "toolbarHidden" => s.toolbar_hidden = v,
            "panelLeft" => s.panel_left = v,
            "addressRow" => s.address_row = v,
            "warpAuto" => s.warp_auto = v,
            "aiOn" => s.ai_on = v,
            "aiSave" => s.ai_save = v,
            "mouseGestures" => s.mouse_gestures = v,
            "bookmarksBar" => s.bookmarks_bar = v,
            _ => return,
        }
        if matches!(name, "hw" | "bg") {
            self.status = "Применится после перезапуска браузера.".to_string();
        }
        if name == "protection" || name == "mouseGestures" {
            self.inject = adblock::inject_script(
                self.store.settings.protection_on,
                self.store.settings.mouse_gestures,
            );
            self.apply_all_site_policies();
            if name == "protection" {
                self.status = if v {
                    "Защита включена. Реклама и трекеры блокируются.".to_string()
                } else {
                    "Защита выключена. Сайты будут открываться как есть.".to_string()
                };
            }
        }
        if name == "redirectGuard" {
            self.guard.set_strict(v);
            self.status = if v {
                "Предупреждения о подозрительных переходах включены.".to_string()
            } else {
                "Предупреждения о подозрительных переходах выключены.".to_string()
            };
        }
        if name == "dlGuard" {
            self.dl_guard.store(v, Ordering::Relaxed);
            self.status = if v {
                "Опасные загрузки будут придерживаться до подтверждения.".to_string()
            } else {
                "Проверка опасных загрузок выключена.".to_string()
            };
        }
        if name == "tabMetrics" {
            self.status = if v {
                "Оценка памяти по вкладкам включена.".to_string()
            } else {
                "Оценка памяти по вкладкам выключена.".to_string()
            };
            if v {
                self.last_heap = Instant::now() - Duration::from_secs(10);
            }
        }
        self.perms.update(&self.store.settings);
        self.perms.update_sites(&self.store.site_rules);
        if name == "aiSave" && !v {
            self.store.ai_history.clear();
        }
        storage::save(&self.store);
        if matches!(name, "toolbarHidden" | "addressRow" | "bookmarksBar") {
            self.layout();
        }
        if matches!(name, "snow" | "glow" | "anim") {
            self.reload_newtab();
        }
        self.refresh();
    }

    fn set_setting(&mut self, key: &str, v: &Value) {
        match key {
            "searchEngine" => {
                let e = v.as_str().unwrap_or("ddg");
                self.store.settings.search_engine = match e {
                    "google" => "google",
                    "own" => "own",
                    _ => "ddg",
                }
                .to_string();
                self.status = "Поисковик сохранён.".to_string();
            }
            "homepage" => {
                self.store.settings.homepage = storage::normalize_home(v.as_str().unwrap_or(""));
                self.status = "Стартовая страница сохранена.".to_string();
            }
            "theme" => {
                self.store.settings.theme = match v.as_str() {
                    Some("light") => "light".to_string(),
                    Some("amoled") => "amoled".to_string(),
                    _ => "dark".to_string(),
                };
                self.reload_newtab();
            }
            "idle" => {
                let n = v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(30);
                self.store.settings.idle_minutes = (n as u32).min(240);
            }
            "ramLimit" => {
                let n = v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(2048);
                self.store.settings.ram_limit_mb = if n == 0 {
                    0
                } else {
                    (n as u32).clamp(256, 16384)
                };
            }
            "autoCleanDays" => {
                let n = v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(7);
                self.store.settings.auto_clean_days = (n as u32).clamp(1, 90);
            }
            "accent" | "accent2" => {
                let t = v.as_str().unwrap_or("").trim();
                if t.len() == 7 && t.starts_with('#') && t[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                    if key == "accent" {
                        self.store.settings.accent = t.to_ascii_lowercase();
                    } else {
                        self.store.settings.accent2 = t.to_ascii_lowercase();
                    }
                }
                self.reload_newtab();
            }
            "tabShape" => {
                let shape = v.as_str().unwrap_or("rounded");
                self.store.settings.tab_shape = match shape {
                    "pill" | "square" => shape,
                    _ => "rounded",
                }
                .to_string();
            }
            "uiRadius" => {
                let n = v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(10);
                self.store.settings.ui_radius = (n as u32).min(18);
            }
            "uiFont" => {
                let f = v.as_str().unwrap_or("segoe");
                self.store.settings.ui_font = match f {
                    "system" | "arial" | "georgia" | "consolas" => f,
                    _ => "segoe",
                }
                .to_string();
            }
            "uiStyle" => {
                self.store.settings.ui_style = if v.as_str() == Some("orvyn") {
                    "orvyn".to_string()
                } else {
                    "chrome".to_string()
                };
                self.layout();
            }
            "bookmarksBar" => {
                self.store.settings.bookmarks_bar = v.as_bool().unwrap_or(false);
                self.layout();
            }
            "uiOpacity" => {
                let n = v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(100);
                self.store.settings.ui_opacity = (n as u32).clamp(60, 100);
            }
            "uiBlur" => {
                let n = v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0);
                self.store.settings.ui_blur = (n as u32).min(24);
            }
            "wallpaper" => {
                let w = v.as_str().unwrap_or("none");
                self.store.settings.wallpaper = match w {
                    "aurora" | "grid" | "dots" | "paper" => w,
                    _ => "none",
                }
                .to_string();
                self.reload_newtab();
            }
            _ => return,
        }
        storage::save(&self.store);
        self.refresh();
    }

    fn clear_data(&mut self, what: &str) {
        match what {
            "history" => {
                self.store.history.clear();
                crate::db::clear_history();
                storage::save(&self.store);
                self.status = "История очищена.".to_string();
            }
            "cache" => {
                self.status = if self.clear_cache() {
                    "Кэш очищен.".to_string()
                } else {
                    "Не удалось очистить кэш.".to_string()
                };
            }
            "cookies" => {
                self.status = if self.clear_cookies() {
                    "Cookies удалены.".to_string()
                } else {
                    "Не удалось удалить cookies.".to_string()
                };
            }
            "all" => {
                self.store.history.clear();
                crate::db::clear_history();
                storage::save(&self.store);
                let a = self.clear_cache();
                let b = self.clear_cookies();
                self.status = if a && b {
                    "История, кэш и cookies очищены.".to_string()
                } else {
                    "История очищена, но не всё удалось удалить.".to_string()
                };
            }
            _ => {}
        }
        self.refresh();
    }

    fn clear_cookies(&self) -> bool {
        let core = self.chrome.webview();
        let Ok(w2) = core.cast::<ICoreWebView2_2>() else { return false };
        let Ok(cm) = (unsafe { w2.CookieManager() }) else { return false };
        unsafe { cm.DeleteAllCookies() }.is_ok()
    }

    fn clear_cache(&self) -> bool {
        let core = self.chrome.webview();
        let Ok(w13) = core.cast::<ICoreWebView2_13>() else { return false };
        let Ok(profile) = (unsafe { w13.Profile() }) else { return false };
        let Ok(p2) = profile.cast::<ICoreWebView2Profile2>() else { return false };
        let handler = ClearBrowsingDataCompletedHandler::create(Box::new(|_hr| Ok(())));
        let kinds = COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE
            | COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE;
        unsafe { p2.ClearBrowsingData(kinds, &handler) }.is_ok()
    }

    /// Запускает подсчёт размера профиля и места сайтов.
    fn scan_profile_size(&mut self) {
        if self.clean_scanning {
            return;
        }
        self.clean_scanning = true;
        let p = self.proxy.clone();
        std::thread::spawn(move || {
            let n = cleaner::profiles_size();
            let _ = p.send_event(UserEvent::CleanSize(n));
        });
        self.refresh();
    }

    /// Считает место, занятое данными сайтов, через DevTools протокол.
    fn scan_usage(&mut self) {
        if self.usage_scanning {
            return;
        }
        let mut seen = HashSet::new();
        let mut list: Vec<(String, String)> = Vec::new();
        {
            let mut add = |url: &str| {
                if list.len() >= 20 {
                    return;
                }
                let (Some(origin), Some(host)) =
                    (security::origin_of(url), security::host_of(url))
                else {
                    return;
                };
                if seen.insert(origin.clone()) {
                    list.push((host, origin));
                }
            };
            if let Some(t) = self.current_tab() {
                add(&t.url);
            }
            for h in self.store.history.iter().rev() {
                add(&h.url);
            }
        }
        if list.is_empty() {
            self.status = "Нет сайтов, по которым можно посчитать место.".to_string();
            self.refresh();
            return;
        }
        self.usage.clear();
        self.usage_scanning = true;
        self.usage_pending = 0;
        let mut started = 0usize;
        for (host, origin) in list {
            if self.call_usage(&host, &origin) {
                started += 1;
            }
        }
        self.usage_pending = started;
        if started == 0 {
            self.usage_scanning = false;
            self.status = "Не удалось запустить подсчёт места сайтов.".to_string();
        } else {
            self.status = "Считаю место сайтов…".to_string();
        }
        self.refresh();
    }

    fn call_usage(&self, host: &str, origin: &str) -> bool {
        let core = self.chrome.webview();
        let method: Vec<u16> = "Storage.getUsageAndQuota\0".encode_utf16().collect();
        let params = json!({ "origin": origin }).to_string();
        let wide: Vec<u16> = params.encode_utf16().chain(std::iter::once(0)).collect();
        let p = self.proxy.clone();
        let host = host.to_string();
        let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
            move |_hr, json| {
                let n = serde_json::from_str::<Value>(&json)
                    .ok()
                    .and_then(|v| v.get("usage").and_then(|u| u.as_u64()))
                    .unwrap_or(0);
                let _ = p.send_event(UserEvent::Usage(host.clone(), n));
                Ok(())
            },
        ));
        unsafe {
            core.CallDevToolsProtocolMethod(PCWSTR(method.as_ptr()), PCWSTR(wide.as_ptr()), &handler)
        }
        .is_ok()
    }

    /// Чистит выбранные пользователем данные: история, кэш, cookies.
    fn clean_selected(&mut self) {
        let (h, c, k) = (
            self.store.settings.clean_history,
            self.store.settings.clean_cache,
            self.store.settings.clean_cookies,
        );
        if !(h || c || k) {
            self.status = "Выбери, что чистить: историю, кэш или cookies.".to_string();
            self.refresh();
            return;
        }
        if h {
            self.store.history.clear();
            crate::db::clear_history();
        }
        let cache_ok = if c { self.clear_cache() } else { true };
        let cookies_ok = if k { self.clear_cookies() } else { true };
        self.store.last_clean = storage::now();
        storage::save(&self.store);
        let mut parts = Vec::new();
        if h {
            parts.push("история");
        }
        if c {
            parts.push("кэш");
        }
        if k {
            parts.push("cookies");
        }
        self.status = if cache_ok && cookies_ok {
            format!("Очищено: {}.", parts.join(", "))
        } else {
            format!("Частично очищено: {}. Повтори ещё раз.", parts.join(", "))
        };
        self.refresh();
    }

    /// Автоочистка при запуске, если расписание сработало.
    fn auto_clean_if_due(&mut self) {
        let s = &self.store.settings;
        if !s.auto_clean
            || !cleaner::clean_due(self.store.last_clean, s.auto_clean_days, storage::now())
        {
            return;
        }
        let (h, c, k) = (s.clean_history, s.clean_cache, s.clean_cookies);
        if !(h || c || k) {
            return;
        }
        if h {
            self.store.history.clear();
            crate::db::clear_history();
        }
        if c {
            self.clear_cache();
        }
        if k {
            self.clear_cookies();
        }
        self.store.last_clean = storage::now();
        storage::save(&self.store);
        self.status = "Автоочистка выполнена при запуске.".to_string();
    }

    fn reset_settings(&mut self) {
        self.store.settings = storage::Settings::default();
        self.store.site_rules.clear();
        self.inject = adblock::inject_script(
            self.store.settings.protection_on,
            self.store.settings.mouse_gestures,
        );
        self.guard.set_strict(self.store.settings.redirect_guard);
        self.dl_guard
            .store(self.store.settings.download_guard, Ordering::Relaxed);
        self.perms.update(&self.store.settings);
        self.perms.update_sites(&self.store.site_rules);
        self.apply_all_site_policies();
        storage::save(&self.store);
        self.status = "Настройки сброшены.".to_string();
        self.reload_newtab();
        self.refresh();
    }

    /// Сбрасывает один раздел настроек и заново применяет то, что от него зависит.
    fn reset_settings_section(&mut self, section: &str) {
        if !storage::reset_section(&mut self.store.settings, section) {
            self.status = "Неизвестный раздел настроек.".to_string();
            self.refresh();
            return;
        }
        self.inject = adblock::inject_script(
            self.store.settings.protection_on,
            self.store.settings.mouse_gestures,
        );
        self.guard.set_strict(self.store.settings.redirect_guard);
        self.dl_guard
            .store(self.store.settings.download_guard, Ordering::Relaxed);
        self.perms.update(&self.store.settings);
        self.perms.update_sites(&self.store.site_rules);
        self.apply_all_site_policies();
        storage::save(&self.store);
        if section == "look" || section == "search" {
            self.reload_newtab();
        }
        let name = storage::SETTINGS_SECTIONS
            .iter()
            .find(|(id, _)| *id == section)
            .map(|(_, name)| *name)
            .unwrap_or(section);
        self.status = format!("Раздел «{name}» сброшен. Остальные настройки не тронуты.");
        self.refresh();
    }

    fn save_proxy_form(&mut self, v: &Value) {
        let s = &mut self.store.settings;
        if let Some(k) = v["kind"].as_str() {
            s.proxy_kind = if k.eq_ignore_ascii_case("http") {
                "http".to_string()
            } else {
                "socks5".to_string()
            };
        }
        if let Some(h) = v["host"].as_str() {
            let raw = h.trim();
            let raw = raw.split("://").last().unwrap_or(raw);
            let mut host = raw.to_string();
            let mut found_port: Option<String> = None;
            if let Some((hh, pp)) = raw.rsplit_once(':') {
                if !hh.contains(':') && !pp.is_empty() && pp.chars().all(|c| c.is_ascii_digit()) {
                    host = hh.to_string();
                    found_port = Some(pp.chars().take(5).collect());
                }
            }
            s.proxy_host = host
                .chars()
                .filter(|c| !c.is_whitespace())
                .take(200)
                .collect();
            if s.proxy_port.trim().is_empty() {
                if let Some(p) = found_port {
                    s.proxy_port = p;
                }
            }
        }
        if let Some(p) = v["port"].as_str() {
            let p: String = p.chars().filter(|c| c.is_ascii_digit()).take(5).collect();
            if !p.is_empty() {
                s.proxy_port = p;
            }
        }
        if let Some(on) = v["on"].as_bool() {
            s.proxy_on = on;
        }
        self.music_context = None;
        storage::save(&self.store);
    }

}

fn app_icon() -> Option<Icon> {
    let bytes = include_bytes!("../assets/orvyn-256.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(&bytes[..]));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba {
        return None;
    }
    let rgba = buf[..info.buffer_size()].to_vec();
    Icon::from_rgba(rgba, info.width, info.height).ok()
}

pub fn run() {
    let started = Instant::now();
    let crashed = storage::begin_run();
    if let Some(m) = &crashed {
        crate::logging::warn(&format!(
            "найден маркер незавершённого запуска: pid {}, версия {}",
            m.pid, m.version
        ));
    }
    let store = storage::load();
    let mut args = browser_args(&store.settings);
    // Отладочный доступ к DevTools протоколу (только если явно попросили).
    if let Ok(port) = std::env::var("ORVYN_CDP_PORT").or_else(|_| std::env::var("ORBIT_CDP_PORT")) {
        if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
            args.push_str(&format!(" --remote-debugging-port={port}"));
        }
    }
    std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", &args);

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("ORVYN 2.2")
        .with_decorations(false)
        .with_window_icon(app_icon())
        .with_inner_size(LogicalSize::new(1360.0, 860.0))
        .with_min_inner_size(LogicalSize::new(720.0, 480.0))
        .build(&event_loop)
        .expect("не удалось создать окно");

    let mut app = App::new(window, proxy, args, store).expect("не удалось инициализировать браузер");

    let urls: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.is_empty() && !a.starts_with('-'))
        .collect();
    if !urls.is_empty() {
        for u in urls {
            app.new_tab(Some(u), false);
        }
        let last = app.tabs.len().saturating_sub(1);
        app.select(last);
    } else if !app.restore_session() {
        app.new_tab(None, true);
    }
    let recovered = app.store.session.len();
    if crashed.is_some() {
        app.report_crash(recovered);
    }

    app.auto_clean_if_due();
    app.auto_warp_if_due();
    app.startup_ms = started.elapsed().as_millis() as u64;

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(1000));
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                if app.should_warn_close() {
                    app.show_close_warning();
                } else {
                    app.save_session();
                    storage::end_run();
                    crate::logging::info("штатный выход по закрытию окна");
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent { event: WindowEvent::Resized(_), .. } => {
                app.layout();
                app.sync_maximized();
            }
            Event::WindowEvent { event: WindowEvent::ScaleFactorChanged { .. }, .. } => app.layout(),
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => app.tick(),
            Event::UserEvent(UserEvent::Quit) => {
                app.save_session();
                storage::end_run();
                crate::logging::info("штатный выход по команде");
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(ev) => app.on_user_event(ev),
            Event::LoopDestroyed => storage::end_run(),
            _ => {}
        }
    });
}
