<script lang="ts">
  import { tick } from "svelte";
  import { api, type Message } from "$lib/api";
  import { chat } from "$lib/chat.svelte";
  import { day } from "$lib/format";
  import Composer from "./Composer.svelte";
  import MessageItem from "./MessageItem.svelte";

  let list: HTMLDivElement | undefined = $state();
  let query = $state("");
  let items: Record<string, MessageItem> = {};

  const GROUP_MS = 5 * 60_000;

  function compact(prev: Message | undefined, m: Message): boolean {
    return !!prev && prev.sender.id === m.sender.id && m.created - prev.created < GROUP_MS && day(prev.created) === day(m.created);
  }

  // Bei neuen Nachrichten nach unten scrollen, wenn der Nutzer unten war.
  let atBottom = true;
  let lastCount = 0;
  let lastSpace: string | null = null;
  $effect(() => {
    const count = chat.messages.length;
    const space = chat.space;
    const changed = space !== lastSpace;
    const grew = count > lastCount;
    lastCount = count;
    lastSpace = space;
    if (changed || (grew && atBottom)) tick().then(() => list && (list.scrollTop = list.scrollHeight));
  });

  async function onScroll() {
    if (!list) return;
    atBottom = list.scrollHeight - list.scrollTop - list.clientHeight < 40;
    if (list.scrollTop < 80 && chat.cursor && !chat.loading) {
      const before = list.scrollHeight;
      await chat.loadOlder();
      await tick();
      list.scrollTop += list.scrollHeight - before;
    }
  }

  function editLast() {
    const mine = [...chat.messages].reverse().find((m) => m.mine);
    if (mine) items[mine.id]?.startEdit();
  }

  function popout() {
    const s = chat.currentSpace;
    if (chat.account && s) api.openWindow(chat.account, s.id, s.name).catch((e) => chat.toast(String(e), "error"));
  }

  function searchKey(e: KeyboardEvent) {
    if (e.key === "Enter") chat.search(query);
  }
</script>

<section class="center">
  <header>
    <div class="name">
      {chat.currentSpace?.name ?? ""}
    </div>
    <div class="spacer"></div>
    {#if !chat.popout}
      <input class="search" placeholder="Suchen" bind:value={query} onkeydown={searchKey} />
      <button class="icon-btn" title="In eigenem Fenster öffnen" onclick={popout}>⧉</button>
    {/if}
  </header>
  <div class="messages" bind:this={list} onscroll={onScroll}>
    {#if chat.loading && !chat.messages.length}
      <p class="info">Lade …</p>
    {:else if !chat.cursor && chat.messages.length}
      <p class="info">Anfang von {chat.currentSpace?.name}</p>
    {/if}
    {#each chat.messages as m, i (m.id)}
      {@const prev = chat.messages[i - 1]}
      {#if !prev || day(prev.created) !== day(m.created)}
        <div class="day"><span>{day(m.created)}</span></div>
      {/if}
      <MessageItem bind:this={items[m.id]} message={m} compact={compact(prev, m)} />
    {/each}
  </div>
  <Composer placeholder={`Nachricht an ${chat.currentSpace?.name ?? ""}`} onEditLast={editLast} />
</section>

<style>
  .center {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--center-bg);
  }
  header {
    height: var(--header-height);
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 16px 0 20px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .name {
    font-weight: 700;
    font-size: 16px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .name::before {
    content: var(--channel-prefix);
  }
  .spacer {
    flex: 1;
  }
  .search {
    width: 220px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 5px 10px;
    background: var(--hover);
  }
  .messages {
    flex: 1;
    overflow-y: auto;
    padding: 8px 0 16px;
  }
  .info {
    text-align: center;
    color: var(--text-muted);
    font-size: 13px;
  }
  .day {
    display: flex;
    align-items: center;
    margin: 16px 20px 4px;
    font-size: 12px;
    font-weight: 700;
    color: var(--text);
  }
  .day::before,
  .day::after {
    content: "";
    flex: 1;
    border-top: 1px solid var(--border);
  }
  .day span {
    padding: 0 12px;
  }
</style>
