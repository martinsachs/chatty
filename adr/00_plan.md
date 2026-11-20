# Plan: chatty-tool – serverunabhängiger Desktop-Chatclient

## Kontext
Martin will einen eigenen, schlanken Chatclient, der nicht an einen Anbieter gebunden ist. Backends in v1:
Google Chat (Firmen-Workspace, OAuth-Login über Google) und Mattermost (frei eintragbarer Server).
Optik wahlweise Mattermost oder Slack, zur Laufzeit umschaltbar. Ziele: performant, wenig RAM,
Popups (OS-Notifications, In-App-Toasts, abgelöste Chatfenster). Slack als Backend später.
Prinzipien: YAGNI, KISS. Verzeichnis `~/workspace/martin.sachs/chatty` ist leer (nur `.harness/`).
Repo: `git@github.com:martinsachs/chatty.git` (öffentlich).

## Entscheidungen (vom Nutzer)
| Thema | Entscheidung |
|---|---|
| Technik | Tauri 2 (Rust) + Svelte + TypeScript |
| OS | Linux, Windows, macOS |
| Backends v1 | Google Chat + Mattermost; Slack später (Trait muss passen, kein Code vorab) |
| Optik | Mattermost- und Slack-Theme, umschaltbar in v1 |
| Funktionen v1 | Spaces/DMs listen, lesen, senden; Threads; Reaktionen; Bearbeiten/Löschen; Anhänge; Suche; Ungelesen |
| Echtzeit Google | Polling (kein Pub/Sub: Mitlese-Risiko bei Selbstanlage) |
| Echtzeit Mattermost | WebSocket `/api/v4/websocket` (nativ vorhanden) |
| Suche | Server-Suche: Google `spaces.messages.search` (`spaces/-`), Mattermost `/posts/search` |
| Speicher | Tokens im OS-Keyring, Nachrichten-Cache in SQLite (kein FTS) |
| Google-OAuth-Client | legt Workspace-/GCP-Admin an; Client-ID/Secret per Konfigurationsdatei je Installation |
| Mattermost-Login | Benutzer/Passwort (+MFA-Code), Session-Token im Keyring |
| Nutzerkreis | Team/Firma |
| Repo/CI | GitHub (öffentlich) + Actions, `tauri-action` Matrix Linux/Windows/macOS |
| Auslieferung | Tauri-Updater mit Signatur, Feed `latest.json` über GitHub Releases; keine OS-Codesignierung |

## Architektur

```
src-tauri/            Rust-Kern
  src/backend/mod.rs        Trait ChatBackend + gemeinsame Typen (Space, Message, Thread, Reaction, User, Attachment)
  src/backend/google/       REST-Client Chat API, OAuth (Loopback + PKCE), Poller
  src/backend/mattermost/   REST v4 + WebSocket
  src/store.rs              SQLite-Cache (rusqlite, bundled): spaces, messages, users
  src/secrets.rs            keyring-Crate (Secret Service / Credential Manager / Keychain)
  src/config.rs             chatty.toml im OS-Config-Verzeichnis (dirs-Crate)
  src/events.rs             Backend-Ereignisse → Tauri-Events ans Frontend, Notifications
src/                  Svelte-Frontend
  lib/components/           Sidebar, ChannelHeader, MessageList (virtualisiert), Composer, ThreadPanel (RHS), SearchPanel, Toasts
  lib/themes/               mattermost.css, slack.css (CSS-Variablen + wenige Layout-Klassen)
  lib/stores/               Svelte-Stores je Konto/Space
konzept/              Doku (README-Index, google-setup.md für Admin, architektur.md)
.github/workflows/    build.yml (PR), release.yml (Tag → Installer + latest.json)
```

**ChatBackend-Trait** (async, eine Instanz je Konto):
`login/restore`, `list_spaces`, `list_messages(space, cursor)`, `list_thread(space, thread)`,
`send(space, text, thread?)`, `edit`, `delete`, `react/unreact`, `upload/download_attachment`,
`search(query)`, `mark_read(space)`, `subscribe() -> Stream<BackendEvent>`.
Polling (Google) und WebSocket (Mattermost) verstecken sich hinter `subscribe()`; das Frontend kennt nur Events.

**RAM/Performance:** ein Prozess, Frontend ohne schwere Libs, virtualisierte Nachrichtenliste,
im RAM nur sichtbare Spaces, Rest aus SQLite nachladen; HTTP über ein gemeinsames `reqwest`-Client-Objekt.

### Google Chat
- OAuth: System-Browser öffnen, Loopback `http://127.0.0.1:<zufallsport>`, PKCE S256, Refresh-Token in Keyring.
- Scopes: `chat.spaces.readonly`, `chat.messages`, `chat.memberships.readonly`, `chat.users.readstate`,
  `directory.readonly` (Namensauflösung über People API, wo Chat nur IDs liefert – noch zu verifizieren).
- Polling: `spaces.list` (Feld `lastActiveTime`) im Intervall → geänderte Spaces → `messages.list` mit
  `createTime >`-Filter; offener Space häufiger. Ungelesen über `getSpaceReadState`, `mark_read` über `updateSpaceReadState`.
- Threads: `messages.create` mit `thread.name` + `REPLY_MESSAGE_FALLBACK_TO_NEW_THREAD`.
- Grenzen beachten: je Space 15 Lese-/1 Schreibzugriff pro Sekunde, Projekt 3000 Lesezugriffe/min (teamweit!) → Backoff bei 429.

### Mattermost
- Login `POST /api/v4/users/login` (login_id, password, token=MFA) → Session-Token in Keyring.
- REST v4 für Kanäle/Posts/Threads/Reaktionen/Dateien/Suche, WebSocket für Ereignisse (posted, post_edited, reaction_added …), automatischer Reconnect.

### Popups
- OS-Notifications: `tauri-plugin-notification`, bei DM/Erwähnung; Klick fokussiert Space.
- In-App-Toasts: Svelte-Komponente.
- Abgelöste Chatfenster: zusätzliches `WebviewWindow` mit Route `/#/space/<id>` (gleicher Rust-Kern, Events geteilt).

### Konfiguration
`chatty.toml` (OS-Config-Verzeichnis): `[google] client_id, client_secret`; Theme, Polling-Intervalle.
Konten (Mattermost-URLs, Google-Konto) im Client hinzufügbar. Repo enthält keine Firmenwerte, keine
E-Mail-Adressen, keine Secrets (öffentlich!).

### Updater/CI
- `tauri-plugin-updater`, Endpunkt `https://github.com/martinsachs/chatty/releases/latest/download/latest.json`.
- Signaturschlüssel: `tauri signer generate` durch Martin lokal; privater Schlüssel nur als GitHub-Secret
  (`TAURI_SIGNING_PRIVATE_KEY`), nie im Repo/Ausgaben. Public Key in `tauri.conf.json`.
- Workflows: PR → `cargo clippy`, `cargo test`, `svelte-check`, Build Linux; Tag → `tauri-action` Matrix als Entwurf, nach allen Builds veröffentlicht.
- Tags pusht Martin.

## Meilensteine
1. **Gerüst:** `npm create tauri-app` (Svelte-TS), Repo-Init mit Remote, CI-Build, `konzept/`.
2. **Kern:** Trait, Typen, SQLite-Store, Keyring, Config, Event-Bus.
3. **UI-Rahmen:** Layout + beide Themes umschaltbar, mit Fake-Backend (nur für Tests/Entwicklung).
4. **Mattermost-Backend** komplett (testbar ohne Admin-Abhängigkeit).
5. **Google-Backend:** OAuth, Spaces/Nachrichten/Threads/Reaktionen/Anhänge/Suche/Ungelesen, Poller.
   Voraussetzung: Admin hat GCP-Projekt nach `konzept/google-setup.md` angelegt.
6. **Popups:** Notifications, Toasts, abgelöste Fenster.
7. **Release:** Updater, Release-Workflow, erste Version.

`konzept/google-setup.md` (Admin-Anleitung): GCP-Projekt, Chat API + Chat-App-Konfiguration (Name/Icon),
Consent-Screen „Intern“, Scopes, OAuth-Client Typ „Desktop“, People API aktivieren; Werte an Nutzer verteilen.

## Verifikation
- Rust: `cargo test` (Trait-Tests gegen Fake-Backend, Store, Config), `cargo clippy -D warnings`.
- Frontend: `svelte-check`, Komponenten manuell per `npm run tauri dev`.
- Mattermost: gegen lokalen Docker-Container `mattermost/mattermost-preview` (Login, Senden, Thread, Reaktion, Datei, Suche, WebSocket-Event).
- Google: manueller E2E-Test mit Martins Konto, sobald Admin-Client existiert (Login-Flow, Nachricht in Test-Space, Polling-Latenz, Ungelesen).
- RAM: `ps -o rss` nach 10 min Betrieb mit beiden Konten; Ziel festlegen (siehe offene Punkte).
- CI: PR-Build grün auf GitHub; Release-Build erzeugt deb/AppImage, msi, dmg + `latest.json`; Update von v0.1.0 → v0.1.1 lokal prüfen.

## Offene Punkte (vor/bei Umsetzung bestätigen)
- Polling-Intervalle Google (Vorschlag: offener Space 5 s, Space-Liste 20 s; konfigurierbar) – passt bei Teamgröße zur Projektquote?
- RAM-Zielwert (Vorschlag < 150 MB mit 2 Konten).
- Namensauflösung via `directory.readonly` noch unverifiziert; Fallback: Namen aus Memberships/Message-Sendern.
- Mattermost-Testserver für E2E: lokaler Docker-Container ok oder echte Instanz?

## Umsetzungsstand und Abweichungen (2026-11-20)
- Offene Punkte ohne Rückmeldung mit den Vorschlagswerten umgesetzt, alle in `chatty.toml` änderbar:
  Polling 5 s / 20 s. RAM-Ziel < 150 MB gilt als Messziel, nicht als harte Grenze.
- Kein Fake-Backend (YAGNI): UI-Tests laufen gegen einen lokalen Mattermost-Container.
- Nachrichtenliste ohne Virtualisierungs-Bibliothek: höchstens 500 Nachrichten je Space im Cache,
  `content-visibility: auto` je Nachricht hält das Rendern billig.
- Abgelöste Fenster per Hash-Route `#popout/<konto>/<space>`.
- `keyring` 3.x statt 4.x (bekannte, stabile API).
- Klick auf eine Desktop-Benachrichtigung fokussiert den Space nicht (vom Notification-Plugin auf
  dem Desktop nicht unterstützt); der In-App-Hinweis hat dafür einen „Öffnen“-Knopf.
- Updater-Schlüssel von Martin erzeugt (2026-11-20, `usage.md` → Releases signieren): öffentlicher
  Schlüssel in `tauri.conf.json`, privater Schlüssel und Passwort nur als GitHub-Secrets.

### Getestet (2026-11-20)
- Rust: 35 Unit-Tests (Kern, Google gegen JSON-Fixtures und lokalen HTTP-Mock, Mattermost-Mapping),
  `cargo clippy -D warnings` sauber; GitHub Actions `build` grün.
- Mattermost: ignorierter Integrationstest `backend/mattermost/it.rs` gegen lokalen
  `mattermost/mattermost-preview` (Login, Spaces über zwei Teams, Senden mit Anhang, Thread, Bearbeiten,
  Reaktionen, Suche, Paging, Löschen, WebSocket-Ereignisse, Reconnect nach Server-Neustart).
- UI (Mattermost-Container): Verlauf, Formatierung, Thread-Zähler, Reaktionen, Live-Nachricht,
  Erwähnung, Ungelesen-Badge, In-App-Hinweis, beide Optiken.
- Mattermost-Profilbilder über das Schema `avatar://` (Backend lädt mit Token), Linux.
- Mattermost: Seitenleiste nur mit Mitgliedschaften; Kanalsuche im Filterfeld mit Beitreten
  (Integrationstest; UI-Eingabe nicht automatisiert geprüft). Google: Beitreten nicht unterstützt
  (die Chat API bietet Nutzern keine Suche nach fremden Spaces).
- RAM (Linux, Release-Build, WebKitGTK, 1 Mattermost-Konto, nach 60 s): Summe PSS 241 MB
  (chatty 88, WebKitWebProcess 135, WebKitNetworkProcess 17), RSS 455 MB. Ziel < 150 MB verfehlt;
  der Großteil ist die WebView-Grundlast. Windows (WebView2) und macOS (WKWebView) nicht gemessen.

- Emojis (2026-11-20): volle Namensliste aus iamcal/emoji-data (`scripts/gen_emoji_table.py`,
  `emoji_table.rs`), Kurzcodes im Text ersetzt das Backend (nicht in Code). Eigene Server-Emojis über
  `avatar://<konto>/emoji/<name>`, Status-Emoji aus `props.customStatus` des DM-Partners, aktualisiert per
  WebSocket `user_updated`. Integrationstest: Kurzcode und Status grün; Seitenleiste mit Status per
  Screenshot geprüft; Server-Emoji-Bild im Text nur über `formatText` geprüft, nicht im Fenster.

### Nicht getestet / offen
- Google Chat live: kein OAuth-Client vorhanden. Unbestätigt: `orderBy` in Kleinschreibung,
  Upload als `multipart/related`, GET-Pfad des Lesestatus, `users/all` für @all, Scope-Namen im Detail.
- Google: `reply_count` zählt nur Antworten auf derselben 50er-Seite; erste Space-Liste fragt den
  Lesestatus je Space einmal ab (Quote).
- Mattermost: `wss://` gegen echten TLS-Server, MFA-Login, Gruppen-DMs; `reply_count` nur clientseitig
  hochgezählt. Neues Profilbild erst nach Neustart (Bilder bleiben je Sitzung im Speicher).
- Windows/macOS-Installer und Updater-Ablauf erst mit dem ersten Tag-Build.
- rustls: ring und aws-lc-rs sind beide im Baum; WebSocket läuft deshalb über reqwests TLS.
- Echter Server (hinter Proxy) kappte die WebSocket-Verbindung alle 30–90 s ohne Verkehr. Client pingt
  jetzt alle 20 s (`{"action":"ping"}` wie die Webapp), Abrisse werden still neu verbunden, danach lädt
  der offene Space verpasste Nachrichten nach. 5 min gegen den echten Server: vorher 3 Abrisse, danach keiner.
  `tokio_tungstenite::connect_async` mit wss nicht verwenden (Panic ohne Default-Provider).
