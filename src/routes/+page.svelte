<script lang="ts">
  import { onMount } from "svelte";
  import { chat } from "$lib/chat.svelte";
  import ChannelView from "$lib/components/ChannelView.svelte";
  import Rhs from "$lib/components/Rhs.svelte";
  import Settings, { type SettingsTab } from "$lib/components/Settings.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import Toasts from "$lib/components/Toasts.svelte";
  import { applyTitleBar } from "$lib/titlebar";
  import { checkForUpdate } from "$lib/update";

  let settingsOpen = $state(false);
  let settingsTab = $state<SettingsTab | undefined>();

  $effect(() => {
    document.documentElement.dataset.theme = chat.theme;
    applyTitleBar();
  });

  onMount(async () => {
    // Abgelöstes Fenster: #popout/<konto>/<space>
    const m = location.hash.match(/^#popout\/([^/]+)\/(.+)$/);
    chat.popout = !!m;
    await chat.init(m ? decodeURIComponent(m[1]) : undefined);
    if (m) {
      await chat.openSpace(decodeURIComponent(m[1]), decodeURIComponent(m[2]));
    } else {
      if (!chat.accounts.length) settingsOpen = true;
      checkForUpdate();
    }
    window.addEventListener("focus", () => chat.markRead());
  });
</script>

<main class:popout={chat.popout}>
  {#if !chat.popout}
    <Sidebar
      onSettings={(tab) => {
        settingsTab = tab;
        settingsOpen = true;
      }}
    />
  {/if}
  {#if chat.space}
    <ChannelView />
  {:else}
    <section class="welcome">
      <h2>chatty</h2>
      <p>{chat.accounts.length ? "Kanal oder Direktnachricht links auswählen." : "Zuerst ein Konto hinzufügen."}</p>
    </section>
  {/if}
  <Rhs />
</main>

{#if settingsOpen}
  <Settings initialTab={settingsTab} onClose={() => (settingsOpen = false)} />
{/if}
<Toasts />

<style>
  main {
    display: flex;
    height: 100vh;
  }
  .welcome {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
  }
</style>
