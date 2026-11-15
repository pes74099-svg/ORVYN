#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![recursion_limit = "512"]

mod adblock;
mod ai;
mod app;
mod backup;
mod capture;
mod cleaner;
mod db;
mod downloads;
mod import;
mod instance;
mod logging;
mod monitor;
mod music;
mod search;
mod security;
mod storage;
mod sync;
mod update;
mod vault;
mod webview_loader;

fn main() {
    let primary = instance::acquire();
    logging::init(primary);
    if !primary {
        if !instance::focus_existing() {
            instance::notify_running();
        }
        logging::warn("второй экземпляр: показан уже запущенный ORVYN, выход");
        return;
    }
    logging::info(&format!("WebView2Loader: {}", webview_loader::report()));
    if !webview_loader::runtime_present() {
        logging::warn("WebView2 Runtime не найден: ORVYN не сможет открыть окно без него");
    }
    app::run();
}
