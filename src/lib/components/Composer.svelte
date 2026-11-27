<script lang="ts">
  import { api, pickedSrc, type PickedFile } from "$lib/api";
  import { chat } from "$lib/chat.svelte";
  import { formatText } from "$lib/format";
  import EmojiPicker from "./EmojiPicker.svelte";

  let {
    placeholder,
    thread = null,
    onEditLast,
  }: { placeholder: string; thread?: string | null; onEditLast?: () => void } = $props();

  let text = $state("");
  let files = $state<PickedFile[]>([]);
  let sending = $state(false);
  let preview = $state(false);
  let emojiOpen = $state(false);
  // Dateien ohne Vorschaubild (kein Bild, zu groß): nur Name zeigen.
  let noThumb = $state<Record<number, boolean>>({});
  let area: HTMLTextAreaElement;
  const emojiUrl = (n: string) => chat.emojiUrl(chat.account, n);
  const isImage = (name: string) => /\.(png|jpe?g|gif|webp)$/i.test(name);

  // Bild aus der Zwischenablage (Bildschirmfoto) als Anhang. Enthält sie auch Text (z. B. Tabellenzellen),
  // gewinnt der Text.
  async function paste(e: ClipboardEvent) {
    const data = e.clipboardData;
    if (!data || data.types.includes("text/plain")) return;
    const images = [...data.files].filter((f) => f.type.startsWith("image/"));
    if (!images.length) return;
    e.preventDefault();
    for (const f of images) {
      const picked = await chat.guard(api.pasteImage(new Uint8Array(await f.arrayBuffer())));
      if (picked) files = [...files, picked];
    }
  }

  async function pick() {
    const picked = await chat.guard(api.pickFiles());
    if (picked?.length) files = [...files, ...picked];
  }

  async function submit() {
    const t = text.trim();
    if ((!t && !files.length) || sending) return;
    sending = true;
    try {
      // Text und Dateien erst nach Erfolg leeren, damit ein Fehlschlag nichts verliert.
      if (await chat.send(t, files.map((f) => f.id), thread)) {
        text = "";
        files = [];
      }
    } finally {
      sending = false;
      area?.focus();
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      submit();
    } else if (e.key === "ArrowUp" && !text && onEditLast) {
      e.preventDefault();
      onEditLast();
    }
  }

  function insertEmoji(emoji: string) {
    emojiOpen = false;
    const start = area?.selectionStart ?? text.length;
    const end = area?.selectionEnd ?? text.length;
    text = text.slice(0, start) + emoji + text.slice(end);
    const cursor = start + emoji.length;
    requestAnimationFrame(() => {
      area.focus();
      area.setSelectionRange(cursor, cursor);
      grow();
    });
  }

  function grow() {
    area.style.height = "auto";
    area.style.height = Math.min(area.scrollHeight, 240) + "px";
  }
</script>

<div class="composer">
  {#if emojiOpen}
    <EmojiPicker onPick={insertEmoji} onClose={() => (emojiOpen = false)} />
  {/if}
  {#if files.length}
    <div class="chips">
      {#each files as f, i (f.id)}
        <span class="chip">
          {#if isImage(f.name) && !noThumb[f.id]}
            <img src={pickedSrc(f.id)} alt="" onerror={() => (noThumb[f.id] = true)} />
          {/if}
          {f.name}
          <button class="icon-btn" title="Entfernen" onclick={() => (files = files.filter((_, j) => j !== i))}>×</button>
        </span>
      {/each}
    </div>
  {/if}
  {#if preview && text.trim()}
    <div class="preview">{@html formatText(text, emojiUrl)}</div>
  {/if}
  <div class="row">
    <textarea
      bind:this={area}
      bind:value={text}
      {placeholder}
      rows="1"
      onkeydown={key}
      oninput={grow}
      onpaste={paste}
      disabled={!chat.currentAccount?.online}
    ></textarea>
  </div>
  <div class="tools">
    <button
      class="icon-btn"
      class:on={emojiOpen}
      title="Emoji einfügen"
      aria-expanded={emojiOpen}
      onclick={(e) => {
        e.stopPropagation();
        emojiOpen = !emojiOpen;
      }}>🙂</button
    >
    <button class="icon-btn" title="Datei anhängen (oder Bild mit Strg+V einfügen)" onclick={pick}>📎</button>
    <button class="icon-btn" class:on={preview} title="Vorschau der Formatierung" onclick={() => (preview = !preview)}
      >👁</button
    >
    <span class="hint">Enter senden · Shift+Enter neue Zeile{onEditLast ? " · ↑ letzte bearbeiten" : ""}</span>
    <button class="btn send" onclick={submit} disabled={sending || (!text.trim() && !files.length)}>Senden</button>
  </div>
</div>

<style>
  .composer {
    position: relative;
    margin: 0 20px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--center-bg);
  }
  .composer:focus-within {
    border-color: var(--text-muted);
  }
  textarea {
    width: 100%;
    border: none;
    resize: none;
    outline: none;
    padding: 10px 12px 4px;
    max-height: 240px;
    background: transparent;
    line-height: 1.4;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px 6px;
  }
  .hint {
    flex: 1;
    font-size: 11px;
    color: var(--text-muted);
  }
  .send {
    padding: 4px 12px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 8px 8px 0;
  }
  .chip img {
    display: block;
    max-width: 160px;
    max-height: 100px;
    margin-bottom: 2px;
    border-radius: var(--radius);
  }
  .preview {
    padding: 8px 12px;
    border-bottom: 1px dashed var(--border);
    line-height: 1.45;
    overflow-wrap: anywhere;
    max-height: 200px;
    overflow-y: auto;
  }
  .on {
    background: var(--hover);
  }
  .chip {
    background: var(--hover);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 2px 4px 2px 8px;
    font-size: 12px;
  }
</style>
