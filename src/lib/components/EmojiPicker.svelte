<script lang="ts" module>
  // Liste einmal je Fenster laden; sie ändert sich nicht.
  let catalogCache: Promise<[string, string][]> | null = null;
</script>

<script lang="ts">
  // Emoji-Auswahl: zuletzt benutzte, eigene Server-Emojis, alle Unicode-Emojis; Suche nach Kurznamen.
  import { api } from "$lib/api";
  import { chat } from "$lib/chat.svelte";

  let { onPick, onClose }: { onPick: (emoji: string) => void; onClose: () => void } = $props();

  const RECENT_KEY = "recentEmojis";
  const RECENT_MAX = 16;

  let query = $state("");
  let catalog = $state.raw<[string, string][]>([]);
  let recent = $state<string[]>(loadRecent());

  catalogCache ??= api.emojiCatalog();
  catalogCache.then((list) => (catalog = list)).catch((e) => chat.toast(`Emojis laden: ${e}`, "error"));

  const needle = $derived(query.trim().toLowerCase().replaceAll(":", ""));
  const matches = (name: string) => !needle || name.includes(needle);
  const customNames = $derived(
    [...((chat.account && chat.customEmojis[chat.account]) || [])].filter(matches).sort(),
  );
  const unicode = $derived(catalog.filter(([name]) => matches(name)));

  function loadRecent(): string[] {
    try {
      return JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    } catch {
      return [];
    }
  }

  function pick(emoji: string) {
    recent = [emoji, ...recent.filter((e) => e !== emoji)].slice(0, RECENT_MAX);
    try {
      localStorage.setItem(RECENT_KEY, JSON.stringify(recent));
    } catch {
      // Ohne Speicher (privater Modus) gibt es nur keine Liste „Zuletzt“.
    }
    onPick(emoji);
  }

  function customUrl(emoji: string) {
    const name = /^:([a-z0-9_+-]+):$/.exec(emoji)?.[1];
    return name ? chat.emojiUrl(chat.account, name) : null;
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="emoji-picker" role="dialog" aria-label="Emoji auswählen" tabindex="-1" onclick={(e) => e.stopPropagation()}>
  <!-- svelte-ignore a11y_autofocus -->
  <input class="search" placeholder="Emoji suchen, z. B. smile" bind:value={query} autofocus />
  <div class="scroll">
    {#if !needle && recent.length}
      <h4>Zuletzt</h4>
      <div class="grid">
        {#each recent as emoji (emoji)}
          {@const url = customUrl(emoji)}
          <button type="button" title={emoji} onclick={() => pick(emoji)}>
            {#if url}<img src={url} alt={emoji} />{:else}{emoji}{/if}
          </button>
        {/each}
      </div>
    {/if}
    {#if customNames.length}
      <h4>Server-Emojis</h4>
      <div class="grid">
        {#each customNames as name (name)}
          <button type="button" title=":{name}:" onclick={() => pick(`:${name}:`)}>
            <img src={chat.emojiUrl(chat.account, name)} alt=":{name}:" />
          </button>
        {/each}
      </div>
    {/if}
    <h4>Emojis</h4>
    <div class="grid">
      {#each unicode as [name, emoji] (emoji)}
        <button type="button" title=":{name}:" onclick={() => pick(emoji)}>{emoji}</button>
      {/each}
    </div>
    {#if needle && !unicode.length && !customNames.length}<p class="none">Nichts gefunden.</p>{/if}
  </div>
</div>

<style>
  .emoji-picker {
    position: absolute;
    bottom: 100%;
    left: 0;
    z-index: 20;
    width: 320px;
    margin-bottom: 4px;
    padding: 8px;
    background: var(--center-bg);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
  }
  .search {
    width: 100%;
    margin-bottom: 6px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--center-bg);
    color: var(--text);
  }
  .scroll {
    max-height: 260px;
    overflow-y: auto;
  }
  h4 {
    margin: 6px 0 2px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
  }
  .grid button {
    border: none;
    background: none;
    font-size: 20px;
    line-height: 1;
    padding: 4px 0;
    cursor: pointer;
    border-radius: var(--radius);
  }
  .grid button:hover {
    background: var(--hover);
  }
  .grid img {
    width: 20px;
    height: 20px;
    object-fit: contain;
  }
  .none {
    font-size: 12px;
    color: var(--text-muted);
  }
</style>
