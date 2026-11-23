// Logiktest für src/lib/format.ts: Formatierung und keine HTML-Injection über Platzhalter.
// Aufruf aus dem Repo-Wurzelverzeichnis: node scripts/test-format.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";

const root = new URL("..", import.meta.url).pathname;
const ts = readFileSync(`${root}src/lib/format.ts`, "utf8");
const js = stripTypeScriptTypes(ts, { mode: "strip" });
const { formatText } = await import(`data:text/javascript,${encodeURIComponent(js)}`);

const emoji = (name) => (name === "party" ? "avatar://localhost/a/emoji/party" : null);
const link = (url, label = url) => `<a href="${url}" target="_blank" rel="noreferrer">${label}</a>`;

assert.equal(formatText("siehe https://x.de/a."), `siehe ${link("https://x.de/a")}.`);
// Markdown-Link: Ziel als Tooltip, damit ein irreführender Text („[https://bank.de](https://evil)“) auffällt.
assert.equal(
  formatText("[Doku](https://x.de/d)"),
  '<a href="https://x.de/d" title="https://x.de/d" target="_blank" rel="noreferrer">Doku</a>',
);
assert.equal(formatText("**fett** _kursiv_ `a<b`"), "<strong>fett</strong> <em>kursiv</em> <code>a&lt;b</code>");
assert.equal(formatText("hi @anna\nzwei"), 'hi <span class="mention">@anna</span><br>zwei');
assert.equal(
  formatText(":party: _https://x.de_", emoji),
  `<img class="emoji" src="avatar://localhost/a/emoji/party" alt=":party:" title=":party:"> <em>${link("https://x.de")}</em>`,
);

// Kein fertiges HTML (Link, Code, Emoji) darf in einem href landen.
const attacks = [
  "https://phish.example/[Klick](https://x/style=position:fixed;inset:0;opacity:0;)",
  "https://phish.example/`code`",
  "https://phish.example/:party:",
  "https://phish.example/\u00000\u0000[a](https://x)",
];
for (const text of attacks) {
  const html = formatText(text, emoji);
  for (const [, href] of html.matchAll(/href="([^"]*)"/g)) {
    assert.ok(!/[<>\u0000]/.test(href), `HTML im href: ${html}`);
  }
  assert.ok(!/\sstyle=/.test(html), `style-Attribut eingeschleust: ${html}`);
  assert.ok(!html.includes("undefined"), `gefälschter Platzhalter: ${html}`);
}

// Fuzzing mit festem Seed: jedes erzeugte Tag muss auf der Whitelist stehen (Pentest 2026-11-23).
const allowed = [
  /^<\/?(strong|em|del|code|pre|br)>$/, /^<\/a>$/, /^<span class="mention">$/, /^<\/span>$/,
  /^<a href="https?:\/\/[^"<>]*"( title="https?:\/\/[^"<>]*")? target="_blank" rel="noreferrer">$/,
  /^<img class="emoji" src="avatar:\/\/localhost\/a\/emoji\/party" alt=":[a-z0-9_+-]+:" title=":[a-z0-9_+-]+:">$/,
];
const alpha = ["https://", "http://x.de/", "[", "]", "(", ")", "`", "```", "*", "**", "_", "~", "~~", "@a", ":party:",
  ":x:", '"', "'", "<", ">", "&", " ", "\n", "x", "\u0000", "0", "/", ".", "javascript:", "=", "#", "%22", "\u202e"];
let seed = 42;
const rnd = (n) => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff) % n);
for (let i = 0; i < 50_000; i++) {
  let text = "";
  for (let k = 1 + rnd(12); k > 0; k--) text += alpha[rnd(alpha.length)];
  const html = formatText(text, emoji);
  for (const tag of html.match(/<[^>]*>?/g) ?? []) {
    assert.ok(allowed.some((r) => r.test(tag)), `unerlaubtes Tag ${tag} aus ${JSON.stringify(text)}`);
  }
}

console.log("format: ok");
