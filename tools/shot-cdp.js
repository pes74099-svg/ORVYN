// Скриншот интерфейса ORVYN через CDP (надёжнее PrintWindow для WebView2).
//
//   node tools/shot-cdp.js C:\path\out.png
const fs = require("fs");
const PORT = process.env.ORVYN_CDP_PORT || "9222";
const BASE = `http://127.0.0.1:${PORT}`;

function connect(url) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    let id = 0;
    const pending = new Map();
    ws.onmessage = (ev) => {
      const m = JSON.parse(ev.data);
      if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
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
  const r = await c.send("Runtime.evaluate", { expression, returnByValue: true });
  c.close();
  return r.result && r.result.result ? r.result.result.value : undefined;
}

(async () => {
  const out = process.argv[2];
  if (!out) { console.error("укажи путь к PNG"); process.exit(1); }
  const list = await (await fetch(`${BASE}/json/list`)).json();
  for (const t of list) {
    if (t.type !== "page") continue;
    if ((await evalOn(t, "!!document.getElementById('panel')")) !== true) continue;
    const c = await connect(t.webSocketDebuggerUrl);
    await c.send("Page.enable");
    const shot = await c.send("Page.captureScreenshot", { format: "png" });
    c.close();
    const data = shot.result && shot.result.data;
    if (!data) { console.error("нет данных скриншота"); process.exit(1); }
    fs.writeFileSync(out, Buffer.from(data, "base64"));
    console.log("SAVED", out);
    return;
  }
  console.error("интерфейс ORVYN не найден");
  process.exit(1);
})();
