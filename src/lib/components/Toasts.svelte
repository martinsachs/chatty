<script lang="ts">
  import { chat } from "$lib/chat.svelte";
</script>

<div class="toasts">
  {#each chat.toasts as t (t.id)}
    <div class="toast {t.kind}">
      <span class="text">{t.text}</span>
      {#if t.action}
        <button
          class="act"
          onclick={() => {
            t.action?.run();
            chat.closeToast(t.id);
          }}>{t.action.label}</button
        >
      {/if}
      <button class="close" title="Schließen" onclick={() => chat.closeToast(t.id)}>✕</button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 100;
    max-width: 380px;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 8px;
    background: #1f1f1f;
    color: #fff;
    border-radius: var(--radius);
    padding: 10px 12px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
    border-left: 4px solid var(--accent);
    animation: in 0.15s ease-out;
  }
  .toast.error {
    border-left-color: var(--error);
  }
  .toast.message {
    border-left-color: var(--mention-bg);
  }
  .text {
    flex: 1;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
  }
  button {
    border: none;
    background: none;
    color: #fff;
  }
  .act {
    font-weight: 700;
    text-decoration: underline;
  }
  .close {
    opacity: 0.6;
  }
  @keyframes in {
    from {
      transform: translateY(8px);
      opacity: 0;
    }
  }
</style>
