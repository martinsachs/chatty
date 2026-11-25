<script lang="ts">
  import "../app.css";
  import "../retro.css";
  import "../seequ.css";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import { applyZoom, zoomKey, zoomStorage } from "$lib/zoom.svelte";
  let { children } = $props();

  onMount(applyZoom);

  // Links aus Nachrichten nie im WebView öffnen (auch nicht per Mittelklick oder neuem Fenster),
  // sondern im Standardbrowser; andere Schemata gar nicht.
  function onLink(e: MouseEvent) {
    const a = (e.target as HTMLElement | null)?.closest?.("a[href]") as HTMLAnchorElement | null;
    if (!a) return;
    e.preventDefault();
    if (e.type === "click" || e.button === 1) {
      if (/^https?:\/\//.test(a.href)) openUrl(a.href);
    }
  }
</script>

<svelte:document onclickcapture={onLink} onauxclickcapture={onLink} />
<svelte:window onkeydown={zoomKey} onstorage={zoomStorage} />

{@render children()}
