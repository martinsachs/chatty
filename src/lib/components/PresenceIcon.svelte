<script lang="ts">
  // Online-Status als Punkt (Stil in app.css); Optiken können die Form ändern (seequ: Kleeblatt).
  import type { Presence } from "$lib/api";
  import { PRESENCE_LABELS } from "$lib/presence";

  let { presence }: { presence: Presence } = $props();
</script>

<span class="presence" data-presence={presence} role="img" aria-label={PRESENCE_LABELS[presence]} title={PRESENCE_LABELS[presence]}
></span>
