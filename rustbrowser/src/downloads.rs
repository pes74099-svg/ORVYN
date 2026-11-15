//! Менеджер загрузок поверх WebView2: прогресс, скорость, пауза/продолжение.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::security;

use wry::WebViewExtWindows;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2DownloadOperation, ICoreWebView2_4,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_PAUSED,
    COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED,
};
use webview2_com::{
    BytesReceivedChangedEventHandler, DownloadStartingEventHandler, StateChangedEventHandler,
};
use windows::core::{Interface, PWSTR};

static NEXT: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static OPS: RefCell<HashMap<u64, ICoreWebView2DownloadOperation>> = RefCell::new(HashMap::new());
    /// URL, которые пользователь разрешил скачать один раз после предупреждения.
    static ALLOW_ONCE: RefCell<std::collections::HashSet<String>> = RefCell::new(std::collections::HashSet::new());
}

/// Разрешает следующую загрузку этого URL, несмотря на проверку.
pub fn allow_once(url: &str) {
    ALLOW_ONCE.with(|m| {
        m.borrow_mut().insert(url.to_string());
    });
}

fn take_allow(url: &str) -> bool {
    ALLOW_ONCE.with(|m| m.borrow_mut().remove(url))
}

#[derive(Debug, Clone)]
pub enum Msg {
    Started { id: u64, url: String, path: String },
    /// Загрузка придержана до подтверждения пользователя.
    Danger { id: u64, url: String, path: String, reason: String },
    Progress { id: u64, bytes: u64, total: i64, speed: u64, state: String },
}

pub type Sink = Arc<dyn Fn(Msg) + Send + Sync + 'static>;

fn text(p: PWSTR) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { p.to_string().unwrap_or_default() }
    }
}

/// Подписывается на все загрузки этой вкладки.
/// `guard` — включена ли проверка опасных файлов.
pub fn attach(wv: &wry::WebView, sink: Sink, guard: Arc<AtomicBool>) {
    let Ok(core) = wv.webview().cast::<ICoreWebView2_4>() else {
        return;
    };

    let s = sink.clone();
    let started = DownloadStartingEventHandler::create(Box::new(move |_wv, args| {
        let Some(args) = args else { return Ok(()) };
        unsafe {
            // Своя панель загрузок вместо диалога Edge.
            let _ = args.SetHandled(true);
            let op = args.DownloadOperation()?;
            let mut uri = PWSTR::null();
            let _ = op.Uri(&mut uri);
            let url = text(uri);
            let mut fp = PWSTR::null();
            let _ = op.ResultFilePath(&mut fp);
            let path = text(fp);

            let id = NEXT.fetch_add(1, Ordering::SeqCst);

            // Опасные типы файлов отменяем сразу и показываем предупреждение.
            // Если пользователь нажмёт «Продолжить», URL получит одноразовое разрешение
            // и загрузка запустится заново уже без блокировки.
            if guard.load(Ordering::Relaxed) {
                if let Some(reason) = security::dangerous_download(&path, &url) {
                    if take_allow(&url) {
                        // Разрешено вручную — продолжаем обычную загрузку.
                    } else {
                        let _ = op.Cancel();
                        let _ = s(Msg::Danger {
                            id,
                            url: url.clone(),
                            path: path.clone(),
                            reason,
                        });
                        return Ok(());
                    }
                }
            }

            OPS.with(|m| {
                m.borrow_mut().insert(id, op.clone());
            });
            let _ = s(Msg::Started {
                id,
                url: url.clone(),
                path: path.clone(),
            });

            let s1 = sink.clone();
            let op1 = op.clone();
            let last_bytes = std::cell::Cell::new(0u64);
            let last_at = std::cell::Cell::new(Instant::now());
            let bytes = BytesReceivedChangedEventHandler::create(Box::new(move |_o, _a| {
                let mut b = 0i64;
                let mut t = -1i64;
                let _ = op1.BytesReceived(&mut b);
                let _ = op1.TotalBytesToReceive(&mut t);
                let b = b.max(0) as u64;
                let now = Instant::now();
                let dt = now.duration_since(last_at.get()).as_secs_f64();
                let speed = if dt > 0.05 {
                    let d = b.saturating_sub(last_bytes.get()) as f64 / dt;
                    last_bytes.set(b);
                    last_at.set(now);
                    d.max(0.0) as u64
                } else {
                    0
                };
                let _ = s1(Msg::Progress {
                    id,
                    bytes: b,
                    total: t,
                    speed,
                    state: "progress".to_string(),
                });
                Ok(())
            }));
            let mut token = 0i64;
            let _ = op.add_BytesReceivedChanged(&bytes, &mut token);

            let s2 = sink.clone();
            let op2 = op.clone();
            let st = StateChangedEventHandler::create(Box::new(move |_o, _a| {
                let mut state =
                    webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_DOWNLOAD_STATE(0);
                let _ = op2.State(&mut state);
                let code = state.0;
                let done = code == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED.0;
                let mut reason =
                    webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON(0);
                let _ = op2.InterruptReason(&mut reason);
                let paused = code == 1 && reason == COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_PAUSED;
                let canceled =
                    code == 1 && reason == COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED;
                let name = if done {
                    "completed"
                } else if paused {
                    "paused"
                } else if canceled {
                    "canceled"
                } else if code == 1 {
                    "interrupted"
                } else {
                    "progress"
                };
                let mut b = 0i64;
                let mut t = -1i64;
                let _ = op2.BytesReceived(&mut b);
                let _ = op2.TotalBytesToReceive(&mut t);
                let _ = s2(Msg::Progress {
                    id,
                    bytes: b.max(0) as u64,
                    total: t,
                    speed: 0,
                    state: name.to_string(),
                });
                Ok(())
            }));
            let mut token2 = 0i64;
            let _ = op.add_StateChanged(&st, &mut token2);
        }
        Ok(())
    }));

    let mut token = 0i64;
    unsafe {
        let _ = core.add_DownloadStarting(&started, &mut token);
    }
}

pub fn control(id: u64, action: &str) -> bool {
    OPS.with(|m| {
        let map = m.borrow();
        let Some(op) = map.get(&id) else { return false };
        unsafe {
            match action {
                "pause" => op.Pause().is_ok(),
                "resume" => op.Resume().is_ok(),
                "cancel" => op.Cancel().is_ok(),
                _ => false,
            }
        }
    })
}

/// Убирает завершённую операцию из карты, чтобы не держать COM-объект.
pub fn forget(id: u64) {
    OPS.with(|m| {
        m.borrow_mut().remove(&id);
    });
}
