// Выполняет JS в интерфейсе ORVYN (а не в открытой странице).
//
//   node tools/ui-eval.js "document.title"
//   node tools/ui-eval.js "(async()=>{ ... })()"
//
// Нужен запущенный ORVYN с ORVYN_CDP_PORT=9222.
const PORT = process.env.ORVYN_CDP_PORT || "9222";
const BASE = `http://127.0.0.1:${PORT}`;

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
  const r = await c.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
    userGesture: true,
  });
  c.close();
  if (r.result && r.result.exceptionDetails) {
    throw new Error(r.result.exceptionDetails.exception?.description || r.result.exceptionDetails.text || "JS error");
  }
  return r.result && r.result.result ? r.result.result.value : undefined;
}

(async () => {
  const expression = process.argv[2];
  if (!expression) {
    console.error("нужно передать JS-выражение строкой");
    process.exit(1);
  }
  const list = await (await fetch(`${BASE}/json/list`)).json();
  for (const t of list) {
    if (t.type !== "page") continue;
    try {
      if ((await evalOn(t, "!!document.getElementById('panel')")) === true) {
        const value = await evalOn(t, expression);
        console.log(typeof value === "string" ? value : JSON.stringify(value));
        return;
      }
    } catch (e) {
      // не интерфейс — идём дальше
    }
  }
  console.error("интерфейс ORVYN не найден; запусти браузер с ORVYN_CDP_PORT=9222");
  process.exit(1);
})();
