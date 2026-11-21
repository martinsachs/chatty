// Logiktest für den UI-Zustand (src/lib/chat.svelte.ts) ohne Fenster: Ladevorgänge, Space-Wechsel, Nachladen.
// Aufruf aus dem Repo-Wurzelverzeichnis: node scripts/test-chat-store.mjs
// Übersetzt die Datei mit dem Svelte-Compiler nach src-tauri/target/ und ersetzt die Tauri-API durch Attrappen.
import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { pathToFileURL } from "node:url";
import { compileModule } from "svelte/compiler";

const root = new URL("..", import.meta.url).pathname;
const outDir = `${root}src-tauri/target/chat-store-test`;
mkdirSync(outDir, { recursive: true });
const ts = readFileSync(`${root}src/lib/chat.svelte.ts`, "utf8");
let code = compileModule(stripTypeScriptTypes(ts, { mode: "strip" }), { generate: "client", filename: "chat.svelte.js" }).js.code;
code = code
  .replace(/import \{ listen \} from "@tauri-apps\/api\/event";/, "const listen = globalThis.__listen;")
  .replace(/import \{[^}]*\} from "\.\/api";/, "const { api, emojiSrc } = globalThis.__api;");
writeFileSync(`${outDir}/chat.mjs`, code);
globalThis.document = { hasFocus: () => true };
const calls = [];
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const msg = (space, n) => ({ id: `${space}-${n}`, space_id: space, created: n, is_reply: false, mine: false,
  reactions: [], attachments: [], sender: { id: "u", name: "u" }, text: "", reply_count: 0, thread_id: null });
let delay = { A: 30, B: 5 };
let server = { A: [msg("A", 1), msg("A", 2)], B: [msg("B", 1)] };
globalThis.__listen = async () => () => {};
globalThis.__api = {
  emojiSrc: () => "",
  api: {
    setActive: async (a, s) => calls.push(["setActive", a, s]),
    cachedMessages: async () => [],
    markRead: async () => {},
    cachedSpaces: async () => [],
    listSpaces: async () => [{ id: "A" }, { id: "B" }],
    customEmojis: async () => [],
    listMessages: async (a, s, cursor) => {
      await wait(delay[s] ?? 1);
      if (cursor) return { messages: [msg(s, -1)], next_cursor: null };
      return { messages: server[s], next_cursor: "c" };
    },
  },
};
const { chat } = await import(pathToFileURL(`${outDir}/chat.mjs`).href);
chat.spaces = { acc: [{ id: "A" }, { id: "B" }] };

// K1: A (langsam) → B (schnell): nur B bleibt.
const pa = chat.openSpace("acc", "A");
const pb = chat.openSpace("acc", "B");
await Promise.all([pa, pb]);
assert.deepEqual(chat.messages.map((m) => m.id), ["B-1"], "A→B: nur B");
if (chat.loadingInitial !== undefined) assert.equal(chat.loadingInitial, false);

// K1: A→B→A: nur A, keine Dopplung.
await Promise.all([chat.openSpace("acc", "A"), chat.openSpace("acc", "B"), chat.openSpace("acc", "A")]);
assert.deepEqual(chat.messages.map((m) => m.id), ["A-1", "A-2"], "A→B→A");

// K1: loadOlder in A läuft, Wechsel zu B → keine A-Nachrichten in B.
delay = { A: 30, B: 1 };
const older = chat.loadOlder();
await chat.openSpace("acc", "B");
await older;
assert.deepEqual(chat.messages.map((m) => m.id), ["B-1"], "loadOlder verworfen");
if (chat.loadingOlder !== undefined) assert.equal(chat.loadingOlder, false);

// K2: catchUp ohne Überlappung ersetzt den Verlauf und setzt den Cursor.
chat.accounts = [{ id: "acc", online: true, label: "x", kind: "mattermost" }];
server.B = [msg("B", 100), msg("B", 101)];
chat["onEvent"]?.call(chat, { account: "acc", event: { type: "status", online: true } });
await wait(20);
assert.deepEqual(chat.messages.map((m) => m.id), ["B-100", "B-101"], "Lücke → ersetzt");
assert.equal(chat.cursor, "c");

// K2: catchUp mit Überlappung mischt sortiert ein.
server.B = [msg("B", 101), msg("B", 102)];
chat["onEvent"]?.call(chat, { account: "acc", event: { type: "status", online: true } });
await wait(20);
assert.deepEqual(chat.messages.map((m) => m.id), ["B-100", "B-101", "B-102"], "Überlappung → gemischt");

// Mattermost: Thread-Antwort steht im Hauptverlauf, Antwortzähler der Startnachricht steigt einmal.
const reply = { ...msg("B", 103), is_reply: true, thread_id: "B-101" };
chat.messages[1].thread_id = "B-101";
chat["onEvent"]?.call(chat, { account: "acc", event: { type: "message_created", message: reply } });
chat["onEvent"]?.call(chat, { account: "acc", event: { type: "message_created", message: reply } });
assert.ok(chat.messages.some((m) => m.id === "B-103" && m.is_reply), "Antwort im Hauptverlauf");
assert.equal(chat.messages.filter((m) => m.id === "B-103").length, 1, "Antwort nur einmal");
assert.equal(chat.messages.find((m) => m.id === "B-101").reply_count, 1, "Zähler einmal erhöht");

// S2 (ADR 05): unten angekommen → nur die neuesten `max` bleiben, Cursor = älteste verbliebene.
chat.messages = [msg("B", 1), msg("B", 2), msg("B", 3), msg("B", 4)];
chat.trimToWindow(3);
assert.deepEqual(chat.messages.map((m) => m.id), ["B-2", "B-3", "B-4"], "gekürzt");
assert.equal(chat.cursor, "B-2");
chat.accounts = [{ id: "acc", online: true, label: "x", kind: "google" }];
chat.trimToWindow(1);
assert.equal(chat.messages.length, 3, "Google: nicht kürzen");

// K5: Kontowechsel meldet alten Space inaktiv.
calls.length = 0;
await chat.selectAccount("acc2");
assert.ok(calls.some((c) => c[0] === "setActive" && c[1] === "acc" && c[2] === null), "setActive(alt, null)");
console.log("ok: alle Prüfungen");
