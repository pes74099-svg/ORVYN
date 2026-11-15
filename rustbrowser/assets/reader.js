/* Режим чтения: вытаскивает статью и показывает её поверх страницы.
   Ничего не меняет на сайте и не отправляет данные наружу. */
(() => {
  const HOST_ID = "__orvyn_reader_host";
  const old = document.getElementById(HOST_ID);
  if (old) { old.remove(); document.documentElement.style.overflow = window.__orvynReaderOverflow || ""; return; }

  let source = document.querySelector("article") || document.querySelector("main");
  if (!source || source.innerText.trim().length < 200) source = document.body;
  if (!source) return;

  const copy = source.cloneNode(true);
  copy.querySelectorAll("script,style,noscript,nav,header,footer,aside,form,button,iframe,svg,video,audio,canvas,object,embed")
      .forEach((e) => e.remove());
  copy.querySelectorAll("[aria-hidden='true'],[role='navigation'],[role='banner'],[role='complementary']")
      .forEach((e) => e.remove());
  copy.querySelectorAll("[class],[id]").forEach((e) => {
    const s = ((e.className || "") + " " + (e.id || "")).toLowerCase();
    if (/(comment|sidebar|share|social|advert|banner|promo|subscribe|newsletter|related|recommend|menu|toolbar|cookie)/.test(s)) {
      e.remove();
    }
  });
  copy.querySelectorAll("a[href]").forEach((a) => {
    try { a.setAttribute("href", new URL(a.getAttribute("href"), location.href).href); } catch (e) {}
    a.setAttribute("target", "_blank");
    a.setAttribute("rel", "noopener noreferrer");
  });
  copy.querySelectorAll("img[src]").forEach((img) => {
    try { img.setAttribute("src", new URL(img.getAttribute("src"), location.href).href); } catch (e) {}
    img.removeAttribute("srcset");
    img.removeAttribute("sizes");
  });

  const title = (document.querySelector("h1") || {}).innerText || document.title || "";
  let size = 19;

  const host = document.createElement("div");
  host.id = HOST_ID;
  host.style.cssText = "position:fixed;inset:0;z-index:2147483647;background:#fbfaf7;";
  const root = host.attachShadow({ mode: "open" });
  root.innerHTML = `
    <style>
      :host{all:initial}
      .bar{position:sticky;top:0;z-index:2;display:flex;align-items:center;gap:8px;
           padding:10px 18px;background:rgba(251,250,247,.94);border-bottom:1px solid #e6e1d8;
           font:13px/1.3 "Segoe UI",system-ui,sans-serif;color:#5a6274;backdrop-filter:blur(8px)}
      .bar .t{flex:1 1 auto;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:#252b38;font-weight:600}
      .bar button{height:28px;padding:0 10px;border-radius:8px;border:1px solid #ddd7cc;background:#fff;
                  color:#3d4658;font:12px "Segoe UI",system-ui,sans-serif;cursor:pointer}
      .bar button:hover{background:#f2eee6}
      .page{max-width:760px;margin:0 auto;padding:34px 26px 90px;color:#252b38;
            font:${size}px/1.65 Georgia,"Times New Roman",serif;user-select:text;overflow-wrap:break-word}
      .page h1{font:700 34px/1.2 "Segoe UI",system-ui,sans-serif;margin:0 0 20px;letter-spacing:-.02em}
      .page h2,.page h3{font-family:"Segoe UI",system-ui,sans-serif;line-height:1.3;margin:1.4em 0 .6em}
      .page p{margin:0 0 1.05em}
      .page img{max-width:100%;height:auto;border-radius:10px;display:block;margin:18px auto}
      .page a{color:#0b6f9e}
      .page blockquote{margin:18px 0;padding:4px 18px;border-left:3px solid #d8cfc0;color:#5d6577}
      .page pre,.page code{font-family:Consolas,monospace;font-size:.86em;background:#f0ece3;border-radius:6px}
      .page pre{padding:12px 14px;overflow:auto}
      .page figure{margin:18px 0}
      .page figcaption{font:12px "Segoe UI",system-ui,sans-serif;color:#7a8192;text-align:center}
      .page table{max-width:100%;border-collapse:collapse;font:13px "Segoe UI",system-ui,sans-serif}
      .page td,.page th{border:1px solid #e0dbd1;padding:6px 8px}
      .page ul,.page ol{padding-left:1.4em}
    </style>
    <div class="bar">
      <span class="t"></span>
      <button id="minus" title="Меньше шрифт">A−</button>
      <button id="plus" title="Больше шрифт">A+</button>
      <button id="close" title="Закрыть (Esc)">Закрыть</button>
    </div>
    <div class="page" id="page"></div>`;
  document.documentElement.appendChild(host);

  const bar = root.querySelector(".t");
  bar.textContent = title;
  const page = root.getElementById("page");
  const h1 = document.createElement("h1");
  h1.textContent = title;
  page.appendChild(h1);
  const body = document.createElement("div");
  body.innerHTML = copy.innerHTML;
  page.appendChild(body);

  const prev = document.documentElement.style.overflow;
  window.__orvynReaderOverflow = prev;
  document.documentElement.style.overflow = "hidden";
  try { window.__orvynReaderScroll = window.scrollY; } catch (e) {}
  window.scrollTo(0, 0);

  const setSize = (d) => {
    size = Math.max(14, Math.min(28, size + d));
    page.style.fontSize = size + "px";
  };
  root.getElementById("plus").onclick = () => setSize(1);
  root.getElementById("minus").onclick = () => setSize(-1);
  const close = () => {
    host.remove();
    document.documentElement.style.overflow = prev;
    try { window.scrollTo(0, window.__orvynReaderScroll || 0); } catch (e) {}
    document.removeEventListener("keydown", onKey, true);
  };
  root.getElementById("close").onclick = close;
  function onKey(e) {
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); close(); }
  }
  document.addEventListener("keydown", onKey, true);
})();
