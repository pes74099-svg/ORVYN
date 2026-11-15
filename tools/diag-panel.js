// Диагностика панели настроек через CDP.
const PORT = process.env.ORVYN_CDP_PORT || "9222";
const BASE = `http://127.0.0.1:${PORT}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function targets() {
  const r = await fetch(`${BASE}/json/list`);
  return r.json();
}

function connect(url) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    let id = 0;
    const pending = new Map();
    ws.onmessage = (ev) => {
      const m = JSON.parse(ev.data);
      if (m.id && pending.has(m.id)) {
        pending.get(m.id)(m);
        pending.delete(m.id);
      }
    };
    ws.onerror = reject;
    ws.onopen = () => resolve({
      send(method, params) {
        return new Promise((res) => {
          const n = ++id;
          pending.set(n, res);
          ws.send(JSON.stringify({ id: n, method, params: params || {} }));
        });
      },
      close() { ws.close(); },
    });
  });
}

async function evalOn(tab, expression) {
  const c = await connect(tab.webSocketDebuggerUrl);
  const r = await c.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true, userGesture: true });
  c.close();
  if (r.result && r.result.exceptionDetails) throw new Error(r.result.exceptionDetails.text || "JS error");
  return r.result && r.result.result ? r.result.result.value : undefined;
}

(async () => {
  const all = await targets();
  console.log("targets:", all.map((t) => `${t.type}:${t.title}`).join(" | "));
  let ui = null;
  for (const t of all) {
    if (t.type !== "page") continue;
    try {
      if ((await evalOn(t, "!!document.getElementById('panel')")) === true) { ui = t; break; }
    } catch (e) {}
  }
  if (!ui) { console.log("UI not found"); process.exit(1); }
  console.log("ui:", ui.title);
  const before = await evalOn(ui, "JSON.stringify({has:document.body.classList.contains('haspanel'),len:(document.getElementById('panel')||{}).innerHTML.length||0,chips:document.querySelectorAll('[data-setchip]').length})");
  console.log("before:", before);
  await evalOn(ui, "window.chrome.webview.postMessage(JSON.stringify({cmd:'panel',name:'settings'})); true");
  await sleep(800);
  const after = await evalOn(ui, "JSON.stringify({has:document.body.classList.contains('haspanel'),len:(document.getElementById('panel')||{}).innerHTML.length||0,chips:document.querySelectorAll('[data-setchip]').length,secs:Array.from(document.querySelectorAll('.setsec')).map(x=>x.dataset.sec)})");
  console.log("after:", after);
  process.exit(0);
})().catch((e) => { console.error(e.message); process.exit(1); });
