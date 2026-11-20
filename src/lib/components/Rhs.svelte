<script lang="ts">
  import { chat } from "$lib/chat.svelte";
  import { day } from "$lib/format";
  import Composer from "./Composer.svelte";
  import MessageItem from "./MessageItem.svelte";

  function spaceName(id: string): string {
    return (chat.account && chat.spaces[chat.account]?.find((s) => s.id === id)?.name) || "";
  }
</script>

{#if chat.rhs}
  <aside class="rhs">
    <header>
      <span class="title">{chat.rhs.kind === "thread" ? "Thread" : `Suche: ${chat.rhs.query}`}</span>
      <button class="icon-btn" title="Schließen" onclick={() => (chat.rhs = null)}>✕</button>
    </header>
    <div class="body">
      {#if chat.rhs.kind === "thread"}
        <MessageItem message={chat.rhs.root} inThread />
        <div class="count">
          {chat.rhs.replies.length}
          {chat.rhs.replies.length === 1 ? "Antwort" : "Antworten"}
        </div>
        {#each chat.rhs.replies as m (m.id)}
          <MessageItem message={m} inThread />
        {/each}
      {:else if chat.rhs.loading}
        <p class="info">Suche läuft …</p>
      {:else if !chat.rhs.results.length}
        <p class="info">Keine Treffer.</p>
      {:else}
        {#each chat.rhs.results as m (m.id)}
          <button class="hit" onclick={() => chat.account && chat.openSpace(chat.account, m.space_id)}>
            <span class="where">{spaceName(m.space_id)} · {day(m.created)}</span>
          </button>
          <MessageItem message={m} inThread />
        {/each}
      {/if}
    </div>
    {#if chat.rhs.kind === "thread"}
      <Composer placeholder="Antworten …" thread={chat.rhs.root.thread_id} />
    {/if}
  </aside>
{/if}

<style>
  .rhs {
    width: 400px;
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    background: var(--center-bg);
    flex-shrink: 0;
  }
  header {
    height: var(--header-height);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px 0 16px;
    border-bottom: 1px solid var(--border);
  }
  .title {
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .body {
    flex: 1;
    overflow-y: auto;
    padding-bottom: 12px;
  }
  .count {
    margin: 4px 20px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    color: var(--text-muted);
  }
  .info {
    text-align: center;
    color: var(--text-muted);
  }
  .hit {
    display: block;
    border: none;
    background: none;
    padding: 12px 20px 0;
    color: var(--link);
    font-size: 12px;
    font-weight: 600;
  }
</style>
