# ORVYN tools

Вспомогательные скрипты для проверки и упаковки сборки. Ничего не устанавливают и не требуют платных сервисов.

## smoke-test.js

Проверяет живой браузер через CDP: вкладки, навигацию, закрытие, панель настроек, пресеты оформления.

Запуск:

```powershell
$env:ORVYN_CDP_PORT = "9222"
& "$env:LOCALAPPDATA\Programs\ORVYN\ORVYN.exe"
node tools\smoke-test.js
```

Требуется Node.js 20+ (встроенные `fetch` и `WebSocket`).

Важно:

- сначала закрой другие копии ORVYN — второй экземпляр не запустится;
- тест открывает и закрывает одну вкладку, меняет пресет оформления и возвращает его обратно;
- профиль, история и закладки не очищаются;
- при ошибке скрипт завершается с кодом 1 и печатает `SMOKE FAIL`.

## package-release.ps1

Собирает релиз целиком: браузер, установщик, папку `C:\orbit\releases\ORVYN-2.2.0`, оба ZIP и `SHA256SUMS.txt`.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File C:\orbit\tools\package-release.ps1
```

Полезные детали:

- перед сборкой закрывает запущенные `ORVYN.exe` и `OrvynSetup.exe`;
- запускает `cargo test`, если не указан `-SkipTests`;
- кладёт `WebView2Loader.dll` рядом с `ORVYN.exe` в релизе и внутрь установщика — без этого файла Windows не запустит exe на чистой машине;
- текст `INSTALL.txt` берётся из `tools\INSTALL-template.txt`;
- архив исходников собирается через `tar` и включает `rustbrowser`, `rustbrowser-setup` и `tools`.

## Что проверить после упаковки

1. Распаковать `ORVYN-2.2.0.zip` в пустую папку и запустить `ORVYN.exe` — рядом должен лежать `WebView2Loader.dll`.
2. Запустить `OrvynSetup-2.2.0.exe`, пройти соглашение и убедиться, что в `%LOCALAPPDATA%\Programs\ORVYN` появились оба файла.
3. Сверить хеши с `SHA256SUMS.txt`.

## Мелкие помощники

Все требуют запущенный ORVYN с `ORVYN_CDP_PORT=9222`.

- `ui-eval.js "<js>"` — выполнить JS именно в интерфейсе ORVYN, а не в открытой странице. Удобно, когда на вкладках висят сайты.
- `shot-cdp.js C:\path\out.png` — скриншот интерфейса через CDP. Надёжнее, чем PrintWindow: WebView2 не всегда отдаёт свежий кадр.
- `check-ui-js.js` — проверить синтаксис JS внутри `ui.html` без запуска браузера.
- `diag-panel.js` — показать цели CDP, найти интерфейс и открыть настройки.
- `pages-text.js` — вывести адрес, заголовок и начало текста каждой вкладки. Помогает понять, что реально загрузилось.
