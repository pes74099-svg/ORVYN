//! Защита от второго экземпляра ORVYN.
//!
//! Два процесса на одном профиле WebView2 и одном `store.json` — это гонка
//! и риск потерять данные. Именованный mutex живёт до конца процесса и
//! освобождается сам, даже если браузер упал.

use std::sync::atomic::{AtomicUsize, Ordering};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, IsIconic, MessageBoxW, SetForegroundWindow, ShowWindow, MB_ICONINFORMATION, MB_OK,
    SW_RESTORE,
};

const MUTEX_NAME: &str = "Local\\ORVYN.SingleInstance.v1";
pub const WINDOW_TITLE: &str = concat!("ORVYN ", env!("CARGO_PKG_VERSION"));

static MUTEX: AtomicUsize = AtomicUsize::new(0);

/// `true` — это первый экземпляр и можно запускаться.
/// При ошибке Win32 разрешаем запуск: лучше два окна, чем браузер, который не стартует.
pub fn acquire() -> bool {
    let name = wide(MUTEX_NAME);
    match unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) } {
        Ok(handle) => {
            MUTEX.store(handle.0 as usize, Ordering::Relaxed);
            let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
            !already
        }
        Err(_) => true,
    }
}

/// Пытается поднять и активировать уже открытое окно ORVYN.
pub fn focus_existing() -> bool {
    let title = wide(WINDOW_TITLE);
    let Ok(hwnd) = (unsafe { FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) }) else {
        return false;
    };
    if hwnd.is_invalid() {
        return false;
    }
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
    true
}

/// Если окно найти не удалось, честно сообщаем, что ORVYN уже запущен.
pub fn notify_running() {
    let text = wide("ORVYN уже запущен. Переключись на открытое окно.");
    let caption = wide("ORVYN");
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
