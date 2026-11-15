//! Установщик ORVYN.
//!
//! Кладёт браузер в %LOCALAPPDATA%\Programs\ORVYN, создаёт ярлыки и запись
//! в «Установка и удаление программ». Права администратора не нужны.
//!
//! Запуск:  OrvynSetup.exe            — установить и открыть
//!          OrvynSetup.exe --uninstall — удалить

#![windows_subsystem = "windows"]

use std::env;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Не показывать консольные окна у дочерних процессов установщика.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

trait Silent {
    fn silent(&mut self) -> &mut Command;
}

impl Silent for Command {
    fn silent(&mut self) -> &mut Command {
        self.creation_flags(CREATE_NO_WINDOW)
    }
}

static PAYLOAD: &[u8] = include_bytes!("../browser.exe");
static LOADER_PAYLOAD: &[u8] = include_bytes!("../WebView2Loader.dll");
/// Имя файла загрузчика WebView2. Windows ищет его рядом с exe.
const LOADER_NAME: &str = "WebView2Loader.dll";

const APP_NAME: &str = "ORVYN";
const VERSION: &str = "3.0.3";
const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\ORVYN";

/// Прошлые имена: чистим при установке, чтобы не осталось двух копий.
const OLD_NAMES: [&str; 3] = ["ORBIT", "Endervasya", "RustBrowser"];

const AGREEMENT: &str = r#"ПОЛЬЗОВАТЕЛЬСКОЕ СОГЛАШЕНИЕ ORVYN
Версия 2.2.0, редакция от 15 ноября 2026 года

Пожалуйста, прочитай это соглашение перед установкой и использованием ORVYN. Устанавливая или используя браузер, ты подтверждаешь, что прочитал, понял и принимаешь его условия. Если не согласен — не устанавливай и не используй ORVYN.

1. ОБЩИЕ ПОЛОЖЕНИЯ
1.1. ORVYN — бесплатный браузер для Windows, который работает на движке Microsoft WebView2. ORVYN не является продуктом Google, Microsoft, Cloudflare или иной компании, упомянутой в соглашении.
1.2. Соглашение заключается между тобой (Пользователь) и командой разработчиков ORVYN (Разработчики). Оно не создаёт отношений агентства, партнёрства, трудовых или представительских отношений.
1.3. ORVYN — нейтральный технический инструмент. Разработчики не управляют действиями Пользователя, не проверяют и не одобряют сайты, файлы и контент, которые он открывает, и не являются стороной отношений между Пользователем и третьими лицами.
1.4. Используя ORVYN, ты подтверждаешь, что обладаешь необходимой дееспособностью. Если ты несовершеннолетний, используй браузер с согласия родителя или законного представителя.

2. ЛИЦЕНЗИЯ
2.1. Разработчики предоставляют тебе бесплатную, неисключительную, непередаваемую и отзывную лицензию на использование ORVYN в личных и рабочих целях на принадлежащих тебе устройствах.
2.2. Ты не вправе продавать, сдавать в аренду, перепродавать ORVYN под другим именем или выдавать изменённую сборку за официальную.
2.3. Ты не вправе обходить технические ограничения ORVYN, если это прямо не разрешено законом.
2.4. Лицензия действует, пока ты соблюдаешь соглашение. При нарушении Разработчики могут прекратить поддержку и распространение твоей копии, но это не лишает тебя прав, которые нельзя ограничить по закону.

3. ИСПОЛЬЗОВАНИЕ ORVYN
3.1. ORVYN можно использовать для доступа к сайтам, работы с почтой и документами, учёбы, просмотра видео, общения и других законных задач.
3.2. Ты сам отвечаешь за свои действия, вводимые данные, скачанные файлы и последствия использования браузера.
3.3. Запрещено использовать ORVYN для нарушения закона, прав других людей, распространения вредоносных файлов, фишинга, мошенничества, спама, взлома, обхода чужих систем защиты и иных противоправных действий.
3.4. Запрещено использовать ORVYN для атак, массовых автоматизированных действий, которые нарушают работу сайтов, или для обхода платного доступа и подписок.
3.5. Ты не вправе выдавать себя за Разработчиков, распространять изменённые версии под именем ORVYN или вводить пользователей в заблуждение относительно происхождения браузера.

4. ВНЕШНИЕ СЕРВИСЫ И САЙТЫ
4.1. ORVYN открывает сторонние сайты. Они работают по своим правилам, могут собирать данные, показывать рекламу и требовать согласия. Разработчики не контролируют их содержание и не отвечают за него.
4.2. ORVYN AI использует публичный бесплатный API Pollinations. Запросы уходят в интернет только когда ты сам открываешь AI и отправляешь сообщение. У сервиса есть лимиты, перерывы и ограничения; ответы могут быть неточными. Действуют правила Pollinations.
4.3. Перевод страниц открывает Google Translate в новой вкладке. Действуют правила Google.
4.4. WARP — официальный клиент Cloudflare. ORVYN только показывает состояние и управляет подключением через warp-cli; собственных VPN-серверов у ORVYN нет. Действуют правила Cloudflare.
4.5. Браузер работает на WebView2 и Windows. Действуют условия Microsoft.
4.6. ORVYN не является стороной отношений между тобой и внешними сервисами. Претензии к ним предъявляются по их правилам.

5. ДАННЫЕ И ПРИВАТНОСТЬ
5.1. Настройки, история, закладки, сессии, userscripts и профиль WebView2 хранятся локально в %APPDATA%\Orvyn.
5.2. Пароли хранятся в локальном зашифрованном сейфе (Argon2id + AES-256-GCM). Мастер-пароль не передаётся Разработчикам и не восстанавливается. Если забыть мастер-пароль, открыть сейф невозможно.
5.3. ORVYN не создаёт аккаунтов, не собирает телеметрию и не отправляет историю на серверы Разработчиков.
5.4. Ты сам делаешь резервные копии и отвечаешь за сохранность профиля. Удаление vault.json или папки профиля удаляет данные без возможности восстановления.
5.5. Не отправляй в AI, перевод и другие внешние сервисы пароли, ключи, документы и личные данные, если не понимаешь, куда они уходят.

6. РЕКЛАМА, ЗАЩИТА И СКРИПТЫ
6.1. ORVYN блокирует рекламу и трекеры, но не гарантирует стопроцентную блокировку. Сайты могут менять разметку, тогда защита может работать хуже.
6.2. Встроенная защита не заменяет антивирус и здравый смысл. Не вводи данные на подозрительных сайтах.
6.3. WebView2 не поддерживает Chrome-расширения. Вместо них ORVYN запускает userscripts из папки профиля.
6.4. Userscripts могут менять страницы и читать их содержимое. Устанавливай только те, которым доверяешь. Разработчики не отвечают за сторонние скрипты.

7. ОБНОВЛЕНИЯ
7.1. Новые версии ORVYN устанавливаются вручную через установщик. Разработчики не обязаны выпускать обновления, поддерживать старые версии или обеспечивать работу сторонних сайтов и сервисов.
7.2. Перед установкой новой версии закрой браузер. Установщик может закрыть запущенный ORVYN, удалить старые версии ORVYN/ORBIT и перенести профиль.

8. ОТКАЗ ОТ ГАРАНТИЙ
8.1. ORVYN предоставляется «как есть» и «как доступно». В пределах, разрешённых законом, Разработчики не дают гарантий бесперебойной работы, отсутствия ошибок, совместимости со всеми сайтами, сохранности данных и пригодности для конкретных целей.
8.2. Ты используешь ORVYN добровольно и на свой риск.

9. ОГРАНИЧЕНИЕ ОТВЕТСТВЕННОСТИ
9.1. В пределах, разрешённых законом, Разработчики не несут ответственности за косвенные, случайные, особые и штрафные убытки, потерю данных, прибыли или деловой репутации, возникшие из-за использования или невозможности использования ORVYN.
9.2. Разработчики не отвечают за действия третьих лиц, содержание сайтов, работу внешних сервисов, скачанные файлы и противоправное использование браузера Пользователем.
9.3. Если Пользователь нарушил закон или права третьих лиц с помощью ORVYN, он урегулирует претензии и возместит ущерб самостоятельно. Если из-за такого нарушения претензии предъявят Разработчикам, Пользователь возмещает им документально подтверждённые убытки и расходы.
9.4. Ничто в соглашении не ограничивает ответственность, которую нельзя ограничить по закону, и не лишает Пользователя обязательных прав, предусмотренных законодательством о защите прав потребителей.

10. ПРЕКРАЩЕНИЕ
10.1. Ты можешь в любой момент прекратить использование ORVYN и удалить его через «Параметры → Приложения» или ярлык удаления.
10.2. Разработчики могут прекратить распространение и поддержку ORVYN. Это не влияет на уже возникшие обязательства и права, которые нельзя ограничить.
10.3. Пункты об отказе от гарантий, ограничении ответственности, возмещении убытков и границах участия сохраняют силу после прекращения использования.

11. ИЗМЕНЕНИЯ СОГЛАШЕНИЯ
11.1. Соглашение может обновляться. Новая редакция показывается при следующей установке.
11.2. Продолжая использовать ORVYN после обновления, ты принимаешь новую редакцию. Если не принимаешь — не устанавливай обновление и прекрати использование.
11.3. Если какой-то пункт признают недействительным, остальные пункты продолжают действовать. Бездействие Разработчиков не является отказом от права требовать соблюдения соглашения.

12. ПРИМЕНИМОЕ ПРАВО И СПОРЫ
12.1. К соглашению применяются обязательные нормы законодательства, которые нельзя изменить договором.
12.2. Споры между Пользователем и Разработчиками решаются путём переговоров, а если договориться не удалось — в компетентном суде по правилам применимого законодательства.
12.3. Споры между Пользователем и третьими лицами решаются без участия Разработчиков.

13. КОНТАКТЫ
13.1. Проект: ORVYN. Автор идеи: Ваня (Собака). Разработчики указаны в разделе «О браузере».
13.2. По вопросам соглашения используй контакты, указанные в разделе «О браузере» или в описании релиза.

Нажимая «Установить» с отмеченным пунктом, ты подтверждаешь, что прочитал и принимаешь это соглашение и политику использования.
"#;

const POLICY: &str = r#"ПОЛИТИКА ИСПОЛЬЗОВАНИЯ ORVYN
Версия 2.2.0, редакция от 15 ноября 2026 года

1. НАЗНАЧЕНИЕ
1.1. ORVYN — инструмент для доступа к сайтам и работы в интернете. Эта политика описывает, что можно и нельзя делать при использовании браузера.
1.2. Используя ORVYN, ты соглашаешься соблюдать политику и законы своей страны.
1.3. ORVYN — нейтральный технический инструмент. Разработчики не контролируют, какие сайты открывает пользователь и что он там делает, и не являются стороной отношений между пользователем и третьими лицами.

2. РАЗРЕШЁННОЕ ИСПОЛЬЗОВАНИЕ
2.1. Личные и рабочие задачи, учёба, просмотр сайтов, работа с почтой и документами.
2.2. Настройка блокировки рекламы, паролей, оформления и других функций под себя.
2.3. Установка собственных userscripts, если они не нарушают закон и права других людей.

3. ЗАПРЕЩЕНО
3.1. Нарушать закон и права других людей, распространять вредоносные файлы и фишинг.
3.2. Использовать ORVYN для атак, взлома, обхода чужих систем защиты, массового спама и мошенничества.
3.3. Выдавать изменённую сборку ORVYN за официальную или продавать её под другим именем.
3.4. Обходить ограничения внешних сервисов (AI, перевод, WARP) и использовать их для запрещённых действий.

4. ДАННЫЕ И СЕКРЕТЫ
4.1. Не отправляй в ORVYN AI и сторонние сервисы пароли, ключи, документы и личные данные, если не понимаешь, куда они уходят.
4.2. Мастер-пароль сейфа и резервные копии храни сам. Разработчики не могут их восстановить.

5. AI, ПЕРЕВОД И WARP
5.1. AI и перевод — внешние бесплатные сервисы. Их ответы могут быть неточными, а работа — непостоянной.
5.2. WARP — официальный клиент Cloudflare. Используй его по правилам Cloudflare.
5.3. Блокировка рекламы не даёт права обходить платный доступ, подписки и защиту сайтов.

6. ОТВЕТСТВЕННОСТЬ
6.1. За свои действия и скачанные файлы отвечаешь ты.
6.2. Разработчики не проверяют и не одобряют сайты, файлы и действия пользователей, не модерируют их и не участвуют в спорах между пользователем и третьими лицами.
6.3. Если пользователь нарушил закон или права третьих лиц, он урегулирует претензии и возместит ущерб самостоятельно. Разработчики не являются стороной таких отношений.
6.4. Разработчики могут прекратить поддержку версии, но не обязаны обеспечивать работу сторонних сайтов и сервисов.
6.5. Нарушение политики — основание не помогать с такой установкой и не гарантировать её работу.
6.6. Ничто в этой политике не ограничивает ответственность, которую нельзя ограничить по закону.

7. ИЗМЕНЕНИЯ
7.1. Политика может обновляться. Новая редакция показывается при следующей установке.
7.2. Продолжая пользоваться ORVYN после обновления, ты принимаешь новую редакцию.

Нажимая «Установить» с отмеченным пунктом, ты подтверждаешь, что прочитал и принимаешь пользовательское соглашение и эту политику.
"#;

static AGREED: AtomicBool = AtomicBool::new(false);

const ID_CHECK: i32 = 101;
const ID_INSTALL: i32 = 102;
const ID_CANCEL: i32 = 103;

type Hwnd = *mut std::ffi::c_void;
type Hinstance = *mut std::ffi::c_void;
type Hicon = *mut std::ffi::c_void;
type Hcursor = *mut std::ffi::c_void;
type Hbrush = *mut std::ffi::c_void;
type Hmenu = *mut std::ffi::c_void;
type Hfont = *mut std::ffi::c_void;
type Hdc = *mut std::ffi::c_void;

#[repr(C)]
struct WndClassW {
    style: u32,
    lpfn_wnd_proc: Option<unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize>,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: Hinstance,
    h_icon: Hicon,
    h_cursor: Hcursor,
    hbr_background: Hbrush,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
}

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: Point,
}

#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(hwnd: Hwnd, text: *const u16, caption: *const u16, utype: u32) -> i32;
    fn RegisterClassW(class: *const WndClassW) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Hwnd,
        menu: Hmenu,
        instance: Hinstance,
        param: *mut std::ffi::c_void,
    ) -> Hwnd;
    fn DefWindowProcW(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn ShowWindow(hwnd: Hwnd, cmd: i32) -> i32;
    fn UpdateWindow(hwnd: Hwnd) -> i32;
    fn GetMessageW(msg: *mut Msg, hwnd: Hwnd, min: u32, max: u32) -> i32;
    fn IsDialogMessageW(hwnd: Hwnd, msg: *const Msg) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn PostQuitMessage(code: i32);
    fn DestroyWindow(hwnd: Hwnd) -> i32;
    fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
    fn SendMessageW(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn GetDlgItem(hwnd: Hwnd, id: i32) -> Hwnd;
    fn GetModuleHandleW(name: *const u16) -> Hinstance;
    fn LoadCursorW(instance: Hinstance, name: *const u16) -> Hcursor;
    fn LoadIconW(instance: Hinstance, name: *const u16) -> Hicon;
    fn AdjustWindowRect(rect: *mut Rect, style: u32, menu: i32) -> i32;
    fn GetSystemMetrics(index: i32) -> i32;
    fn SetProcessDPIAware() -> i32;
    fn SetFocus(hwnd: Hwnd) -> Hwnd;
    fn GetDC(hwnd: Hwnd) -> Hdc;
    fn ReleaseDC(hwnd: Hwnd, dc: Hdc) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateFontW(
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strike_out: u32,
        charset: u32,
        out_precision: u32,
        clip_precision: u32,
        quality: u32,
        pitch: u32,
        face: *const u16,
    ) -> Hfont;
    fn GetDeviceCaps(dc: Hdc, index: i32) -> i32;
}

const WM_CREATE: u32 = 0x0001;
const WM_DESTROY: u32 = 0x0002;
const WM_CLOSE: u32 = 0x0010;
const WM_SETFONT: u32 = 0x0030;
const WM_COMMAND: u32 = 0x0111;
const WM_SETTEXT: u32 = 0x000C;
const BM_GETCHECK: u32 = 0x00F0;
const BST_CHECKED: isize = 1;

const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_BORDER: u32 = 0x0080_0000;
const WS_VSCROLL: u32 = 0x0020_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_CAPTION: u32 = 0x00C0_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_CLIPCHILDREN: u32 = 0x0200_0000;
const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0040;
const ES_READONLY: u32 = 0x0800;
const BS_AUTOCHECKBOX: u32 = 0x0003;
const BS_DEFPUSHBUTTON: u32 = 0x0001;
const SW_SHOW: i32 = 5;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn message_box(parent: Hwnd, text: &str, caption: &str) {
    let t = wide(text);
    let c = wide(caption);
    unsafe {
        MessageBoxW(parent, t.as_ptr(), c.as_ptr(), 0x40);
    }
}

fn msg(text: &str) {
    message_box(std::ptr::null_mut(), text, APP_NAME);
}

fn screen_dpi() -> i32 {
    unsafe {
        let dc = GetDC(std::ptr::null_mut());
        if dc.is_null() {
            return 96;
        }
        let dpi = GetDeviceCaps(dc, 88); // LOGPIXELSX
        ReleaseDC(std::ptr::null_mut(), dc);
        if (96..=480).contains(&dpi) {
            dpi
        } else {
            96
        }
    }
}

unsafe fn set_font(hwnd: Hwnd, font: Hfont) {
    if !hwnd.is_null() && !font.is_null() {
        SendMessageW(hwnd, WM_SETFONT, font as usize, 1);
    }
}

unsafe fn create_agreement_controls(hwnd: Hwnd) {
    let mut rc = Rect { left: 0, top: 0, right: 0, bottom: 0 };
    GetClientRect(hwnd, &mut rc);
    let dpi = screen_dpi();
    let s = |v: i32| v * dpi / 96;

    let cw = rc.right;
    let ch = rc.bottom;
    let m = s(24);
    let title_h = s(26);
    let intro_h = s(40);
    let button_h = s(34);
    let button_w = s(150);
    let cancel_w = s(110);
    let gap = s(12);
    let bottom = s(20);
    let check_h = s(26);
    let btn_y = ch - bottom - button_h;
    let check_y = btn_y - gap - check_h;
    let edit_y = m + title_h + s(4) + intro_h + s(10);
    let edit_h = (check_y - s(12) - edit_y).max(s(80));

    let instance = GetModuleHandleW(std::ptr::null());
    let body_font = CreateFontW(
        -s(15), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, wide("Segoe UI").as_ptr(),
    );
    let title_font = CreateFontW(
        -s(20), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, wide("Segoe UI").as_ptr(),
    );

    let title = wide(&format!("ORVYN {VERSION} — установка"));
    let title_w = CreateWindowExW(
        0,
        wide("STATIC").as_ptr(),
        title.as_ptr(),
        WS_CHILD | WS_VISIBLE,
        m,
        m,
        cw - 2 * m,
        title_h,
        hwnd,
        std::ptr::null_mut(),
        instance,
        std::ptr::null_mut(),
    );
    set_font(title_w, title_font);

    let intro = wide(
        "Прочитай пользовательское соглашение и политику использования ниже. Установка начнётся только после того, как ты поставишь галочку и нажмёшь «Установить».",
    );
    let intro_w = CreateWindowExW(
        0,
        wide("STATIC").as_ptr(),
        intro.as_ptr(),
        WS_CHILD | WS_VISIBLE,
        m,
        m + title_h + s(4),
        cw - 2 * m,
        intro_h,
        hwnd,
        std::ptr::null_mut(),
        instance,
        std::ptr::null_mut(),
    );
    set_font(intro_w, body_font);

    let edit = CreateWindowExW(
        0x0000_0200, // WS_EX_CLIENTEDGE
        wide("EDIT").as_ptr(),
        std::ptr::null(),
        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_VSCROLL | WS_TABSTOP
            | ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL,
        m,
        edit_y,
        cw - 2 * m,
        edit_h,
        hwnd,
        std::ptr::null_mut(),
        instance,
        std::ptr::null_mut(),
    );
    set_font(edit, body_font);
    let docs = format!("{AGREEMENT}\n\n{}\n\n{POLICY}", "=".repeat(72));
    let agreement = wide(&docs.replace('\n', "\r\n"));
    SendMessageW(edit, WM_SETTEXT, 0, agreement.as_ptr() as isize);

    let check = CreateWindowExW(
        0,
        wide("BUTTON").as_ptr(),
        wide("Я принимаю пользовательское соглашение и политику использования").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX,
        m,
        check_y,
        cw - 2 * m,
        check_h,
        hwnd,
        ID_CHECK as isize as Hmenu,
        instance,
        std::ptr::null_mut(),
    );
    set_font(check, body_font);
    SetFocus(check);

    let install = CreateWindowExW(
        0,
        wide("BUTTON").as_ptr(),
        wide("Установить").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON,
        cw - m - button_w,
        btn_y,
        button_w,
        button_h,
        hwnd,
        ID_INSTALL as isize as Hmenu,
        instance,
        std::ptr::null_mut(),
    );
    set_font(install, body_font);

    let cancel = CreateWindowExW(
        0,
        wide("BUTTON").as_ptr(),
        wide("Отмена").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        cw - m - button_w - gap - cancel_w,
        btn_y,
        cancel_w,
        button_h,
        hwnd,
        ID_CANCEL as isize as Hmenu,
        instance,
        std::ptr::null_mut(),
    );
    set_font(cancel, body_font);
}

unsafe extern "system" fn agreement_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    match message {
        WM_CREATE => {
            create_agreement_controls(hwnd);
            0
        }
        WM_COMMAND => {
            match (wparam & 0xffff) as i32 {
                ID_INSTALL => {
                    let check = GetDlgItem(hwnd, ID_CHECK);
                    if !check.is_null() && SendMessageW(check, BM_GETCHECK, 0, 0) == BST_CHECKED {
                        AGREED.store(true, Ordering::SeqCst);
                        DestroyWindow(hwnd);
                    } else {
                        message_box(
                            hwnd,
                            "Чтобы установить ORVYN, прочитай пользовательское соглашение и политику использования и отметь галочку «Я принимаю пользовательское соглашение и политику использования».",
                            APP_NAME,
                        );
                    }
                }
                ID_CANCEL => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

/// Показывает соглашение. Возвращает true, только если пользователь явно принял его.
fn show_agreement() -> bool {
    unsafe {
        SetProcessDPIAware();
        let dpi = screen_dpi();
        let s = |v: i32| v * dpi / 96;
        let screen_w = GetSystemMetrics(0);
        let screen_h = GetSystemMetrics(1);

        let client_w = s(700).min((screen_w - 60).max(s(520)));
        let client_h = s(620).min((screen_h - 100).max(s(440)));
        let style = WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
        let mut wr = Rect { left: 0, top: 0, right: client_w, bottom: client_h };
        AdjustWindowRect(&mut wr, style, 0);
        let win_w = wr.right - wr.left;
        let win_h = wr.bottom - wr.top;
        let x = ((screen_w - win_w) / 2).max(0);
        let y = ((screen_h - win_h) / 2).max(0);

        let instance = GetModuleHandleW(std::ptr::null());
        if instance.is_null() {
            msg("Не удалось запустить установщик.");
            return false;
        }
        let class_name = wide("OrvynSetupAgreement");
        let class = WndClassW {
            style: 0,
            lpfn_wnd_proc: Some(agreement_proc),
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: instance,
            h_icon: {
                let icon = LoadIconW(instance, 1 as *const u16);
                if icon.is_null() {
                    LoadIconW(std::ptr::null_mut(), 32512 as *const u16)
                } else {
                    icon
                }
            },
            h_cursor: LoadCursorW(std::ptr::null_mut(), 32512 as *const u16),
            hbr_background: 6 as Hbrush, // COLOR_WINDOW + 1
            lpsz_menu_name: std::ptr::null(),
            lpsz_class_name: class_name.as_ptr(),
        };
        // Класс регистрируется один раз; ошибка «уже зарегистрирован» не мешает.
        let _ = RegisterClassW(&class);

        let title = wide(&format!("ORVYN {VERSION} — установка"));
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            style,
            x,
            y,
            win_w,
            win_h,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null_mut(),
        );
        if hwnd.is_null() {
            msg("Не удалось открыть окно соглашения.");
            return false;
        }
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        SetFocus(GetDlgItem(hwnd, ID_CHECK));

        let mut message = std::mem::zeroed::<Msg>();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(hwnd, &message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        AGREED.load(Ordering::SeqCst)
    }
}

fn install_dir() -> PathBuf {
    let base = env::var("LOCALAPPDATA").unwrap_or_else(|_| "C:\\".to_string());
    PathBuf::from(base).join("Programs").join(APP_NAME)
}

fn start_menu() -> Option<PathBuf> {
    env::var("APPDATA")
        .ok()
        .map(|p| PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs"))
}

fn desktop() -> Option<PathBuf> {
    env::var("USERPROFILE")
        .ok()
        .map(|p| PathBuf::from(p).join("Desktop"))
        .filter(|p| p.exists())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--uninstall") {
        uninstall();
        return;
    }
    if !args.iter().any(|a| a == "--yes") && !show_agreement() {
        return;
    }
    if !webview_runtime_present() {
        msg(
            "На компьютере не найден Microsoft Edge WebView2 Runtime.\n\n\
             ORVYN работает поверх него, поэтому после установки его нужно поставить отдельно. \
             Это бесплатно и ставится один раз:\n\n\
             https://go.microsoft.com/fwlink/p/?LinkId=2124703\n\n\
             Установка ORVYN продолжится.",
        );
    }
    match install() {
        Ok(exe) => {
            migrate_profile();
            let _ = Command::new(&exe).spawn();
        }
        Err(e) => msg(&format!("Установка не удалась:\n\n{e}")),
    }
}

/// Закрывает старый браузер и убирает установку под прошлым именем.
fn cleanup_old() {
    for old in OLD_NAMES {
        // Старый процесс может держать файлы — закрываем его перед заменой.
        let _ = Command::new("taskkill")
            .args(["/IM", &format!("{old}.exe"), "/F"])
            .silent()
            .status();

        if let Some(sm) = start_menu() {
            let _ = fs::remove_file(sm.join(format!("{old}.lnk")));
        }
        if let Some(dt) = desktop() {
            let _ = fs::remove_file(dt.join(format!("{old}.lnk")));
        }
        let _ = Command::new("reg")
            .args([
                "delete",
                &format!(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{old}"),
                "/f",
            ])
            .silent()
            .status();
        if let Ok(base) = env::var("LOCALAPPDATA") {
            let dir = PathBuf::from(base).join("Programs").join(old);
            if dir.exists() {
                // Файлы могут быть заняты прошлым процессом — не считаем это ошибкой.
                let _ = fs::remove_dir_all(&dir);
                if dir.exists() {
                    let _ = Command::new("cmd")
                        .args(["/c", &format!("ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\"", dir.display())])
                        .silent()
                        .spawn();
                }
            }
        }
    }
}

/// Переносит профиль ORBIT в папку ORVYN, чтобы не потерять настройки и пароли.
fn migrate_profile() {
    let Ok(appdata) = env::var("APPDATA") else { return };
    let old = PathBuf::from(&appdata).join("Orbit");
    let new = PathBuf::from(&appdata).join("Orvyn");
    if old.exists() && !new.exists() {
        let _ = fs::rename(&old, &new);
        // Если переименовать не удалось, браузер повторит перенос при первом запуске.
    }
}

/// Пишет файл, повторяя попытку: антивирус или прошлый процесс могут держать файл открытым.
fn write_retry(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut last = None;
    for _ in 0..40 {
        match fs::write(path, bytes) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    }
    Err(match last {
        Some(e) => format!("не могу записать {}: {e}", path.display()),
        None => format!("не могу записать {}", path.display()),
    })
}

/// Есть ли на машине Microsoft Edge WebView2 Runtime.
///
/// ORVYN не тащит его с собой: Microsoft ставит Runtime вместе с Windows и Edge,
/// а на старых системах его нужно доустановить бесплатно.
fn webview_runtime_present() -> bool {
    let mut roots: Vec<PathBuf> = Vec::new();
    for key in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
        if let Ok(v) = env::var(key) {
            roots.push(PathBuf::from(v).join("Microsoft").join("EdgeWebView").join("Application"));
        }
    }
    roots.iter().any(|root| {
        fs::read_dir(root)
            .map(|it| it.flatten().any(|e| e.path().join("EBWebView").is_dir()))
            .unwrap_or(false)
    })
}

fn install() -> Result<PathBuf, String> {
    cleanup_old();
    // Обновление поверх запущенного ORVYN: закрываем его перед заменой файла.
    let _ = Command::new("taskkill")
        .args(["/IM", "ORVYN.exe", "/F"])
        .silent()
        .status();

    let dir = install_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("не могу создать {}: {e}", dir.display()))?;

    let exe = dir.join(format!("{APP_NAME}.exe"));
    // Старый процесс закрывается не мгновенно: Windows может ещё держать файл.
    write_retry(&exe, PAYLOAD)?;

    // Загрузчик WebView2 кладём рядом: без него Windows не запустит exe на чистой машине.
    let loader = dir.join(LOADER_NAME);
    write_retry(&loader, LOADER_PAYLOAD)?;
    if !exe.is_file() || !loader.is_file() {
        return Err(format!(
            "после установки не хватает файлов в {}: проверь, не блокирует ли запись антивирус",
            dir.display()
        ));
    }

    if let Some(sm) = start_menu() {
        let _ = fs::create_dir_all(&sm);
        shortcut(&sm.join(format!("{APP_NAME}.lnk")), &exe);
    }
    if let Some(dt) = desktop() {
        shortcut(&dt.join(format!("{APP_NAME}.lnk")), &exe);
    }

    let dir_s = dir.display().to_string();
    let exe_s = exe.display().to_string();
    let uninstall_cmd = format!("\"{exe_s}\" --uninstall");
    let size_kb = ((PAYLOAD.len() + LOADER_PAYLOAD.len()) / 1024).to_string();

    let values: [(&str, &str, &str); 9] = [
        ("DisplayName", "REG_SZ", APP_NAME),
        ("DisplayVersion", "REG_SZ", VERSION),
        ("Publisher", "REG_SZ", APP_NAME),
        ("InstallLocation", "REG_SZ", &dir_s),
        ("DisplayIcon", "REG_SZ", &exe_s),
        ("UninstallString", "REG_SZ", &uninstall_cmd),
        ("EstimatedSize", "REG_DWORD", &size_kb),
        ("NoModify", "REG_DWORD", "1"),
        ("NoRepair", "REG_DWORD", "1"),
    ];
    for (name, ty, data) in values {
        let _ = Command::new("reg")
            .args(["add", UNINSTALL_KEY, "/v", name, "/t", ty, "/d", data, "/f"])
            .silent()
            .status();
    }

    Ok(exe)
}

fn uninstall() {
    let _ = Command::new("taskkill")
        .args(["/IM", "ORVYN.exe", "/F"])
        .silent()
        .status();
    let dir = install_dir();
    if let Some(sm) = start_menu() {
        let _ = fs::remove_file(sm.join(format!("{APP_NAME}.lnk")));
    }
    if let Some(dt) = desktop() {
        let _ = fs::remove_file(dt.join(format!("{APP_NAME}.lnk")));
    }
    let _ = Command::new("reg")
        .args(["delete", UNINSTALL_KEY, "/f"])
        .silent()
        .status();

    let script = format!(
        "ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\"",
        dir.display()
    );
    let _ = Command::new("cmd").args(["/c", &script]).silent().spawn();
    msg(&format!("{APP_NAME} удалён"));
}

fn shortcut(lnk: &Path, target: &Path) {
    let work = target.parent().map(|p| p.display().to_string()).unwrap_or_default();
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.WorkingDirectory='{}';$s.IconLocation='{},0';$s.Description='ORVYN — лёгкий браузер';$s.Save()",
        lnk.display(),
        target.display(),
        work,
        target.display()
    );
    let _ = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .silent()
        .status();
}
