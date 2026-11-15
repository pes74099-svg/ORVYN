// ORVYN smoke test: вкладки, навигация, закрытие, панели, пресет оформления.
//
// Требует запущенный ORVYN с открытым CDP:
//   set ORVYN_CDP_PORT=9222
//   orvyn.exe
//   node tools/smoke-test.js
//
// Ничего не устанавливает и не требует платных сервисов.

const PORT = process.env.ORVYN_CDP_PORT || "9222";
const BASE = `http://127.0.0.1:${PORT}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function targets() {
  const r = await fetch(`${BASE}/json/list`);
  if (!r.ok) throw new Error(`CDP ${r.status}`);
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
    ws.onopen = () =>
      resolve({
        send(method, params) {
          return new Promise((res) => {
            const n = ++id;
            pending.set(n, res);
            ws.send(JSON.stringify({ id: n, method, params: params || {} }));
          });
        },
        close() {
          ws.close();
        },
      });
  });
}

async function evalOn(tab, expression, awaitPromise = false) {
  const c = await connect(tab.webSocketDebuggerUrl);
  const r = await c.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise,
    userGesture: true,
  });
  c.close();
  if (r.result && r.result.exceptionDetails) {
    throw new Error(r.result.exceptionDetails.text || "JS error");
  }
  return r.result && r.result.result ? r.result.result.value : undefined;
}

async function findUi() {
  for (const t of await targets()) {
    if (t.type !== "page") continue;
    try {
      if ((await evalOn(t, "!!document.getElementById('panel')")) === true) return t;
    } catch (e) {
      // не UI — пропускаем
    }
  }
  return null;
}

function uiCmd(ui, payload) {
  const json = JSON.stringify(payload);
  return evalOn(ui, `window.chrome.webview.postMessage(${JSON.stringify(json)}); true`);
}

(async () => {
  const ui = await findUi();
  if (!ui) throw new Error("UI не найден. Запусти ORVYN с ORVYN_CDP_PORT=9222");

  const count = () => evalOn(ui, "document.querySelectorAll('.tab').length");
  const activeText = () =>
    evalOn(ui, "(document.querySelector('.tab.active') || {}).innerText || ''");
  const checks = [];
  const ok = (name, cond, detail) => {
    checks.push({ name, pass: !!cond, detail });
    if (!cond) throw new Error(`${name}: ${detail || "fail"}`);
  };

  const baseline = await count();
  ok("есть вкладки", baseline > 0, `вкладок: ${baseline}`);

  await uiCmd(ui, { cmd: "newTab" });
  await sleep(900);
  ok("новая вкладка", (await count()) === baseline + 1, `стало: ${await count()}`);

  await uiCmd(ui, { cmd: "navigate", text: "about:blank" });
  let loaded = false;
  let seen = "";
  for (let i = 0; i < 10; i++) {
    await sleep(400);
    seen = await activeText();
    if (seen.toLowerCase().includes("blank")) {
      loaded = true;
      break;
    }
  }
  ok("навигация", loaded, loaded ? seen.replace(/\s+/g, " ").trim() : "страница не переключилась");

  const tabId = await evalOn(
    ui,
    "Number((document.querySelector('.tab.active') || {}).dataset?.tabId || 0)"
  );
  ok("id вкладки", tabId > 0, `id=${tabId}`);
  await uiCmd(ui, { cmd: "closeTab", id: tabId });
  await sleep(900);
  ok("закрытие вкладки", (await count()) === baseline, `вкладок: ${await count()}`);

  await uiCmd(ui, { cmd: "panel", name: "settings" });
  await sleep(600);
  const panelText = await evalOn(ui, "document.body.innerText");
  ok("панель настроек", panelText.toUpperCase().includes("НАСТРОЙК"), "заголовок найден");

  const chips = await evalOn(ui, "document.querySelectorAll('[data-setchip]').length");
  ok("разделы настроек", chips >= 10, `чипсов: ${chips}`);
  await evalOn(ui, "document.querySelector('[data-setchip=privacy]').click(); true");
  await sleep(300);
  const visibleSections = await evalOn(
    ui,
    "JSON.stringify(Array.from(document.querySelectorAll('.setsec')).filter((x) => getComputedStyle(x).display !== 'none').map((x) => x.dataset.sec))"
  );
  ok("фильтр по разделу", visibleSections === '["privacy"]', visibleSections);
  const resetVisible = await evalOn(
    ui,
    "getComputedStyle(document.getElementById('setresetrow')).display !== 'none'"
  );
  ok("кнопка сброса раздела", resetVisible === true, "видна в выбранном разделе");

  const advToggle = await evalOn(ui, "!!document.getElementById('setadv')");
  ok("переключатель расширенных", advToggle === true, "есть");
  await evalOn(ui, "document.querySelector('[data-setchip=all]').click(); true");
  await sleep(200);
  await evalOn(ui, "document.querySelector('[data-setchip=memory]').click(); true");
  await sleep(200);
  const hiddenAdv = await evalOn(
    ui,
    "Array.from(document.querySelectorAll('.setsec[data-sec=memory] .item[data-adv]')).filter((x) => getComputedStyle(x).display !== 'none').length"
  );
  ok("расширенные скрыты", hiddenAdv === 0, `видимых: ${hiddenAdv}`);
  await evalOn(ui, "(()=>{const b=document.getElementById('setadv'); b.checked=true; b.onchange(); return true;})()");
  await sleep(200);
  const shownAdv = await evalOn(
    ui,
    "Array.from(document.querySelectorAll('.setsec[data-sec=memory] .item[data-adv]')).filter((x) => getComputedStyle(x).display !== 'none').length"
  );
  ok("расширенные включаются", shownAdv === 3, `видимых: ${shownAdv}`);
  await evalOn(ui, "(()=>{const b=document.getElementById('setadv'); b.checked=false; b.onchange(); return true;})()");
  await sleep(150);
  await uiCmd(ui, { cmd: "panel", name: "" });
  await sleep(300);

  await uiCmd(ui, { cmd: "panel", name: "custom" });
  await sleep(600);
  const presets = await evalOn(ui, "document.querySelectorAll('[data-preset]').length");
  ok("пресеты", presets === 10, `кнопок: ${presets}`);

  const beforeAccent = await evalOn(
    ui,
    "getComputedStyle(document.body).getPropertyValue('--accent').trim()"
  );
  await evalOn(ui, "document.querySelector('[data-preset=cyberpunk]').click(); true");
  await sleep(800);
  const cyberAccent = await evalOn(
    ui,
    "getComputedStyle(document.body).getPropertyValue('--accent').trim()"
  );
  ok("пресет применяется", cyberAccent && cyberAccent !== beforeAccent, cyberAccent);
  await uiCmd(ui, { cmd: "themePreset", id: "custom" });
  await sleep(700);
  const backAccent = await evalOn(
    ui,
    "getComputedStyle(document.body).getPropertyValue('--accent').trim()"
  );
  ok("пресет откатывается", backAccent === beforeAccent, `accent: ${backAccent}`);

  await uiCmd(ui, { cmd: "panel", name: "" });
  for (const c of checks) console.log(`PASS ${c.name}${c.detail ? ` (${c.detail})` : ""}`);
  console.log("SMOKE OK");
  process.exit(0);
})().catch(async (e) => {
  console.error("SMOKE FAIL:", e.message);
  process.exit(1);
});
