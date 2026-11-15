// Печатает заголовок и начало текста для каждой вкладки (кроме интерфейса).
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
  const r = await c.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  c.close();
  if (r.result && r.result.exceptionDetails) return "JS error: " + (r.result.exceptionDetails.text || "");
  return r.result && r.result.result ? r.result.result.value : undefined;
}

(async () => {
  const list = await (await fetch(`${BASE}/json/list`)).json();
  for (const t of list) {
    if (t.type !== "page") continue;
    if ((await evalOn(t, "!!document.getElementById('panel')")) === true) continue;
    const info = await evalOn(t, "JSON.stringify({h:location.href,t:document.title,bg:getComputedStyle(document.body).backgroundColor,n:document.body?document.body.innerText.replace(/\\s+/g,' ').slice(0,150):'',len:document.body?document.body.innerHTML.length:0})");
    console.log(t.title || "(no title)", "=>", info);
  }
})();
