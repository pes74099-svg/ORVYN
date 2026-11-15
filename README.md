# ORVYN

![CI](https://github.com/pes74099-svg/ORVYN/actions/workflows/ci.yml/badge.svg)

Лёгкий браузер для Windows на Rust + WebView2. Без Electron и лишних фоновых процессов.

- Движок: Microsoft Edge WebView2 (Chromium).
- Оболочка: Rust, `tao` + `wry`, интерфейс на HTML/CSS/JS.
- Установщик: отдельный Rust-бинарник, кладёт `ORVYN.exe` и `WebView2Loader.dll` в `%LOCALAPPDATA%\Programs\ORVYN`.
- Цена: 0 ₽. Никаких подписок, платных API и телеметрии.

## Скачать

Готовые сборки — в разделе **Releases** этого репозитория.

Нужен Microsoft Edge WebView2 Runtime. На Windows 10 и 11 он обычно уже установлен; если нет — [бесплатный установщик Microsoft](https://go.microsoft.com/fwlink/p/?LinkId=2124703).

Важно: `WebView2Loader.dll` должен лежать рядом с `ORVYN.exe`. Установщик и ZIP кладут его сам, отдельно один exe не переноси.

## Что внутри

- `rustbrowser/` — браузер: окно, вкладки, история и закладки в SQLite, сейф паролей, блокировщик рекламы, память, сессии, темы.
- `rustbrowser-setup/` — установщик с соглашением, ярлыками, записью в «Установку и удаление программ» и миграцией профиля ORBIT → ORVYN.
- `tools/` — проверка и упаковка: smoke-тест через CDP, скриншоты, сборка релиза, шаблон `INSTALL.txt`.

## Сборка

Нужны Rust (GNU-тулчейн для Windows), MinGW-w64 и Node.js 20+ для тестов.

```powershell
cd rustbrowser
cargo build --release
```

Установщик:

```powershell
copy ..\rustbrowser\target\release\orvyn.exe ..\rustbrowser-setup\browser.exe
cd ..\rustbrowser-setup
cargo build --release
```

Полный релиз (браузер + установщик + ZIP + SHA-256):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\package-release.ps1
```

`WebView2Loader.dll` лежит в `rustbrowser-setup/` и вшивается в установщик.

## Проверки

```powershell
cd rustbrowser
cargo test
```

Smoke-тест на живом браузере:

```powershell
$env:ORVYN_CDP_PORT = "9222"
& "$env:LOCALAPPDATA\Programs\ORVYN\ORVYN.exe"
node tools\smoke-test.js
```

## Данные пользователя

Настройки, сессия, история, закладки, сейф и профиль WebView2 лежат в `%APPDATA%\Orvyn`. Логи — в `%APPDATA%\Orvyn\logs\orvyn.log`, только локально. Пароли и содержимое страниц не логируются.

## Разработчики

ORVYN — команда разработчиков  
Автор идеи: Ваня (Собака)  
Оригинальный разработчик: Федотка Ориг  
Кодирование: DeepSeek  
Инструмент разработки: OpenCode  
Промпты: ChatGPT  

© 2026 ORVYN. Все права (не)защищены.
