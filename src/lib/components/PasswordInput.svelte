<script lang="ts">
  // Passwortfeld mit Augen-Knopf zum Anzeigen; weitere Attribute (required, autocomplete …) gehen ans <input>.
  import type { HTMLInputAttributes } from "svelte/elements";

  let { value = $bindable(""), ...rest }: { value?: string } & Omit<HTMLInputAttributes, "type" | "value"> = $props();
  let visible = $state(false);
</script>

<span class="password">
  <input {...rest} type={visible ? "text" : "password"} bind:value />
  <button
    type="button"
    class="eye"
    title={visible ? "Verbergen" : "Anzeigen"}
    aria-label={visible ? "Passwort verbergen" : "Passwort anzeigen"}
    aria-pressed={visible}
    onclick={() => (visible = !visible)}
  >
    <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
      <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z" />
      <circle cx="12" cy="12" r="3" />
      {#if visible}<line x1="3" y1="21" x2="21" y2="3" />{/if}
    </svg>
  </button>
</span>

<style>
  .password {
    position: relative;
    display: flex;
  }
  input {
    flex: 1;
    padding-right: 34px;
  }
  .eye {
    position: absolute;
    right: 4px;
    top: 50%;
    transform: translateY(-50%);
    border: none;
    background: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 4px;
    line-height: 0;
  }
  .eye:hover,
  .eye[aria-pressed="true"] {
    color: var(--text);
  }
  svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
