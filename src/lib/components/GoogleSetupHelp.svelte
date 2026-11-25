<script lang="ts">
  // Kurzanleitung mit Direktlinks in die Google Cloud Console (ausführlich: konzept/google-setup.md).
  // Links öffnet +layout.svelte im Standardbrowser.
  import { chat } from "$lib/chat.svelte";

  const CONSOLE = "https://console.cloud.google.com";
  const GUIDE = "https://github.com/martinsachs/chatty/blob/main/konzept/google-setup.md";
  // Wie SCOPES in src-tauri/core/src/backend/google/oauth.rs.
  const SCOPES = [
    "https://www.googleapis.com/auth/chat.spaces.readonly",
    "https://www.googleapis.com/auth/chat.messages",
    "https://www.googleapis.com/auth/chat.memberships.readonly",
    "https://www.googleapis.com/auth/chat.users.readstate",
    "https://www.googleapis.com/auth/directory.readonly",
    "https://www.googleapis.com/auth/userinfo.profile",
    "https://www.googleapis.com/auth/userinfo.email",
  ];

  let open = $state(false);

  async function copyScopes() {
    try {
      await navigator.clipboard.writeText(SCOPES.join("\n"));
      chat.toast("Scopes kopiert", "info");
    } catch (e) {
      chat.toast(`Kopieren fehlgeschlagen: ${e}`, "error");
    }
  }
</script>

<button
  type="button"
  class="info"
  title="OAuth-Client einrichten"
  aria-expanded={open}
  onclick={() => (open = !open)}>ⓘ</button
>

{#if open}
  <div class="help" role="note">
    <b>OAuth-Client einrichten (einmal je Organisation)</b>
    <ol>
      <li><a href="{CONSOLE}/projectcreate">Projekt anlegen</a> (Organisation des Workspace wählen)</li>
      <li>
        <a href="{CONSOLE}/apis/library/chat.googleapis.com">Chat API</a> und
        <a href="{CONSOLE}/apis/library/people.googleapis.com">People API</a> aktivieren
      </li>
      <li>
        <a href="{CONSOLE}/apis/api/chat.googleapis.com/hangouts-chat">Chat-App konfigurieren</a>: Name, Avatar-URL,
        interaktive Funktionen aus
      </li>
      <li>
        Zustimmung: <a href="{CONSOLE}/auth/branding">Branding</a>, <a href="{CONSOLE}/auth/audience">Zielgruppe „Intern“</a>,
        <a href="{CONSOLE}/auth/scopes">Datenzugriff</a>
        <button type="button" class="btn secondary small" onclick={copyScopes}>Scopes kopieren</button>
      </li>
      <li><a href="{CONSOLE}/auth/clients/create">Client anlegen</a>, Typ <b>Desktop-App</b>; ID und Secret unten eintragen</li>
      <li>
        Falls Drittanbieter-Apps gesperrt sind: Workspace-Admin gibt die Client-ID frei
        (<a href="https://admin.google.com/ac/owl/list?tab=configuredApps">App-Zugriff verwalten</a>)
      </li>
    </ol>
    <a href={GUIDE}>Ausführliche Anleitung</a>
  </div>
{/if}

<style>
  .info {
    border: none;
    background: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 15px;
    padding: 0 4px;
  }
  .info:hover,
  .info[aria-expanded="true"] {
    color: var(--accent);
  }
  .help {
    margin: 0 0 12px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--hover);
    font-size: 13px;
  }
  ol {
    margin: 6px 0;
    padding-left: 20px;
  }
  li {
    margin: 3px 0;
  }
  a {
    color: var(--link);
  }
  .small {
    padding: 1px 6px;
    font-size: 12px;
    margin-left: 4px;
  }
</style>
