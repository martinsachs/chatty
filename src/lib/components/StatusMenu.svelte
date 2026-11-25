<script lang="ts">
  // Eigener Online-Status mit Auswahlmenü; nur bei Konten, deren Backend einen Status kennt.
  import type { Presence } from "$lib/api";
  import { chat } from "$lib/chat.svelte";
  import { PRESENCE_CHOICES, PRESENCE_LABELS } from "$lib/presence";
  import PresenceIcon from "./PresenceIcon.svelte";

  let { account }: { account: string } = $props();
  let open = $state(false);
  let presence = $derived(chat.ownPresence[account]);

  function choose(choice: Presence) {
    open = false;
    chat.setPresence(account, choice);
  }
</script>

<svelte:window onclick={() => (open = false)} onkeydown={(e) => e.key === "Escape" && (open = false)} />

{#if presence}
  <span class="status-menu">
    <button
      type="button"
      class="current"
      title="Status ändern"
      aria-haspopup="menu"
      aria-expanded={open}
      onclick={(e) => {
        e.stopPropagation();
        open = !open;
      }}
    >
      <PresenceIcon {presence} />
      {PRESENCE_LABELS[presence]} ▾
    </button>
    {#if open}
      <ul class="menu" role="menu">
        {#each PRESENCE_CHOICES as choice (choice)}
          <li role="none">
            <button
              type="button"
              role="menuitemradio"
              aria-checked={choice === presence}
              onclick={(e) => {
                e.stopPropagation();
                choose(choice);
              }}
            >
              <PresenceIcon presence={choice} />
              {PRESENCE_LABELS[choice]}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </span>
{/if}

<style>
  .status-menu {
    position: relative;
  }
  .current {
    border: none;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
    padding: 0;
  }
  .menu {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 20;
    min-width: 12em;
    margin: 4px 0 0;
    padding: 4px 0;
    list-style: none;
    background: var(--center-bg);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 4px 12px;
    border: none;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .menu button:hover,
  .menu button[aria-checked="true"] {
    background: var(--hover);
  }
</style>
