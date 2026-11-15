// Проверка синтаксиса JS из ui.html без запуска браузера.
const fs = require("fs");
const path = require("path");

const target = process.argv[2] || path.resolve(__dirname, "..", "rustbrowser", "assets", "ui.html");
const html = fs.readFileSync(target, "utf8");
const blocks = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)].map((m) => m[1]);
if (!blocks.length) {
  console.error("не найден <script>");
  process.exit(1);
}
for (const [i, code] of blocks.entries()) {
  try {
    new Function(code);
  } catch (e) {
    console.error(`SCRIPT ${i + 1}: ${e.message}`);
    process.exit(1);
  }
}
console.log(`JS OK (${blocks.length} блоков)`);
