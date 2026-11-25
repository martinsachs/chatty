<script lang="ts">
  import { api, type AccountInfo } from "$lib/api";
  import { chat } from "$lib/chat.svelte";
  import { setZoom, zoom, ZOOMS } from "$lib/zoom.svelte";

  let { onClose }: { onClose: () => void } = $props();

  let tab = $state<"accounts" | "add-mm" | "settings">(chat.accounts.length ? "accounts" : "add-mm");
  let theme = $state(chat.theme);
  let notifications = $state(chat.notifications);

  // Mattermost-Formular; reloginId gesetzt = vorhandenes Konto neu anmelden
  let reloginId = $state<string | undefined>();
  let server = $state("https://");
  let login = $state("");
  let password = $state("");
  let mfa = $state("");
  let busy = $state(false);
  let error = $state("");

  // Google-OAuth-Client (vom Workspace-Admin); Secret wird nie zurückgelesen
  let clientId = $state(chat.googleClientId);
  let clientSecret = $state("");
  let editClient = $state(!chat.googleConfigured);

  async function saveClient(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = "";
    try {
      await api.setGoogleClient(clientId, clientSecret);
      clientSecret = "";
      await chat.refreshInfo();
      editClient = !chat.googleConfigured;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  // GIF-Anbieter (opt-in); Schlüssel wird nie zurückgelesen
  let gifApiUrl = $state(chat.gifApiUrl);
  let gifApiKey = $state("");

  async function applyGif(apiUrl: string, apiKey: string) {
    busy = true;
    error = "";
    try {
      await api.setGifConfig(apiUrl, apiKey);
      gifApiKey = "";
      await chat.refreshInfo();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function saveGif(e: SubmitEvent) {
    e.preventDefault();
    applyGif(gifApiUrl, gifApiKey);
  }

  function disableGif() {
    gifApiUrl = "";
    applyGif("", "");
  }

  async function done(promise: Promise<unknown>) {
    busy = true;
    error = "";
    try {
      await promise;
      password = "";
      mfa = "";
      await chat.refreshInfo();
      for (const a of chat.accounts) if (!chat.spaces[a.id]) await chat.loadSpaces(a.id);
      tab = "accounts";
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function addMattermost(e: SubmitEvent) {
    e.preventDefault();
    done(api.addMattermost({ accountId: reloginId, server, login, password, mfa: mfa || undefined }));
  }

  function addGoogle(accountId?: string) {
    done(api.addGoogle(accountId));
  }

  function relogin(a: AccountInfo) {
    if (a.kind === "google") return addGoogle(a.id);
    reloginId = a.id;
    server = a.server ?? "https://";
    login = a.login ?? "";
    tab = "add-mm";
  }

  async function remove(a: AccountInfo) {
    if (!confirm(`Konto ${a.label} entfernen? Token und Cache werden gelöscht.`)) return;
    await chat.guard(api.removeAccount(a.id));
    if (chat.account === a.id) {
      chat.account = null;
      chat.space = null;
      chat.messages = [];
    }
    delete chat.spaces[a.id];
    await chat.refreshInfo();
  }

  async function saveSettings() {
    await chat.saveSettings(theme, notifications);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <div class="dialog" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="-1">
    <header>
      <nav>
        <button class:active={tab === "accounts"} onclick={() => (tab = "accounts")}>Konten</button>
        <button
          class:active={tab === "add-mm"}
          onclick={() => {
            reloginId = undefined;
            tab = "add-mm";
          }}>Konto hinzufügen</button
        >
        <button class:active={tab === "settings"} onclick={() => (tab = "settings")}>Darstellung</button>
      </nav>
      <button class="icon-btn" title="Schließen" onclick={onClose}>✕</button>
    </header>

    <div class="content">
      {#if tab === "accounts"}
        {#each chat.accounts as a (a.id)}
          <div class="account">
            <span class="dot" class:on={a.online}></span>
            <div class="label">
              <b>{a.label}</b>
              <small>{a.kind === "google" ? "Google Chat" : `Mattermost · ${a.server}`}</small>
            </div>
            {#if !a.online}<button class="btn secondary" disabled={busy} onclick={() => relogin(a)}>Neu anmelden</button>{/if}
            <button class="btn secondary" onclick={() => remove(a)}>Entfernen</button>
          </div>
        {:else}
          <p>Noch keine Konten.</p>
        {/each}
      {:else if tab === "add-mm"}
        <h3>Google Chat</h3>
        {#if chat.googleConfigured && !editClient}
          <p>Anmeldung im Browser mit deinem Google-Konto.</p>
          <button class="btn" disabled={busy} onclick={() => addGoogle()}>Mit Google anmelden</button>
          <button class="btn secondary" disabled={busy} onclick={() => (editClient = true)}>OAuth-Client ändern</button>
        {:else}
          <p class="muted">OAuth-Client vom Workspace-Admin eintragen (siehe usage.md). Wird in <code>chatty.toml</code> gespeichert.</p>
          <form onsubmit={saveClient}>
            <label class="field">Client-ID<input bind:value={clientId} placeholder="….apps.googleusercontent.com" /></label>
            <label class="field"
              >Client-Secret{#if chat.googleConfigured}<small class="muted"> (leer = unverändert)</small>{/if}<input
                type="password"
                bind:value={clientSecret}
                autocomplete="off"
              /></label
            >
            <button class="btn" disabled={busy}>Speichern</button>
            {#if chat.googleConfigured}
              <button type="button" class="btn secondary" onclick={() => (editClient = false)}>Abbrechen</button>
            {/if}
          </form>
        {/if}

        <h3>{reloginId ? "Mattermost – neu anmelden" : "Mattermost"}</h3>
        <form onsubmit={addMattermost}>
          <label class="field">Server-URL<input bind:value={server} required placeholder="https://mattermost.example.org" /></label>
          <label class="field">Benutzername oder E-Mail<input bind:value={login} required autocomplete="username" /></label>
          <label class="field">Passwort<input type="password" bind:value={password} required autocomplete="current-password" /></label>
          <label class="field">MFA-Code (falls aktiv)<input bind:value={mfa} inputmode="numeric" autocomplete="one-time-code" /></label>
          <button class="btn" disabled={busy}>Anmelden</button>
        </form>
      {:else}
        <label class="field">
          Optik
          <select bind:value={theme} onchange={saveSettings}>
            <option value="mattermost">Mattermost</option>
            <option value="slack">Slack</option>
            <option value="ehex">ehex</option>
            <option value="mirc">mIRC</option>
            <option value="dos">DOS (Norton Commander)</option>
          </select>
        </label>
        <label class="field">
          Schriftgröße (auch Strg + Plus/Minus/0)
          <select value={zoom.value} onchange={(e) => setZoom(Number(e.currentTarget.value))}>
            {#each ZOOMS as z (z)}<option value={z}>{Math.round(z * 100)} %</option>{/each}
          </select>
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={notifications} onchange={saveSettings} />
          Desktop-Benachrichtigungen bei Direktnachrichten und Erwähnungen
        </label>

        <h3>GIF-Suche (optional)</h3>
        <p class="muted">
          Aus, solange kein Anbieter eingetragen ist. Suchbegriffe und deine IP-Adresse gehen an den Anbieter; nur
          einschalten, wenn deine Organisation das erlaubt (Auftragsverarbeitung). Geeignet sind GIPHY-kompatible APIs,
          z. B. GIPHY oder heypster.
        </p>
        <form onsubmit={saveGif}>
          <label class="field"
            >API-URL<input bind:value={gifApiUrl} required placeholder="https://api.giphy.com/v1/gifs" /></label
          >
          <label class="field"
            >API-Schlüssel{#if chat.gifProvider}<small class="muted"> (leer = unverändert)</small>{/if}<input
              type="password"
              bind:value={gifApiKey}
              autocomplete="off"
            /></label
          >
          <button class="btn" disabled={busy}>Speichern</button>
          {#if chat.gifProvider}
            <button type="button" class="btn secondary" disabled={busy} onclick={disableGif}>Ausschalten</button>
          {/if}
        </form>
      {/if}
      {#if busy}<p class="muted">Bitte warten … (Google: Anmeldung im Browser abschließen)</p>{/if}
      {#if error}<p class="error">{error}</p>{/if}
    </div>
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
    width: 520px;
    max-height: 85vh;
    background: var(--center-bg);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 32px rgba(0, 0, 0, 0.2);
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid var(--border);
    padding: 0 12px;
  }
  nav button {
    border: none;
    background: none;
    padding: 14px 10px;
    border-bottom: 2px solid transparent;
    color: var(--text-muted);
  }
  nav button.active {
    color: var(--text);
    border-bottom-color: var(--accent);
    font-weight: 600;
  }
  .content {
    padding: 16px 24px 24px;
    overflow-y: auto;
  }
  h3 {
    margin: 12px 0 8px;
    font-size: 15px;
  }
  .account {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
  }
  .label {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  small,
  .muted {
    color: var(--text-muted);
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #aaa;
  }
  .dot.on {
    background: var(--online);
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .error {
    color: var(--error);
  }
</style>
