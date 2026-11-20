<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { chat } from "$lib/chat.svelte";

  let {
    placeholder,
    thread = null,
    onEditLast,
  }: { placeholder: string; thread?: string | null; onEditLast?: () => void } = $props();

  let text = $state("");
  let files = $state<string[]>([]);
  let sending = $state(false);
  let area: HTMLTextAreaElement;

  async function pick() {
    const picked = await open({ multiple: true });
    if (picked) files = [...files, ...(Array.isArray(picked) ? picked : [picked])];
  }

  async function submit() {
    const t = text.trim();
    if ((!t && !files.length) || sending) return;
    sending = true;
    const f = files;
    text = "";
    files = [];
    try {
      await chat.send(t, f, thread);
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

  function grow() {
    area.style.height = "auto";
    area.style.height = Math.min(area.scrollHeight, 240) + "px";
  }
</script>

<div class="composer">
  {#if files.length}
    <div class="chips">
      {#each files as f, i (f)}
        <span class="chip">
          {f.split(/[\\/]/).pop()}
          <button class="icon-btn" title="Entfernen" onclick={() => (files = files.filter((_, j) => j !== i))}>×</button>
        </span>
      {/each}
    </div>
  {/if}
  <div class="row">
    <textarea
      bind:this={area}
      bind:value={text}
      {placeholder}
      rows="1"
      onkeydown={key}
      oninput={grow}
      disabled={!chat.currentAccount?.online}
    ></textarea>
  </div>
  <div class="tools">
    <button class="icon-btn" title="Datei anhängen" onclick={pick}>📎</button>
    <span class="hint">Enter senden · Shift+Enter neue Zeile{onEditLast ? " · ↑ letzte bearbeiten" : ""}</span>
    <button class="btn send" onclick={submit} disabled={sending || (!text.trim() && !files.length)}>Senden</button>
  </div>
</div>

<style>
  .composer {
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
  .chip {
    background: var(--hover);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 2px 4px 2px 8px;
    font-size: 12px;
  }
</style>
