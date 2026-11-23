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
assert.equal(formatText("[Doku](https://x.de/d)"), link("https://x.de/d", "Doku"));
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

console.log("format: ok");
