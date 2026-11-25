<script lang="ts">
  // GIF-Suche beim eingetragenen Anbieter. Sucht erst nach Eingabe: ohne Suchbegriff geht nichts an ihn.
  import { api, type Gif } from "$lib/api";
  import { chat } from "$lib/chat.svelte";

  let { onPick, onClose }: { onPick: (gif: Gif) => void; onClose: () => void } = $props();

  const SEARCH_DELAY_MS = 400;

  let query = $state("");
  let results = $state.raw<Gif[]>([]);
  let loading = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let searchSeq = 0;
  // GIPHY verlangt den sichtbaren Hinweis „Powered by GIPHY“ (API-Bedingungen, Abschnitt 5A).
  const isGiphy = $derived(chat.gifProvider?.endsWith("giphy.com") ?? false);

  function scheduleSearch() {
    clearTimeout(timer);
    const q = query.trim();
    if (!q) {
      searchSeq++;
      results = [];
      loading = false;
      return;
    }
    timer = setTimeout(() => search(q), SEARCH_DELAY_MS);
  }

  async function search(q: string) {
    const seq = ++searchSeq;
    loading = true;
    try {
      const found = await api.searchGifs(q);
      if (seq === searchSeq) results = found;
    } catch (e) {
      if (seq === searchSeq) chat.toast(`GIF-Suche: ${e}`, "error");
    } finally {
      if (seq === searchSeq) loading = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="gif-picker" role="dialog" aria-label="GIF auswählen" tabindex="-1" onclick={(e) => e.stopPropagation()}>
  <!-- svelte-ignore a11y_autofocus -->
  <input class="search" placeholder="GIF suchen" bind:value={query} oninput={scheduleSearch} autofocus />
  <div class="scroll">
    {#if loading}
      <p class="none">Suche …</p>
    {:else if query.trim() && !results.length}
      <p class="none">Nichts gefunden.</p>
    {/if}
    <div class="grid">
      {#each results as gif (gif.url)}
        <button type="button" title={gif.title} onclick={() => onPick(gif)}>
          <img src={gif.preview} alt={gif.title} />
        </button>
      {/each}
    </div>
  </div>
  <p class="attribution">{isGiphy ? "Powered by GIPHY" : `Suche über ${chat.gifProvider}`}</p>
</div>

<style>
  .gif-picker {
    position: absolute;
    bottom: 100%;
    left: 0;
    z-index: 20;
    width: 340px;
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
    max-height: 300px;
    overflow-y: auto;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 4px;
  }
  .grid button {
    border: none;
    padding: 0;
    background: var(--hover);
    cursor: pointer;
    border-radius: var(--radius);
    overflow: hidden;
  }
  .grid img {
    display: block;
    width: 100%;
    height: 80px;
    object-fit: cover;
  }
  .none,
  .attribution {
    margin: 4px 0 0;
    font-size: 11px;
    color: var(--text-muted);
  }
  .attribution {
    text-align: right;
  }
</style>
