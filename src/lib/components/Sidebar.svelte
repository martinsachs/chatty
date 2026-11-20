<script lang="ts">
  import { chat } from "$lib/chat.svelte";
  import { api, type Space, type SpaceKind } from "$lib/api";
  import { initials } from "$lib/format";

  let { onSettings }: { onSettings: () => void } = $props();

  const sections: { title: string; kinds: SpaceKind[] }[] = [
    { title: "Kanäle", kinds: ["channel"] },
    { title: "Direktnachrichten", kinds: ["direct", "group"] },
  ];

  let filter = $state("");

  // Beitretbare Kanäle vom Server (nur Mattermost), passend zum Filter, leicht verzögert.
  let joinable = $state<Space[]>([]);
  $effect(() => {
    const q = filter.trim();
    const acc = chat.currentAccount;
    joinable = [];
    if (!q || acc?.kind !== "mattermost" || !acc.online) return;
    const t = setTimeout(async () => {
      const found = await api.browseSpaces(acc.id, q).catch(() => []);
      if (filter.trim() === q && chat.account === acc.id) joinable = found;
    }, 300);
    return () => clearTimeout(t);
  });

  async function join(s: Space) {
    if (!chat.account) return;
    filter = "";
    await chat.joinSpace(chat.account, s.id);
  }

  function spacesOf(kinds: SpaceKind[]): Space[] {
    const all = (chat.account && chat.spaces[chat.account]) || [];
    const f = filter.trim().toLowerCase();
    return all.filter((s) => kinds.includes(s.kind) && (!f || s.name.toLowerCase().includes(f)));
  }

  function unreadTotal(account: string): number {
    return (chat.spaces[account] ?? []).reduce((n, s) => n + s.mention_count, 0);
  }
</script>

<nav class="rail" aria-label="Konten">
  {#each chat.accounts as a (a.id)}
    <button
      class="team"
      class:active={a.id === chat.account}
      title={`${a.label} (${a.kind === "google" ? "Google Chat" : "Mattermost"})`}
      onclick={() => chat.selectAccount(a.id)}
    >
      {initials(a.label)}
      {#if !a.online}<span class="offline" title="nicht verbunden"></span>{/if}
      {#if unreadTotal(a.id)}<span class="badge">{unreadTotal(a.id)}</span>{/if}
    </button>
  {/each}
  <button class="team add" title="Konto hinzufügen / Einstellungen" onclick={onSettings}>+</button>
</nav>

<aside class="sidebar">
  <header>
    <div class="title">{chat.currentAccount?.label ?? "chatty"}</div>
    <div class="sub">
      {chat.currentAccount ? (chat.currentAccount.kind === "google" ? "Google Chat" : "Mattermost") : ""}
      {#if chat.currentAccount && !chat.currentAccount.online}· getrennt{/if}
    </div>
  </header>
  <input
    class="filter"
    placeholder={chat.currentAccount?.kind === "mattermost" ? "Kanäle filtern / suchen" : "Kanäle filtern"}
    bind:value={filter}
  />
  <div class="list">
    {#each sections as sec (sec.title)}
      {@const items = spacesOf(sec.kinds)}
      {#if items.length}
        <div class="section">{sec.title}</div>
        {#each items as s (s.id)}
          <button
            class="item"
            class:active={s.id === chat.space}
            class:unread={s.unread}
            onclick={() => chat.account && chat.openSpace(chat.account, s.id)}
          >
            <span class="icon">{s.kind === "channel" ? "#" : s.kind === "direct" ? "●" : "◆"}</span>
            <span class="name">{s.name}</span>
            {#if s.mention_count}<span class="badge">{s.mention_count}</span>{/if}
          </button>
        {/each}
      {/if}
    {/each}
    {#if joinable.length}
      <div class="section">Weitere Kanäle</div>
      {#each joinable as s (s.id)}
        <button class="item" title="Beitreten" onclick={() => join(s)}>
          <span class="icon">#</span>
          <span class="name">{s.name}</span>
          <span class="join">Beitreten</span>
        </button>
      {/each}
    {/if}
    {#if !chat.accounts.length}
      <p class="empty">Noch kein Konto. Mit <b>+</b> links ein Konto hinzufügen.</p>
    {/if}
  </div>
</aside>

<style>
  .rail {
    width: 64px;
    background: var(--rail-bg);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 12px 0;
    flex-shrink: 0;
  }
  .team {
    position: relative;
    width: 40px;
    height: 40px;
    border-radius: 8px;
    border: none;
    background: rgba(255, 255, 255, 0.16);
    color: #fff;
    font-weight: 700;
  }
  .team.active {
    box-shadow: 0 0 0 3px var(--rail-bg), 0 0 0 5px rgba(255, 255, 255, 0.64);
  }
  .team.add {
    background: transparent;
    border: 1px dashed rgba(255, 255, 255, 0.4);
    font-size: 20px;
  }
  .offline {
    position: absolute;
    left: -3px;
    bottom: -3px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #999;
    border: 2px solid var(--rail-bg);
  }
  .sidebar {
    width: 240px;
    background: var(--sidebar-bg);
    color: var(--sidebar-text);
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
  }
  header {
    height: var(--header-height);
    background: var(--sidebar-header-bg);
    padding: 8px 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  }
  .title {
    color: var(--sidebar-text-strong);
    font-weight: 700;
    font-size: 16px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub {
    font-size: 12px;
  }
  .filter {
    margin: 10px 12px;
    padding: 6px 10px;
    border-radius: var(--radius);
    border: none;
    background: rgba(255, 255, 255, 0.12);
    color: #fff;
  }
  .filter::placeholder {
    color: var(--sidebar-text);
  }
  .list {
    overflow-y: auto;
    flex: 1;
    padding-bottom: 12px;
  }
  .section {
    padding: 14px 16px 4px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    border: none;
    background: transparent;
    color: var(--sidebar-text);
    text-align: left;
    padding: 6px 16px;
    border-left: 4px solid transparent;
  }
  .item:hover {
    background: var(--sidebar-hover);
  }
  .item.active {
    background: var(--sidebar-active-bg);
    border-left-color: var(--sidebar-active-bar);
    color: var(--sidebar-text-strong);
  }
  .item.unread {
    color: var(--sidebar-text-strong);
    font-weight: 700;
  }
  .icon {
    width: 14px;
    text-align: center;
    opacity: 0.7;
    font-size: 12px;
  }
  .name {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .join {
    font-size: 11px;
    opacity: 0.7;
  }
  .badge {
    background: var(--mention-bg);
    color: #fff;
    border-radius: 10px;
    padding: 0 6px;
    font-size: 11px;
    font-weight: 700;
    min-width: 18px;
    text-align: center;
  }
  .team .badge {
    position: absolute;
    top: -6px;
    right: -6px;
  }
  .empty {
    padding: 16px;
    font-size: 13px;
  }
</style>
