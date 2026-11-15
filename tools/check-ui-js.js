// Проверяет синтаксис JS в ui.html (и newtab.html, если файл есть рядом).
// Шаблонные вставки вида {{SNOW}} заменяются числом, чтобы код оставался валидным.
const fs = require("fs");
const path = require("path");

const base = path.resolve(__dirname, "..", "rustbrowser", "assets");

function check(file) {
  const html = fs.readFileSync(file, "utf8").replace(/\{\{[A-Z0-9_]+\}\}/g, "1");
  const blocks = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)].map((m) => m[1]);
  if (!blocks.length) {
    console.error(`нет блока <script> в ${file}`);
    process.exit(1);
  }
  for (const [i, code] of blocks.entries()) {
    try {
      new Function(code);
    } catch (e) {
      console.error(`${path.basename(file)} SCRIPT ${i + 1}: ${e.message}`);
      process.exit(1);
    }
  }
  return blocks.length;
}

if (process.argv[2]) {
  const n = check(process.argv[2]);
  console.log(`JS OK (${n} блоков)`);
  process.exit(0);
}

let total = 0;
for (const name of ["ui.html", "newtab.html"]) {
  const file = path.join(base, name);
  if (!fs.existsSync(file)) continue;
  total += check(file);
}
console.log(`JS OK (${total} блоков: ui.html + newtab.html)`);
