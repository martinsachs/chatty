<script lang="ts">
  import { chat } from "$lib/chat.svelte";
  import { api, type Space } from "$lib/api";
  import { initials } from "$lib/format";

  let { onSettings }: { onSettings: () => void } = $props();

  type Section = "fav" | "channel" | "dm";
  const sections: { id: Section; title: string }[] = [
    { id: "fav", title: "Favoriten" },
    { id: "channel", title: "Kanäle" },
    { id: "dm", title: "Direktnachrichten" },
  ];

  type Sort = "recent" | "name" | "count";
  const sorts: { id: Sort; label: string }[] = [
    { id: "recent", label: "Neueste" },
    { id: "name", label: "Name" },
    { id: "count", label: "Meiste" },
  ];

  function load(key: string, fallback: string): string {
    try {
      return localStorage.getItem(key) ?? fallback;
    } catch {
      return fallback;
    }
  }

  function save(key: string, value: string) {
    try {
      localStorage.setItem(key, value);
    } catch {}
  }

  let filter = $state("");
  let sort = $state<Sort>(load("sidebarSort", "recent") as Sort);

  // Reihenfolge der Favoriten je Konto (per Ziehen), nur lokal.
  let favOrder = $state<string[]>([]);
  $effect(() => {
    const acc = chat.account;
    try {
      favOrder = acc ? JSON.parse(load(`favOrder:${acc}`, "[]")) : [];
    } catch {
      favOrder = [];
    }
  });

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

  // Breite per Ziehen am rechten Rand, gemerkt im Browser-Speicher der WebView.
  const MIN_W = 160;
  const MAX_W = 520;
  let width = $state(loadWidth());

  function loadWidth(): number {
    try {
      const w = Number(localStorage.getItem("sidebarWidth"));
      if (w >= MIN_W && w <= MAX_W) return w;
    } catch {}
    return 240;
  }

  function saveWidth() {
    try {
      localStorage.setItem("sidebarWidth", String(width));
    } catch {}
  }

  function startResize(e: PointerEvent) {
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const startW = width;
    const move = (ev: PointerEvent) => {
      width = Math.min(MAX_W, Math.max(MIN_W, startW + ev.clientX - startX));
    };
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      saveWidth();
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
  }

  async function join(s: Space) {
    if (!chat.account) return;
    filter = "";
    await chat.joinSpace(chat.account, s.id);
  }

  // Direktnachrichten ohne Nachricht seit 90 Tagen ausblenden (außer ungelesen, offen oder gesucht).
  const DM_MAX_AGE_MS = 90 * 86_400_000;

  function visible(s: Space, f: string): boolean {
    if (f) return s.name.toLowerCase().includes(f);
    if (s.kind === "channel" || s.favorite || s.unread || s.id === chat.space) return true;
    return s.last_active >= Date.now() - DM_MAX_AGE_MS;
  }

  function inSection(s: Space, sec: Section): boolean {
    if (sec === "fav") return s.favorite;
    if (s.favorite) return false;
    return sec === "channel" ? s.kind === "channel" : s.kind !== "channel";
  }

  function byName(a: Space, b: Space): number {
    return a.name.localeCompare(b.name, "de");
  }

  function compare(a: Space, b: Space): number {
    if (sort === "recent") return b.last_active - a.last_active || byName(a, b);
    if (sort === "count") return b.msg_count - a.msg_count || byName(a, b);
    return byName(a, b);
  }

  function spacesOf(sec: Section): Space[] {
    const all = (chat.account && chat.spaces[chat.account]) || [];
    const f = filter.trim().toLowerCase();
    const list = all.filter((s) => inSection(s, sec) && visible(s, f));
    if (sec !== "fav") return list.sort(compare);
    // Gezogene Reihenfolge zuerst, neue Favoriten danach nach gewählter Sortierung.
    const pos = (s: Space) => {
      const i = favOrder.indexOf(s.id);
      return i < 0 ? Number.MAX_SAFE_INTEGER : i;
    };
    return list.sort((a, b) => pos(a) - pos(b) || compare(a, b));
  }

  // Ziehen per Pointer-Events (HTML5-Drag&Drop kollidiert mit Tauris Datei-Drop).
  let drag = $state<{ id: string; section: Section | null; target: string | null } | null>(null);
  let suppressClick = false;

  function startDrag(e: PointerEvent, s: Space) {
    if (e.button !== 0) return;
    const sx = e.clientX;
    const sy = e.clientY;
    let active = false;
    const move = (ev: PointerEvent) => {
      if (!active && Math.abs(ev.clientX - sx) + Math.abs(ev.clientY - sy) < 6) return;
      active = true;
      const hit = (document.elementFromPoint(ev.clientX, ev.clientY) as HTMLElement | null)?.closest<HTMLElement>(
        "[data-section]",
      );
      drag = {
        id: s.id,
        section: (hit?.dataset.section as Section | undefined) ?? null,
        target: hit?.dataset.space ?? null,
      };
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      if (active && drag) {
        suppressClick = true;
        setTimeout(() => (suppressClick = false), 0);
        drop(drag);
      }
      drag = null;
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  function drop(d: { id: string; section: Section | null; target: string | null }) {
    const acc = chat.account;
    const s = acc ? chat.spaces[acc]?.find((x) => x.id === d.id) : undefined;
    if (!acc || !s || !d.section) return;
    if (d.section === "fav") {
      const order = spacesOf("fav")
        .map((x) => x.id)
        .filter((id) => id !== d.id);
      const at = d.target ? order.indexOf(d.target) : -1;
      order.splice(at < 0 ? order.length : at, 0, d.id);
      favOrder = order;
      save(`favOrder:${acc}`, JSON.stringify(order));
      if (!s.favorite) chat.setFlag(acc, s.id, { favorite: true });
    } else if (s.favorite) {
      chat.setFlag(acc, s.id, { favorite: false });
    }
  }

  function open(s: Space) {
    if (!suppressClick && chat.account) chat.openSpace(chat.account, s.id);
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

<aside class="sidebar" style:width="{width}px">
  <header>
    <div class="title">{chat.currentAccount?.label ?? "chatty"}</div>
    <div class="sub">
      {chat.currentAccount ? (chat.currentAccount.kind === "google" ? "Google Chat" : "Mattermost") : ""}
      {#if chat.currentAccount && !chat.currentAccount.online}· getrennt{/if}
    </div>
  </header>
  <div class="tools">
    <input
      class="filter"
      placeholder={chat.currentAccount?.kind === "mattermost" ? "Kanäle filtern / suchen" : "Kanäle filtern"}
      bind:value={filter}
    />
    <select class="sort" title="Sortierung: Neueste = letzte Nachricht, Meiste = Anzahl Nachrichten" bind:value={sort} onchange={() => save("sidebarSort", sort)}>
      {#each sorts as o (o.id)}
        <option value={o.id}>{o.label}</option>
      {/each}
    </select>
  </div>
  <div class="list">
    {#each sections as sec (sec.id)}
      {@const items = spacesOf(sec.id)}
      {#if items.length || (drag && sec.id === "fav")}
        <div class="section" data-section={sec.id} class:drop={drag?.section === sec.id && !drag.target}>
          {sec.title}
        </div>
        {#if !items.length}
          <div class="hint" data-section={sec.id}>Hierher ziehen</div>
        {/if}
        {#each items as s (s.id)}
          <div
            class="row"
            data-section={sec.id}
            data-space={s.id}
            class:drop={drag && drag.target === s.id && drag.id !== s.id}
            class:dragging={drag?.id === s.id}
          >
            <button
              class="item"
              class:active={s.id === chat.space}
              class:unread={s.unread && !s.muted}
              class:muted={s.muted}
              onpointerdown={(e) => startDrag(e, s)}
              onclick={() => open(s)}
            >
              <span class="icon">{s.kind === "channel" ? "#" : s.kind === "direct" ? "●" : "◆"}</span>
              <span class="name">{s.name}</span>
              {#if s.status_emoji}
                {@const custom = /^:([a-z0-9_+-]+):$/.exec(s.status_emoji)}
                {@const url = custom ? chat.emojiUrl(chat.account, custom[1]) : null}
                <span class="status" title="Status">
                  {#if url}<img class="emoji" src={url} alt={s.status_emoji} />{:else if !custom}{s.status_emoji}{/if}
                </span>
              {/if}
              {#if s.mention_count}<span class="badge">{s.mention_count}</span>{/if}
            </button>
            <span class="actions">
              <button
                class="act"
                title={s.favorite ? "Aus Favoriten entfernen" : "Zu Favoriten"}
                onclick={() => chat.account && chat.setFlag(chat.account, s.id, { favorite: !s.favorite })}
                >{s.favorite ? "★" : "☆"}</button
              >
              <button
                class="act"
                title={s.muted ? "Stummschaltung aufheben" : "Stumm schalten"}
                onclick={() => chat.account && chat.setFlag(chat.account, s.id, { muted: !s.muted })}
                >{s.muted ? "🔕" : "🔔"}</button
              >
            </span>
          </div>
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
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="resize" title="Breite ändern (Doppelklick: Standard)" onpointerdown={startResize} ondblclick={() => {
      width = 240;
      saveWidth();
    }}></div>
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
    position: relative;
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
    border-bottom: var(--brand-line-width, 1px) solid rgba(255, 255, 255, 0.1);
    border-image: var(--brand-line) 1;
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
  .tools {
    display: flex;
    gap: 6px;
    margin: 10px 12px;
  }
  .sort {
    flex-shrink: 0;
    appearance: none;
    -webkit-appearance: none;
    padding: 0 8px;
    border: none;
    border-radius: var(--radius);
    background: rgba(255, 255, 255, 0.12);
    color: var(--sidebar-text-strong);
    font-size: 12px;
  }
  .sort option {
    color: #000;
  }
  .filter {
    flex: 1;
    min-width: 0;
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
  .row {
    position: relative;
    user-select: none;
  }
  .row.drop {
    box-shadow: inset 0 2px 0 var(--sidebar-active-bar);
  }
  .row.dragging {
    opacity: 0.4;
  }
  .section.drop,
  .hint {
    color: var(--sidebar-text-strong);
  }
  .hint {
    padding: 6px 16px;
    margin: 0 12px;
    border: 1px dashed var(--sidebar-text);
    border-radius: var(--radius);
    font-size: 12px;
  }
  .actions {
    position: absolute;
    right: 8px;
    top: 50%;
    transform: translateY(-50%);
    display: none;
    gap: 2px;
  }
  .row:hover .actions {
    display: flex;
  }
  .act {
    border: none;
    background: var(--sidebar-bg);
    color: var(--sidebar-text-strong);
    border-radius: 4px;
    padding: 0 4px;
    font-size: 12px;
    line-height: 20px;
  }
  .act:hover {
    background: var(--sidebar-hover);
  }
  .status {
    flex-shrink: 0;
    font-size: 13px;
  }
  .item.muted {
    opacity: 0.5;
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
  .resize {
    position: absolute;
    top: 0;
    right: -3px;
    width: 6px;
    height: 100%;
    cursor: col-resize;
    z-index: 2;
  }
  .resize:hover {
    background: var(--sidebar-active-bar);
    opacity: 0.5;
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
