<script lang="ts">
  // Eigenes Server-Emoji anlegen (nur Mattermost), aufgebaut wie der Dialog der Mattermost-Webapp.
  // Der Kern verkleinert das Bild vorher; die Vorschau zeigt genau das, was hochgeladen wird.
  import { api, emojiDraftSrc, type EmojiDraft } from "$lib/api";
  import { chat } from "$lib/chat.svelte";

  let { onClose }: { onClose: () => void } = $props();

  const NAME_MAX = 64;

  let name = $state("");
  let draft = $state<EmojiDraft | null>(null);
  let nameProblem = $state<string | null>(null);
  let error = $state("");
  let picking = $state(false);
  let saving = $state(false);
  let checkSeq = 0;

  const account = chat.account;
  const canSave = $derived(!!account && !!draft && !!name && !nameProblem && !saving);

  $effect(() => {
    const current = name;
    if (!account || !current) {
      nameProblem = null;
      return;
    }
    const seq = ++checkSeq;
    api
      .emojiNameProblem(account, current)
      .then((problem) => seq === checkSeq && (nameProblem = problem))
      .catch((e) => seq === checkSeq && (nameProblem = String(e)));
  });

  /** Vorschlag aus dem Dateinamen: ohne Endung, nur erlaubte Zeichen. */
  function nameFromFile(fileName: string): string {
    return fileName
      .replace(/\.[^.]+$/, "")
      .toLowerCase()
      .replace(/[^a-z0-9_+-]+/g, "_")
      .replace(/^_+|_+$/g, "")
      .slice(0, NAME_MAX);
  }

  async function pickImage() {
    error = "";
    picking = true;
    try {
      const picked = await api.pickEmojiImage();
      if (!picked) return;
      draft = picked;
      if (!name) name = nameFromFile(picked.file_name);
    } catch (e) {
      error = String(e);
    } finally {
      picking = false;
    }
  }

  async function save() {
    if (!canSave || !account || !draft) return;
    error = "";
    saving = true;
    try {
      await api.createEmoji(account, name, draft.id);
      chat.addCustomEmoji(account, name);
      chat.toast(`Emoji :${name}: angelegt`, "info");
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  function describe(d: EmojiDraft): string {
    const size = `${d.width} × ${d.height}`;
    const parts = [d.resized ? `verkleinert auf ${size}` : size];
    if (d.frames > 1) parts.push(`${d.frames} Bilder`);
    parts.push(`${Math.max(1, Math.round(d.bytes / 1024))} KiB`);
    return parts.join(", ");
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="add-emoji-title"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
  >
    <form
      onsubmit={(e) => {
        e.preventDefault();
        save();
      }}
    >
      <header>
        <h3 id="add-emoji-title">Emoji hinzufügen</h3>
        <button type="button" class="icon-btn" title="Schließen" onclick={onClose}>✕</button>
      </header>

      <div class="content">
        <label for="emoji-name">Name</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          id="emoji-name"
          bind:value={() => name, (v) => (name = v.toLowerCase())}
          maxlength={NAME_MAX}
          autocomplete="off"
          spellcheck="false"
          autofocus
          aria-invalid={!!nameProblem}
          aria-describedby="emoji-name-hint"
        />
        <small id="emoji-name-hint" class:problem={nameProblem}>
          {nameProblem ?? "Bis 64 Zeichen: Kleinbuchstaben, Ziffern und - + _. Im Text als :name: nutzbar."}
        </small>

        <span class="label">Bild</span>
        <div class="pick">
          <button type="button" class="btn secondary" disabled={picking || saving} onclick={pickImage}>
            {picking ? "Wird vorbereitet …" : "Auswählen …"}
          </button>
          {#if draft}<span class="file" title={draft.file_name}>{draft.file_name}</span>{/if}
        </div>
        <small>
          {#if draft}
            {describe(draft)}
          {:else}
            PNG, JPEG, GIF oder WebP. Große Bilder werden auf 128 × 128 verkleinert, GIFs bleiben animiert
            (höchstens 70 Bilder).
          {/if}
        </small>

        {#if draft}
          <span class="label">Vorschau</span>
          <p class="preview">
            Das ist ein Satz mit <img class="emoji" src={emojiDraftSrc(draft.id)} alt=":{name || 'emoji'}:" /> darin.
          </p>
        {/if}

        {#if error}<p class="error" role="alert">{error}</p>{/if}
      </div>

      <footer>
        <button type="button" class="btn secondary" onclick={onClose}>Abbrechen</button>
        <button type="submit" class="btn" disabled={!canSave}>{saving ? "Wird gespeichert …" : "Speichern"}</button>
      </footer>
    </form>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 50;
  }
  .dialog {
    width: 440px;
    max-width: calc(100vw - 32px);
    max-height: 85vh;
    background: var(--center-bg);
    color: var(--text);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 32px rgba(0, 0, 0, 0.2);
  }
  form {
    display: contents;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid var(--border);
    padding: 0 12px 0 24px;
  }
  h3 {
    margin: 14px 0;
    font-size: 15px;
  }
  .content {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 16px 24px;
    overflow-y: auto;
  }
  label,
  .label {
    margin-top: 10px;
    font-weight: 600;
  }
  label:first-child {
    margin-top: 0;
  }
  input {
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--center-bg);
    color: var(--text);
  }
  small {
    color: var(--text-muted);
  }
  small.problem,
  .error {
    color: var(--error);
  }
  .pick {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .file {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .preview {
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .error {
    margin: 10px 0 0;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 24px 16px;
    border-top: 1px solid var(--border);
  }
</style>
