# chatty

Schlanker Desktop-Chatclient für **Google Chat** und **Mattermost**, in der Optik von Mattermost
oder Slack (umschaltbar). Ein Fenster, mehrere Konten, wenig Speicher.

Gebaut mit [Tauri 2](https://tauri.app) (Rust) und Svelte 5. Läuft auf Linux, Windows und macOS.

## Funktionen

- Konten: Google Chat (OAuth-Login im Browser) und Mattermost (Benutzer/Passwort, MFA), beliebig viele
- Spaces, Kanäle, Direkt- und Gruppennachrichten mit Ungelesen- und Erwähnungsanzeige
- Lesen, Senden, Bearbeiten, Löschen, Threads (rechte Seitenleiste), Emoji-Reaktionen
- Dateianhänge hoch- und herunterladen
- Serverseitige Volltextsuche
- Popups: Desktop-Benachrichtigungen bei DMs und Erwähnungen, Hinweise im Fenster,
  Chats in eigenem Fenster (⧉)
- Optik Mattermost oder Slack
- Signierte automatische Updates über GitHub Releases

Bedienung und Einrichtung: [usage.md](usage.md). Architektur und Entscheidungen:
[adr/00_plan.md](adr/00_plan.md), [konzept/](konzept/README.md).

## Wie es funktioniert

| | Google Chat | Mattermost |
|---|---|---|
| Login | OAuth 2.0, Loopback-Redirect + PKCE | `POST /api/v4/users/login` |
| Neue Nachrichten | Polling (offener Space 5 s, Space-Liste 20 s) | WebSocket |
| Suche | `spaces.messages.search` | `/teams/{id}/posts/search` |

Tokens liegen im Schlüsselbund des Betriebssystems, Nachrichten zwischengespeichert in einer lokalen
SQLite-Datei (höchstens 500 je Space).

## Entwickeln

Voraussetzungen: Rust (stable), Node.js 22+, unter Linux zusätzlich:

```sh
sudo apt install libwebkit2gtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev libdbus-1-dev build-essential pkg-config
```

```sh
npm install
npm run tauri dev        # App mit Hot Reload starten
npm run check            # Svelte/TypeScript prüfen
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
npm run tauri build      # Installer für das eigene System
```

Projektaufbau:

```
src/                    Svelte-Frontend (Komponenten, Themes in app.css)
src-tauri/src/
  app.rs                Zustand, Ereignisschleife, Tauri-Kommandos, Benachrichtigungen
  backend/mod.rs        Trait ChatBackend – Schnittstelle für jeden Chatdienst
  backend/google/       Google Chat
  backend/mattermost/   Mattermost
  store.rs              SQLite-Cache
  secrets.rs            OS-Schlüsselbund
  config.rs             chatty.toml, accounts.json
```

Weitere Dienste (z. B. Slack) kommen als neues Modul unter `backend/` dazu, das `ChatBackend`
implementiert.

## Release

Tag `vX.Y.Z` pushen → GitHub Actions baut Installer (deb, AppImage, rpm, msi/exe, dmg) und
`latest.json` als Release-Entwurf. Nach Veröffentlichen des Entwurfs holen sich bestehende
Installationen das Update. Einrichtung des Signaturschlüssels: [usage.md](usage.md#releases-signieren).

## Lizenz

[Apache License 2.0](LICENSE.txt)
