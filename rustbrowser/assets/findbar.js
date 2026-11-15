/* Поиск по странице. Вставляется в активную вкладку через evaluate_script.
   Ничего не хранит и не отправляет наружу: только подсветка совпадений. */
(() => {
  const HOST_ID = "__orvyn_find_host";
  const STYLE_ID = "__orvyn_find_style";
  const existing = document.getElementById(HOST_ID);
  if (existing) {
    const box = existing.shadowRoot && existing.shadowRoot.getElementById("q");
    if (box) { box.focus(); box.select(); }
    return;
  }
  if (!document.body) return;

  const host = document.createElement("div");
  host.id = HOST_ID;
  host.style.cssText = "position:fixed;right:18px;bottom:18px;z-index:2147483647;";
  const root = host.attachShadow({ mode: "open" });
  root.innerHTML = `
    <style>
      .bar{display:flex;align-items:center;gap:6px;padding:8px 9px;border-radius:12px;
           background:#111826;color:#e9eefb;border:1px solid #2a3850;
           box-shadow:0 12px 40px rgba(0,0,0,.55);font:13px/1.2 "Segoe UI",system-ui,sans-serif}
      input{width:210px;height:28px;padding:0 9px;border-radius:8px;border:1px solid #33445f;
            background:#080d15;color:#e9eefb;outline:none;font:inherit}
      input:focus{border-color:#4fd1ff;box-shadow:0 0 0 2px rgba(79,209,255,.16)}
      .n{min-width:44px;text-align:center;color:#8d9ab3;font-size:11.5px;font-variant-numeric:tabular-nums}
      button{width:26px;height:26px;border:0;border-radius:7px;background:transparent;color:#a9b7cc;
             cursor:pointer;font:15px/1 "Segoe UI",sans-serif}
      button:hover{background:#1d293b;color:#fff}
      .x{font-size:13px;color:#8d9ab3}
    </style>
    <div class="bar">
      <input id="q" placeholder="Найти на странице" spellcheck="false" autocomplete="off">
      <span class="n" id="n"></span>
      <button id="prev" title="Предыдущее (Shift+Enter)">↑</button>
      <button id="next" title="Следующее (Enter)">↓</button>
      <button class="x" id="close" title="Закрыть (Esc)">✕</button>
    </div>`;
  document.documentElement.appendChild(host);

  const style = document.createElement("style");
  style.id = STYLE_ID;
  style.textContent = `
    ::highlight(orvyn-find){background:#ffe27a;color:#10151f}
    ::highlight(orvyn-find-active){background:#ff9d3c;color:#10151f}
  `;
  document.head.appendChild(style);

  const q = root.getElementById("q");
  const count = root.getElementById("n");
  const supported = !!(window.CSS && CSS.highlights && window.Highlight);
  let matches = [];
  let active = -1;
  let main = null;
  let activeHl = null;

  function clearMarks() {
    if (supported) {
      try { CSS.highlights.delete("orvyn-find"); } catch (e) {}
      try { CSS.highlights.delete("orvyn-find-active"); } catch (e) {}
    }
  }

  function rangeOf(m) {
    const r = document.createRange();
    r.setStart(m.node, m.start);
    r.setEnd(m.node, m.end);
    return r;
  }

  function collect(term) {
    clearMarks();
    matches = [];
    active = -1;
    if (!term) { count.textContent = ""; return; }
    const low = term.toLocaleLowerCase();
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {
      acceptNode(n) {
        const p = n.parentElement;
        if (!p) return NodeFilter.FILTER_REJECT;
        const tag = p.tagName;
        if (tag === "SCRIPT" || tag === "STYLE" || tag === "NOSCRIPT" ||
            tag === "TEXTAREA" || tag === "INPUT" || tag === "SELECT") {
          return NodeFilter.FILTER_REJECT;
        }
        if (p.closest && p.closest("#" + HOST_ID)) return NodeFilter.FILTER_REJECT;
        if (!n.nodeValue || !n.nodeValue.trim()) return NodeFilter.FILTER_REJECT;
        return NodeFilter.FILTER_ACCEPT;
      }
    });
    let node;
    while ((node = walker.nextNode())) {
      const text = node.nodeValue;
      const tl = text.toLocaleLowerCase();
      let i = 0;
      while ((i = tl.indexOf(low, i)) >= 0) {
        matches.push({ node, start: i, end: i + term.length });
        i += Math.max(1, term.length);
        if (matches.length >= 4000) break;
      }
      if (matches.length >= 4000) break;
    }
    if (supported && matches.length) {
      main = new Highlight();
      for (const m of matches) main.add(rangeOf(m));
      CSS.highlights.set("orvyn-find", main);
      activeHl = new Highlight();
      CSS.highlights.set("orvyn-find-active", activeHl);
    }
    if (matches.length) { active = 0; show(); }
    else count.textContent = "нет";
  }

  function show() {
    if (active < 0 || active >= matches.length) return;
    const r = rangeOf(matches[active]);
    if (supported && activeHl) {
      activeHl.clear();
      activeHl.add(r);
    }
    const el = matches[active].node.parentElement;
    if (el) {
      try { el.scrollIntoView({ block: "center", inline: "nearest" }); } catch (e) {}
    }
    count.textContent = (active + 1) + "/" + matches.length;
  }

  function step(d) {
    if (!matches.length) return;
    active = (active + d + matches.length) % matches.length;
    show();
  }

  function close() {
    clearMarks();
    host.remove();
    const st = document.getElementById(STYLE_ID);
    if (st) st.remove();
  }

  q.addEventListener("input", () => collect(q.value));
  q.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      step(e.shiftKey ? -1 : 1);
    } else if (e.key === "Escape") {
      e.preventDefault();
      close();
    }
    e.stopPropagation();
  });
  root.getElementById("prev").onclick = () => step(-1);
  root.getElementById("next").onclick = () => step(1);
  root.getElementById("close").onclick = close;
  q.focus();
  q.select();
})();
