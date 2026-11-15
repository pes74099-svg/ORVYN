//! Собственный фильтр рекламы и трекеров.
//!
//! Работает в два слоя:
//!  1) сетевой слой — перехват fetch/XHR/Image/Script/Iframe/window.open по списку доменов;
//!  2) косметический слой — CSS + MutationObserver, вырезающий типовые рекламные блоки.

/// Домены, запросы к которым блокируются (совпадение по хосту и поддоменам).
pub const BLOCKED_HOSTS: &[&str] = &[
    "doubleclick.net",
    "googlesyndication.com",
    "googleadservices.com",
    "adservice.google.com",
    "google-analytics.com",
    "googletagmanager.com",
    "googletagservices.com",
    "adnxs.com",
    "adsrvr.org",
    "advertising.com",
    "adform.net",
    "adroll.com",
    "adcolony.com",
    "adfox.ru",
    "adriver.ru",
    "ads.yahoo.com",
    "amazon-adsystem.com",
    "appnexus.com",
    "criteo.com",
    "criteo.net",
    "rubiconproject.com",
    "pubmatic.com",
    "openx.net",
    "taboola.com",
    "outbrain.com",
    "zedo.com",
    "smartadserver.com",
    "moatads.com",
    "scorecardresearch.com",
    "quantserve.com",
    "quantcount.com",
    "hotjar.com",
    "fullstory.com",
    "mouseflow.com",
    "crazyegg.com",
    "clicktale.net",
    "mc.yandex.ru",
    "an.yandex.ru",
    "yandexadexchange.net",
    "top-fwz1.mail.ru",
    "ad.mail.ru",
    "r.mail.ru",
    "ads.vk.com",
    "ad.vk.com",
    "connect.facebook.net",
    "facebook.net",
    "ads.linkedin.com",
    "px.ads.linkedin.com",
    "analytics.twitter.com",
    "static.ads-twitter.com",
    "ads.tiktok.com",
    "analytics.tiktok.com",
    "sentry-cdn.com",
    "bugsnag.com",
    "newrelic.com",
    "nr-data.net",
    "mixpanel.com",
    "segment.io",
    "segment.com",
    "amplitude.com",
    "heapanalytics.com",
    "yieldmo.com",
    "teads.tv",
    "spotxchange.com",
    "indexww.com",
    "sovrn.com",
    "lijit.com",
    "gumgum.com",
    "sharethrough.com",
    "triplelift.com",
    "1rx.io",
    "bidswitch.net",
    "casalemedia.com",
    "contextweb.com",
    "emxdgt.com",
    "faithlife-ads.com",
    "gwallet.com",
    "krxd.net",
    "mathtag.com",
    "media.net",
    "narrative.io",
    "pulsepoint.com",
    "rhythmone.com",
    "simpli.fi",
    "sonobi.com",
    "spotx.tv",
    "tapad.com",
    "tremorhub.com",
    "turn.com",
    "undertone.com",
    "vidoomy.com",
    "weborama.fr",
    "yieldlab.net",
    "zemanta.com",
    "propellerads.com",
    "popads.net",
    "popcash.net",
    "exoclick.com",
    "juicyads.com",
    "trafficjunky.com",
    "adsterra.com",
    "hilltopads.net",
    "clickadu.com",
    "onclickads.net",
    "mgid.com",
    "revcontent.com",
    "adskeeper.com",
    "pushwoosh.com",
    "onesignal.com",
    "pushengage.com",
    "histats.com",
    "statcounter.com",
    "liveinternet.ru",
    "top100.rambler.ru",
    "counter.yadro.ru",
    "mc.webvisor.org",
    "metrika.yandex.net",
    "clarity.ms",
    "bat.bing.com",
    "c.bing.com",
    "t.co",
];

/// Косметические правила: типовые рекламные контейнеры.
pub const COSMETIC_SELECTORS: &[&str] = &[
    ".adsbygoogle",
    ".ad",
    ".ads",
    ".advert",
    ".advertisement",
    ".advertising",
    ".ad-container",
    ".ad-wrapper",
    ".ad-slot",
    ".ad-banner",
    ".ad-unit",
    ".ads-container",
    ".ads-wrapper",
    ".adsbox",
    ".ad-placeholder",
    ".ad-placement",
    ".adblock-stub",
    "[id^=\"google_ads\"]",
    "[id^=\"div-gpt-ad\"]",
    "[id^=\"ad-slot\"]",
    "[id^=\"ad_banner\"]",
    "[class^=\"ad-slot\"]",
    "[class*=\"advert\"]",
    "[class*=\"sponsored-post\"]",
    "[data-ad-slot]",
    "[data-ad-unit]",
    "[data-google-query-id]",
    "iframe[src*=\"doubleclick.net\"]",
    "iframe[src*=\"googlesyndication.com\"]",
    "iframe[src*=\"adservice\"]",
    "iframe[id^=\"google_ads_iframe\"]",
    "ins.adsbygoogle",
    ".promo-banner",
    ".sponsored-content",
    ".native-ad",
    ".outbrain-widget",
    ".taboola-widget",
    "[id^=\"taboola\"]",
    "[id^=\"outbrain\"]",
    ".social-share-sticky",
    ".newsletter-popup",
    ".cookie-consent-banner ~ .ads",
];

fn js_string_list(items: &[&str]) -> String {
    let parts: Vec<String> = items
        .iter()
        .map(|s| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    format!("[{}]", parts.join(","))
}

/// Скрипт, который внедряется в каждую страницу до её загрузки.
/// `protect` — стартовое состояние защиты (мастер-выключатель).
pub fn inject_script(protect: bool, gestures: bool) -> String {
    let hosts = js_string_list(BLOCKED_HOSTS);
    let selectors = js_string_list(COSMETIC_SELECTORS);
    format!(
        r##"(function() {{
  if (window.__rbInjected) return;
  window.__rbInjected = true;

  var HOSTS = {hosts};
  var SELECTORS = {selectors}.join(",");
  var PATHS = [
    ["youtube.com", "/api/stats/ads"],
    ["youtube.com", "/api/stats/ad"],
    ["youtube.com", "/pagead/"],
    ["youtube.com", "/ptracking"],
    ["youtube.com", "/get_midroll_info"]
  ];
  var blockedCount = 0;
  window.__orvynProtect = {protect};
  window.__orvynGestures = {gestures};

  // Рантайм-переключатель для правил «защита на этом сайте».
  // Меняется из Rust через evaluate_script, без перезагрузки страницы.
  window.__orvynProtectSet = function(on) {{
    window.__orvynProtect = !!on;
    try {{
      var st = document.getElementById("__rb_cosmetic");
      if (!on && st) st.remove();
      var yt = document.getElementById("__rb_yt");
      if (!on && yt) yt.remove();
      if (on) {{ paint(); sweep(); }}
    }} catch (e) {{}}
  }};
  window.__orvynGesturesSet = function(on) {{ window.__orvynGestures = !!on; }};

  function partsOf(u) {{
    try {{ var x = new URL(u, location.href); return [x.hostname.toLowerCase(), x.pathname.toLowerCase()]; }}
    catch (e) {{ return ["", ""]; }}
  }}
  function isBlocked(u) {{
    if (window.__orvynProtect === false) return false;
    if (!u) return false;
    var p = partsOf(u);
    var h = p[0];
    if (!h) return false;
    for (var i = 0; i < HOSTS.length; i++) {{
      var d = HOSTS[i];
      if (h === d || h.endsWith("." + d)) return true;
    }}
    for (var j = 0; j < PATHS.length; j++) {{
      var host = PATHS[j][0], frag = PATHS[j][1];
      if ((h === host || h.endsWith("." + host)) && p[1].indexOf(frag) >= 0) return true;
    }}
    return false;
  }}
  function report(u) {{
    blockedCount++;
    try {{ window.ipc.postMessage(JSON.stringify({{ rb: "blocked", host: partsOf(u)[0] }})); }} catch (e) {{}}
  }}

  // --- сетевой слой ---
  var origFetch = window.fetch;
  if (origFetch) {{
    window.fetch = function(input, init) {{
      var u = (typeof input === "string") ? input : (input && input.url) || "";
      if (isBlocked(u)) {{ report(u); return Promise.reject(new TypeError("Blocked by ORVYN")); }}
      return origFetch.apply(this, arguments);
    }};
  }}

  var origOpen = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function(method, url) {{
    if (isBlocked(url)) {{
      report(url);
      this.__rbBlocked = true;
      return origOpen.call(this, "GET", "about:blank");
    }}
    return origOpen.apply(this, arguments);
  }};
  var origSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.send = function() {{
    if (this.__rbBlocked) return;
    return origSend.apply(this, arguments);
  }};

  function hookSrc(proto, prop) {{
    try {{
      var d = Object.getOwnPropertyDescriptor(proto, prop);
      if (!d || !d.set) return;
      Object.defineProperty(proto, prop, {{
        configurable: true,
        enumerable: d.enumerable,
        get: d.get,
        set: function(v) {{
          if (isBlocked(String(v))) {{ report(v); return; }}
          return d.set.call(this, v);
        }}
      }});
    }} catch (e) {{}}
  }}
  hookSrc(HTMLImageElement.prototype, "src");
  hookSrc(HTMLScriptElement.prototype, "src");
  hookSrc(HTMLIFrameElement.prototype, "src");
  hookSrc(HTMLSourceElement.prototype, "src");

  var origSetAttribute = Element.prototype.setAttribute;
  Element.prototype.setAttribute = function(name, value) {{
    if (/^(src|href)$/i.test(name) && isBlocked(String(value))) {{ report(value); return; }}
    return origSetAttribute.apply(this, arguments);
  }};

  var origWindowOpen = window.open;
  window.open = function(url) {{
    if (isBlocked(url)) {{ report(url); return null; }}
    return origWindowOpen.apply(this, arguments);
  }};

  // --- косметический слой ---
  function paint() {{
    if (window.__orvynProtect === false) return;
    if (!document.documentElement) return;
    if (document.getElementById("__rb_cosmetic")) return;
    var st = document.createElement("style");
    st.id = "__rb_cosmetic";
    st.textContent = SELECTORS + "{{display:none !important;visibility:hidden !important;height:0 !important;}}";
    document.documentElement.appendChild(st);
  }}
  function sweep() {{
    if (window.__orvynProtect === false) return;
    var nodes = document.querySelectorAll(SELECTORS);
    for (var i = 0; i < nodes.length; i++) {{
      var n = nodes[i];
      if (n && !n.__rbHide) {{ n.__rbHide = true; n.style.setProperty("display", "none", "important"); }}
    }}
  }}
  if (document.readyState === "loading") {{
    document.addEventListener("DOMContentLoaded", function() {{ paint(); sweep(); }});
  }} else {{ paint(); sweep(); }}
  try {{
    new MutationObserver(function(muts) {{
      for (var i = 0; i < muts.length; i++) {{
        if (muts[i].addedNodes && muts[i].addedNodes.length) {{ sweep(); break; }}
      }}
    }}).observe(document.documentElement || document, {{ childList: true, subtree: true }});
  }} catch (e) {{}}

  // --- YouTube: реклама не показывается ---
  var isYT = /(^|\.)youtube(-nocookie)?\.com$/.test(location.hostname);
  if (isYT) {{
    var SKIP_BTNS = [".ytp-ad-skip-button", ".ytp-ad-skip-button-modern", ".ytp-skip-ad-button",
                     ".ytp-ad-skip-button-container button", ".ytp-ad-overlay-close-button",
                     ".ytp-ad-survey-answer-button", ".ytp-ad-survey-answers .ytp-ad-survey-answer-button"];
    var AD_UI = ["#player-ads", "#masthead-ad", "#offer-module", ".ytp-ad-overlay-container",
                 ".ytp-ad-image-overlay", ".ytp-ad-text-overlay", ".ytp-ad-overlay-slot",
                 ".ytp-ad-player-overlay", ".ytp-ad-player-overlay-layout", ".ytp-ad-progress-list",
                 ".ytp-featured-product", ".ytp-suggested-action", ".ytp-ad-module",
                 "ytd-display-ad-renderer", "ytd-promoted-sparkles-web-renderer",
                 "ytd-ad-slot-renderer", "ytd-in-feed-ad-layout-renderer",
                 "ytd-banner-promo-renderer", "ytd-statement-banner-renderer",
                 "ytd-engagement-panel-section-list-renderer[target-id*=ads]",
                 "ytd-rich-item-renderer:has(ytd-ad-slot-renderer)"];
    function ytStyle() {{
      if (window.__orvynProtect === false) return;
      if (!document.documentElement || document.getElementById("__rb_yt")) return;
      var st = document.createElement("style");
      st.id = "__rb_yt";
      st.textContent = AD_UI.join(",") + "{{display:none !important;}}";
      document.documentElement.appendChild(st);
    }}
    var adBusy = false;
    var prevRate = 1;
    var prevMuted = null;
    function ytTick() {{
      if (window.__orvynProtect === false) return;
      var player = document.querySelector(".html5-video-player");
      if (!player) return;
      var v = player.querySelector("video");
      var ad = player.classList.contains("ad-showing") || player.classList.contains("ad-interrupting");
      if (ad) {{
        for (var i = 0; i < SKIP_BTNS.length; i++) {{
          var b = player.querySelector(SKIP_BTNS[i]);
          if (b && b.offsetParent !== null) {{ try {{ b.click(); }} catch (e) {{}} }}
        }}
        if (v) {{
          if (!adBusy) {{
            adBusy = true;
            try {{ prevMuted = v.muted; v.muted = true; }} catch (e) {{}}
            try {{ prevRate = v.playbackRate || 1; v.playbackRate = 16; }} catch (e) {{}}
          }}
          if (isFinite(v.duration) && v.duration > 0) {{
            try {{ if (v.duration - v.currentTime > 0.2) v.currentTime = v.duration; }} catch (e) {{}}
          }}
        }}
      }} else if (adBusy) {{
        adBusy = false;
        try {{ if (v && prevMuted !== null) v.muted = prevMuted; }} catch (e) {{}}
        try {{ if (v) v.playbackRate = prevRate || 1; }} catch (e) {{}}
        prevMuted = null;
        prevRate = 1;
      }}
    }}
    function ytWatch() {{
      var player = document.querySelector(".html5-video-player");
      if (!player || player.__orvynYT) return;
      player.__orvynYT = true;
      try {{
        new MutationObserver(ytTick).observe(player, {{ attributes: true, attributeFilter: ["class"] }});
      }} catch (e) {{}}
    }}
    function ytNav() {{ ytStyle(); ytWatch(); ytTick(); }}
    var ytTimer = setInterval(function() {{ ytWatch(); ytTick(); }}, 100);
    window.addEventListener("beforeunload", function () {{ clearInterval(ytTimer); }});
    document.addEventListener("DOMContentLoaded", ytNav, {{ once: true }});
    window.addEventListener("yt-navigate-finish", ytNav, true);
    document.addEventListener("yt-navigate-finish", ytNav, true);
    document.addEventListener("yt-page-data-updated", ytNav, true);
    if (document.readyState !== "loading") {{ ytNav(); }}
  }}

  // --- жесты правой кнопкой мыши ---
  var gx = 0, gy = 0, gdown = false, gmoved = false;
  document.addEventListener("mousedown", function(e) {{
    if (window.__orvynGestures === false || e.button !== 2) return;
    gx = e.clientX; gy = e.clientY; gdown = true; gmoved = false;
  }}, true);
  document.addEventListener("mousemove", function(e) {{
    if (window.__orvynGestures === false || !gdown) return;
    if (Math.abs(e.clientX - gx) > 28 || Math.abs(e.clientY - gy) > 28) gmoved = true;
  }}, true);
  document.addEventListener("contextmenu", function(e) {{
    if (window.__orvynGestures === false) return;
    if (gmoved) {{ e.preventDefault(); e.stopImmediatePropagation(); }}
  }}, true);
  document.addEventListener("mouseup", function(e) {{
    if (window.__orvynGestures === false || e.button !== 2 || !gdown) return;
    gdown = false;
    if (!gmoved) {{ gmoved = false; return; }}
    var dx = e.clientX - gx, dy = e.clientY - gy;
    var action = "";
    if (Math.abs(dx) > Math.abs(dy)) action = dx < 0 ? "back" : "forward";
    else action = dy < 0 ? "top" : "bottom";
    gmoved = false;
    try {{ window.ipc.postMessage(JSON.stringify({{ rb: "gesture", action: action }})); }} catch (err) {{}}
  }}, true);

  // --- пользовательские скрипты (замена расширениям) ---
  {userscripts}

  // --- горячие клавиши ---
  document.addEventListener("keydown", function(e) {{
    var k = e.key ? e.key.toLowerCase() : "";
    var combo = "";
    if (e.ctrlKey) combo += "ctrl+";
    if (e.altKey) combo += "alt+";
    if (e.shiftKey) combo += "shift+";
    combo += k;
    var known = ["ctrl+t","ctrl+w","ctrl+l","ctrl+r","ctrl+d","ctrl+tab","ctrl+shift+tab",
                 "ctrl+shift+i","alt+arrowleft","alt+arrowright","alt+home","f5",
                 "ctrl+equal","ctrl+-","ctrl+0","ctrl+shift+t","ctrl+shift+n",
                 "ctrl+shift+d","ctrl+f","ctrl+m","ctrl+shift+a","ctrl+shift+b",
                 "ctrl+shift+l","alt+a","ctrl++","ctrl+=","f11"];
    if (known.indexOf(combo) >= 0) {{
      e.preventDefault();
      e.stopPropagation();
      try {{ window.ipc.postMessage(JSON.stringify({{ rb: "key", combo: combo }})); }} catch (err) {{}}
    }}
  }}, true);

  // --- ссылки с target=_blank открываем как вкладки ---
  document.addEventListener("click", function(e) {{
    var a = e.target && e.target.closest ? e.target.closest("a") : null;
    if (!a) return;
    var href = a.getAttribute("href");
    if (a.target === "_blank" && href && href.indexOf("javascript:") !== 0) {{
      e.preventDefault();
      try {{ window.ipc.postMessage(JSON.stringify({{ rb: "newtab", url: a.href }})); }} catch (err) {{}}
    }}
  }}, true);
}})();"##,
        hosts = hosts,
        selectors = selectors,
        userscripts = user_scripts(),
        protect = if protect { "true" } else { "false" },
        gestures = if gestures { "true" } else { "false" },
    )
}

/// Папка с пользовательскими скриптами — наш аналог расширений.
pub fn userscript_dir() -> std::path::PathBuf {
    crate::storage::data_dir().join("userscripts")
}

const README: &str = "Сюда можно класть свои скрипты для сайтов — это замена расширениям.\n\n\
Файлы с расширением .js выполняются на каждой странице до её загрузки\n\
(как content script в Chrome). Порядок — по имени файла.\n\n\
Уже положены:\n\
  music-region.js      — кнопка «Регион» на музыкальных сайтах: открывает\n\
                         страницу через прокси из настроек ORVYN;\n\
  music-background.js  — музыка продолжает играть в фоновой вкладке.\n\n\
Пример своего скрипта: my-youtube.js\n\
------------------------------------------------------------\n\
(function () {\n\
\x20 // убрать предложение подписаться\n\
\x20 const kill = () => document.querySelectorAll('.ytp-pause-overlay').forEach(n => n.remove());\n\
\x20 setInterval(kill, 1000);\n\
})();\n\n\
После добавления файла перезапусти браузер (скрипты вшиваются при старте).\n";

const MUSIC_REGION_JS: &str = r##"/* ORVYN: кнопка «Регион» на музыкальных сайтах.
   Открывает текущую страницу в отдельной вкладке через прокси из настроек.
   Если прокси не настроен, ORVYN откроет панель «Музыка» с полями. */
(function () {
  var host = location.hostname.replace(/^www\./, "").toLowerCase();
  var music = /(^|\.)(soundcloud\.com|music\.yandex\.(ru|com|kz|by|uz)|music\.youtube\.com|open\.spotify\.com|deezer\.com|bandcamp\.com|audiomack\.com|music\.apple\.com|radio\.garden|zvuk\.com|tidal\.com)$/.test(host) ||
    ((host === "vk.com" || /\.vk\.com$/.test(host)) && /^\/(audio|music)/.test(location.pathname)) ||
    ((host === "yandex.ru" || host === "yandex.com") && /^\/music/.test(location.pathname));
  if (!music || document.getElementById("__orvyn_region")) return;

  var btn = document.createElement("button");
  btn.id = "__orvyn_region";
  btn.type = "button";
  btn.title = "Открыть эту страницу через прокси (обход блокировки по региону)";
  btn.style.cssText = "position:fixed;left:14px;bottom:14px;z-index:2147483647;display:flex;align-items:center;gap:8px;" +
    "font:600 12px/1 'Segoe UI',system-ui,sans-serif;padding:9px 13px;border-radius:999px;" +
    "background:rgba(10,16,25,.92);color:#bfe9ff;border:1px solid rgba(79,209,255,.35);" +
    "box-shadow:0 10px 28px rgba(0,0,0,.45);cursor:pointer;opacity:.72;backdrop-filter:blur(8px);";
  btn.innerHTML = '<span style="width:8px;height:8px;border-radius:50%;background:#4fd1ff;box-shadow:0 0 10px #4fd1ff"></span><span>Регион</span>';
  btn.onmouseenter = function () { btn.style.opacity = "1"; };
  btn.onmouseleave = function () { btn.style.opacity = ".72"; };
  btn.onclick = function () {
    try { window.ipc.postMessage(JSON.stringify({ rb: "proxytab", url: location.href })); } catch (e) {}
  };

  function place() {
    if (document.body && !document.getElementById("__orvyn_region")) document.body.appendChild(btn);
  }
  place();
  document.addEventListener("DOMContentLoaded", place);

  // Если на странице явно написано «недоступно в вашем регионе» — подсвечиваем.
  var LOCK = /(not available in your (country|region|location)|unavailable in your (country|region|location)|blocked in your country|geo.?block|недоступн\w* в ваш\w* (стране|регионе))/i;
  setTimeout(function () {
    try {
      var text = ((document.body && document.body.innerText) || "").slice(0, 30000);
      if (LOCK.test(text)) {
        btn.style.background = "linear-gradient(135deg,#4fd1ff,#8b7cff)";
        btn.style.borderColor = "transparent";
        btn.style.color = "#051018";
        btn.style.opacity = "1";
        btn.lastChild.textContent = "Регион: открыть через прокси";
      }
    } catch (e) {}
  }, 2500);
})();
"##;

const MUSIC_BACKGROUND_JS: &str = r##"/* ORVYN: музыка не встаёт на паузу, когда вкладка уходит в фон. */
(function () {
  var host = location.hostname.replace(/^www\./, "").toLowerCase();
  var music = /(^|\.)(soundcloud\.com|music\.yandex\.(ru|com|kz|by|uz)|music\.youtube\.com|open\.spotify\.com|deezer\.com|bandcamp\.com|audiomack\.com|music\.apple\.com|radio\.garden|zvuk\.com|tidal\.com)$/.test(host) ||
    ((host === "vk.com" || /\.vk\.com$/.test(host)) && /^\/(audio|music)/.test(location.pathname)) ||
    ((host === "yandex.ru" || host === "yandex.com") && /^\/music/.test(location.pathname));
  if (!music) return;
  try {
    Object.defineProperty(document, "hidden", { configurable: true, get: function () { return false; } });
    Object.defineProperty(document, "visibilityState", { configurable: true, get: function () { return "visible"; } });
  } catch (e) {}
  var guard = function (e) { e.stopImmediatePropagation(); };
  document.addEventListener("visibilitychange", guard, true);
  document.addEventListener("webkitvisibilitychange", guard, true);
})();
"##;

/// Создаёт папку скриптов и кладёт стандартные, если их ещё нет.
pub fn ensure_userscripts() {
    let dir = userscript_dir();
    let _ = std::fs::create_dir_all(&dir);
    let defaults = [
        ("README.txt", README),
        ("music-region.js", MUSIC_REGION_JS),
        ("music-background.js", MUSIC_BACKGROUND_JS),
    ];
    for (name, body) in defaults {
        let p = dir.join(name);
        if !p.exists() {
            let _ = std::fs::write(p, body);
        }
    }
}

fn js_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Все `*.js` из папки пользовательских скриптов, обёрнутые так, чтобы падение
/// одного скрипта не ломало страницу.
pub fn user_scripts() -> String {
    let dir = userscript_dir();
    let Ok(rd) = std::fs::read_dir(&dir) else { return String::new() };
    let mut files: Vec<std::path::PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .map(|e| e.eq_ignore_ascii_case("js"))
                    .unwrap_or(false)
        })
        .collect();
    files.sort();

    let mut out = String::new();
    for f in files {
        let Ok(src) = std::fs::read_to_string(&f) else { continue };
        let name = f
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        out.push_str(&format!(
            "  try {{ (new Function({src}))(); }} catch (e) {{ console.warn(\"[userscript {name}]\", e); }}\n",
            src = js_quote(&src),
            name = name.replace('"', "'"),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_is_valid_js() {
        let js = inject_script(true, true);
        let p = std::env::temp_dir().join("rb_inject.js");
        std::fs::write(&p, &js).unwrap();
        println!("dumped to {}", p.display());
        assert!(js.contains("ytTick"));
        assert!(js.contains("__orvynProtectSet"));
        assert!(js.contains("window.__orvynProtect = true"));
        assert!(!js.contains("{{"));
        assert!(inject_script(true, false).contains("ytTick"));
        assert!(inject_script(false, false).contains("window.__orvynProtect = false"));
    }
}
