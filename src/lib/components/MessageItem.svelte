<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, avatarSrc, previewSrc, type Attachment, type Message } from "$lib/api";
  import { chat } from "$lib/chat.svelte";
  import { colorFor, emojiOnly, formatText, initials, QUICK_EMOJI, size, time } from "$lib/format";

  let {
    message: m,
    compact = false,
    inThread = false,
  }: { message: Message; compact?: boolean; inThread?: boolean } = $props();

  let editing = $state(false);
  let draft = $state("");
  let picker = $state(false);
  let avatarFailed = $state(false);
  // Anhänge, deren Vorschau nicht lädt (zu groß, Fehler): nur als Download zeigen.
  let previewFailed = $state<Record<string, boolean>>({});
  const emojiUrl = (n: string) => chat.emojiUrl(chat.account, n);

  // Mattermost liefert Profilbilder nur mit Token: über das Backend (avatar://) laden.
  const avatar = $derived(
    m.sender.avatar_url ??
      (chat.accounts.find((a) => a.id === chat.account)?.kind === "mattermost" && chat.account
        ? avatarSrc(chat.account, m.sender.id)
        : null),
  );

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

  async function copy() {
    try {
      await navigator.clipboard.writeText(m.text);
      chat.toast("Text kopiert");
    } catch (e) {
      chat.toast(String(e), "error");
    }
  }

  async function del() {
    if (confirm("Nachricht wirklich löschen?")) await chat.remove(m);
  }

  async function download(a: Attachment) {
    if (!chat.account) return;
    if (a.url && !a.id) return openUrl(a.url);
    try {
      if (await api.saveAttachment(chat.account, a)) chat.toast(`${a.name} gespeichert`);
    } catch (e) {
      if (a.url) openUrl(a.url);
      else chat.toast(String(e), "error");
    }
  }

</script>

<div class="msg" class:compact class:reply={m.is_reply && !inThread} class:mentioned={m.mentions_me && !m.mine} style:--nick={colorFor(m.sender.name)}>
  <div class="gutter">
    {#if compact}
      <span class="hover-time">{time(m.created)}</span>
    {:else if avatar && !avatarFailed}
      <img class="avatar" src={avatar} alt="" loading="lazy" onerror={() => (avatarFailed = true)} />
    {:else}
      <div class="avatar" style:background={colorFor(m.sender.name)}>{initials(m.sender.name)}</div>
    {/if}
  </div>
  <div class="body">
    <!-- Kopf immer im DOM: IRC-/DOS-Optik zeigt Zeit und Absender in jeder Zeile. -->
    <div class="head">
      <span class="sender">{m.sender.name}</span>
      <span class="time">{time(m.created)}</span>
    </div>
    {#if editing}
      <!-- svelte-ignore a11y_autofocus -->
      <textarea class="edit" bind:value={draft} onkeydown={editKey} autofocus rows="3"></textarea>
      <div class="edit-hint">Enter speichert · Esc bricht ab</div>
    {:else}
      <div class="text" class:jumbo={emojiOnly(m.text, emojiUrl)}>
        {@html formatText(m.text, emojiUrl)}{#if m.edited}<span class="edited"> (bearbeitet)</span>{/if}
      </div>
    {/if}
    {#if m.attachments.length && chat.account}
      {@const account = chat.account}
      <div class="previews">
        {#each m.attachments as a (a.id + a.name)}
          {@const src = previewSrc(account, a)}
          {#if src && !previewFailed[a.id]}
            <button class="preview" onclick={() => download(a)} title="{a.name} herunterladen">
              <img {src} alt={a.name} loading="lazy" onerror={() => (previewFailed[a.id] = true)} />
            </button>
          {/if}
        {/each}
      </div>
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
          {@const custom = /^:([a-z0-9_+-]+):$/.exec(r.emoji)}
          {@const url = custom ? chat.emojiUrl(chat.account, custom[1]) : null}
          <button class="reaction" class:me={r.me} onclick={() => chat.toggleReaction(m, r.emoji)}>
            {#if url}<img class="emoji" src={url} alt={r.emoji} title={r.emoji} />{:else}<span class="emo">{r.emoji}</span>{/if}
            <span>{r.count}</span>
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
      <button class="icon-btn" title="Text kopieren (Teile: markieren, Strg+C)" onclick={copy}>⎘</button>
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
  /* Thread-Antwort im Hauptverlauf (Mattermost): Balken links. */
  .msg.reply {
    border-left: 3px solid var(--link);
    padding-left: 17px;
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
  .compact .head {
    display: none;
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
  /* Nur Emojis: groß wie bei Mattermost/Slack. */
  .text.jumbo {
    font-size: 2.2em;
    line-height: 1.25;
  }
  .text.jumbo :global(.emo) {
    font-size: 1em;
  }
  .previews {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 4px;
  }
  .previews:empty {
    display: none;
  }
  .preview {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0;
    background: none;
    overflow: hidden;
    line-height: 0;
  }
  .preview img {
    max-width: min(400px, 100%);
    max-height: 300px;
    object-fit: contain;
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
    grid-template-columns: repeat(6, 36px);
    background: var(--center-bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
    padding: 4px;
  }
  .picker button {
    border: none;
    background: none;
    font-size: 22px;
    padding: 4px;
    border-radius: 4px;
  }
  .picker button:hover {
    background: var(--hover);
  }
</style>
