<script lang="ts">
  import { save } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, type Attachment, type Message } from "$lib/api";
  import { chat } from "$lib/chat.svelte";
  import { colorFor, formatText, initials, QUICK_EMOJI, size, time } from "$lib/format";

  let {
    message: m,
    compact = false,
    inThread = false,
  }: { message: Message; compact?: boolean; inThread?: boolean } = $props();

  let editing = $state(false);
  let draft = $state("");
  let picker = $state(false);

  export function startEdit() {
    draft = m.text;
    editing = true;
  }

  async function saveEdit() {
    const text = draft.trim();
    editing = false;
    if (text && text !== m.text) await chat.edit(m, text);
  }

  function editKey(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      saveEdit();
    } else if (e.key === "Escape") editing = false;
  }

  async function del() {
    if (confirm("Nachricht wirklich löschen?")) await chat.remove(m);
  }

  async function download(a: Attachment) {
    if (!chat.account) return;
    if (a.url && !a.id) return openUrl(a.url);
    const dest = await save({ defaultPath: a.name });
    if (!dest) return;
    try {
      await api.download(chat.account, a, dest);
      chat.toast(`${a.name} gespeichert`);
    } catch (e) {
      if (a.url) openUrl(a.url);
      else chat.toast(String(e), "error");
    }
  }

  function linkClick(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest("a");
    if (a?.href) {
      e.preventDefault();
      openUrl(a.href);
    }
  }
</script>

<div class="msg" class:compact class:mentioned={m.mentions_me && !m.mine}>
  <div class="gutter">
    {#if compact}
      <span class="hover-time">{time(m.created)}</span>
    {:else if m.sender.avatar_url}
      <img class="avatar" src={m.sender.avatar_url} alt="" loading="lazy" />
    {:else}
      <div class="avatar" style:background={colorFor(m.sender.name)}>{initials(m.sender.name)}</div>
    {/if}
  </div>
  <div class="body">
    {#if !compact}
      <div class="head">
        <span class="sender">{m.sender.name}</span>
        <span class="time">{time(m.created)}</span>
      </div>
    {/if}
    {#if editing}
      <!-- svelte-ignore a11y_autofocus -->
      <textarea class="edit" bind:value={draft} onkeydown={editKey} autofocus rows="3"></textarea>
      <div class="edit-hint">Enter speichert · Esc bricht ab</div>
    {:else}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="text" onclick={linkClick}>
        {@html formatText(m.text)}{#if m.edited}<span class="edited"> (bearbeitet)</span>{/if}
      </div>
    {/if}
    {#if m.attachments.length}
      <div class="files">
        {#each m.attachments as a (a.id + a.name)}
          <button class="file" onclick={() => download(a)} title="Herunterladen">
            <span class="file-icon">📄</span>
            <span class="file-name">{a.name}</span>
            <span class="file-size">{size(a.size)}</span>
          </button>
        {/each}
      </div>
    {/if}
    {#if m.reactions.length}
      <div class="reactions">
        {#each m.reactions as r (r.emoji)}
          <button class="reaction" class:me={r.me} onclick={() => chat.toggleReaction(m, r.emoji)}>
            {r.emoji} <span>{r.count}</span>
          </button>
        {/each}
      </div>
    {/if}
    {#if !inThread && m.reply_count > 0}
      <button class="replies" onclick={() => chat.openThread(m)}>
        {m.reply_count} {m.reply_count === 1 ? "Antwort" : "Antworten"}
      </button>
    {/if}
  </div>
  {#if !editing}
    <div class="actions">
      <button class="icon-btn" title="Reaktion" onclick={() => (picker = !picker)}>☺</button>
      {#if !inThread && m.thread_id}
        <button class="icon-btn" title="Antworten" onclick={() => chat.openThread(m)}>↩</button>
      {/if}
      {#if m.mine}
        <button class="icon-btn" title="Bearbeiten" onclick={startEdit}>✎</button>
        <button class="icon-btn" title="Löschen" onclick={del}>🗑</button>
      {/if}
    </div>
  {/if}
  {#if picker}
    <div class="picker">
      {#each QUICK_EMOJI as e (e)}
        <button
          onclick={() => {
            picker = false;
            chat.toggleReaction(m, e);
          }}>{e}</button
        >
      {/each}
    </div>
  {/if}
</div>

<style>
  .msg {
    position: relative;
    display: flex;
    gap: 8px;
    padding: 8px 20px 4px;
    content-visibility: auto;
    contain-intrinsic-size: auto 48px;
  }
  .msg.compact {
    padding-top: 2px;
  }
  .msg:hover {
    background: var(--hover);
  }
  .msg.mentioned {
    background: var(--mention-highlight);
  }
  .gutter {
    width: 36px;
    flex-shrink: 0;
  }
  .avatar {
    width: 36px;
    height: 36px;
    border-radius: var(--avatar-radius);
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 13px;
    font-weight: 700;
    object-fit: cover;
  }
  .hover-time {
    visibility: hidden;
    font-size: 10px;
    color: var(--text-muted);
    line-height: 22px;
  }
  .msg:hover .hover-time {
    visibility: visible;
  }
  .body {
    flex: 1;
    min-width: 0;
  }
  .head {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .sender {
    font-weight: 700;
  }
  .time {
    font-size: 12px;
    color: var(--text-muted);
  }
  .text {
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
  .edited {
    font-size: 12px;
    color: var(--text-muted);
  }
  .edit {
    width: 100%;
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    padding: 6px 8px;
    resize: vertical;
  }
  .edit-hint {
    font-size: 11px;
    color: var(--text-muted);
  }
  .files {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 4px;
  }
  .file {
    display: flex;
    align-items: center;
    gap: 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--center-bg);
    padding: 6px 10px;
    max-width: 320px;
  }
  .file-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .file-size {
    color: var(--text-muted);
    font-size: 12px;
  }
  .reactions {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 4px;
  }
  .reaction {
    border: 1px solid var(--border);
    background: var(--center-bg);
    border-radius: 12px;
    padding: 1px 8px;
    font-size: 13px;
  }
  .reaction.me {
    border-color: var(--link);
    background: color-mix(in srgb, var(--link) 10%, transparent);
  }
  .replies {
    border: none;
    background: none;
    color: var(--link);
    font-weight: 600;
    padding: 2px 0;
    font-size: 13px;
  }
  .actions {
    position: absolute;
    top: -12px;
    right: 16px;
    display: none;
    background: var(--center-bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.1);
  }
  .msg:hover .actions {
    display: flex;
  }
  .picker {
    position: absolute;
    top: 18px;
    right: 16px;
    z-index: 5;
    display: grid;
    grid-template-columns: repeat(6, 32px);
    background: var(--center-bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
    padding: 4px;
  }
  .picker button {
    border: none;
    background: none;
    font-size: 18px;
    padding: 4px;
    border-radius: 4px;
  }
  .picker button:hover {
    background: var(--hover);
  }
</style>
