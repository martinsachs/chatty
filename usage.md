# chatty benutzen

## Installieren

Installer der passenden Plattform aus den
[GitHub Releases](https://github.com/martinsachs/chatty/releases) laden:

| System | Datei |
|---|---|
| Linux | `.deb` (Debian/Ubuntu), `.rpm` oder `.AppImage` |
| Windows | `.msi` oder `-setup.exe` |
| macOS | `.dmg` (Universal: Intel und Apple Silicon) |

Die Installer sind nicht mit einem Hersteller-Zertifikat signiert:

- **Windows:** SmartScreen warnt → „Weitere Informationen“ → „Trotzdem ausführen“.
- **macOS:** Gatekeeper blockiert → im Finder Rechtsklick auf chatty → „Öffnen“, oder
  `xattr -d com.apple.quarantine /Applications/chatty.app`.

Updates kommen danach automatisch: Beim Start erscheint ein Hinweis „Version … verfügbar“,
ein Klick auf **Installieren** lädt das (signiert geprüfte) Update und startet neu.

## Konto hinzufügen

Beim ersten Start öffnet sich der Dialog **Konto hinzufügen**; später über **+** in der linken Leiste.

### Mattermost

Server-URL (z. B. `https://mattermost.example.org`), Benutzername oder E-Mail und Passwort eingeben.
Ist Zwei-Faktor-Anmeldung aktiv, zusätzlich den aktuellen MFA-Code. chatty speichert danach nur das
Sitzungs-Token im Schlüsselbund, nicht das Passwort. Läuft die Sitzung ab, zeigt das Konto „getrennt“;
unter **Konten → Neu anmelden** erneut einloggen.

### Google Chat

Google Chat braucht einen OAuth-Client der eigenen Organisation. Den legt einmalig ein
Google-Workspace- bzw. Cloud-Admin an: [konzept/google-setup.md](konzept/google-setup.md).
Der Admin verteilt `client_id` und `client_secret`; jeder Nutzer trägt sie in `chatty.toml` ein:

```toml
[google]
client_id = "123456789-abc.apps.googleusercontent.com"
client_secret = "GOCSPX-..."
```

Danach chatty neu starten, **+ → Mit Google anmelden**. Der Browser öffnet die Google-Anmeldung;
nach Zustimmung meldet die Seite „Anmeldung abgeschlossen“ und das Konto erscheint links.

Das `client_secret` eines Desktop-Clients ist laut Google nicht vertraulich (es steckt in jeder
Installation). Es gibt allein keinen Zugriff auf Daten, dafür ist immer die Anmeldung des Nutzers nötig.

## Konfigurationsdatei `chatty.toml`

| System | Ort |
|---|---|
| Linux | `~/.config/chatty/chatty.toml` |
| Windows | `%APPDATA%\chatty\chatty.toml` |
| macOS | `~/Library/Application Support/chatty/chatty.toml` |

Alle Einträge sind optional:

```toml
theme = "mattermost"      # oder "slack"
notifications = true      # Desktop-Benachrichtigungen

[google]
client_id = "..."
client_secret = "..."

[poll]                    # nur Google Chat
active_secs = 5           # offener Space
spaces_secs = 20          # Space-Liste / Ungelesen
```

Optik und Benachrichtigungen lassen sich auch unter **+ → Darstellung** ändern. Längere
Polling-Intervalle sparen API-Kontingent: Das Google-Cloud-Projekt hat ein gemeinsames Limit
(3000 Lesezugriffe pro Minute) für alle Nutzer.

Weitere Dateien im selben Ordner: `accounts.json` (Kontenliste, ohne Geheimnisse). Der
Nachrichten-Cache liegt unter `~/.local/share/chatty/cache.sqlite` (Windows `%APPDATA%\chatty`,
macOS `~/Library/Application Support/chatty`). Er darf gelöscht werden.

## Bedienung

| Aktion | So geht's |
|---|---|
| Konto wechseln | Kürzel in der linken Leiste |
| Kanal öffnen | Eintrag in der Seitenleiste, oben filtern |
| Senden | Enter; neue Zeile mit Shift+Enter |
| Letzte eigene Nachricht bearbeiten | ↑ im leeren Eingabefeld |
| Nachricht bearbeiten/löschen | Maus über die Nachricht → ✎ / 🗑 |
| Reaktion | Maus über die Nachricht → ☺, oder vorhandene Reaktion anklicken (an/aus) |
| Thread | ↩ an der Nachricht oder „n Antworten“ → rechte Seitenleiste |
| Datei senden | 📎 im Eingabefeld |
| Datei laden | Anhang anklicken → Speicherort wählen |
| Suchen | Suchfeld oben rechts, Enter; Treffer anklicken springt in den Space |
| Chat in eigenem Fenster | ⧉ oben rechts |
| Ältere Nachrichten | nach oben scrollen |

Formatierung: `**fett**` bzw. `*fett*`, `_kursiv_`, `~durchgestrichen~`, `` `Code` ``,
` ```Block``` `, Links werden klickbar.

### Popups

- **Desktop-Benachrichtigung** bei Direktnachrichten und Erwähnungen, wenn der Chat nicht gerade
  sichtbar ist.
- **Hinweis im Fenster** (unten rechts) mit **Öffnen**-Knopf, ebenso bei Fehlern.
- **Eigenes Fenster** je Chat über ⧉.

## Grenzen

- Google Chat: Neue Nachrichten kommen per Polling (bis zu 5 s Verzögerung im offenen Space,
  20 s in anderen). Erwähnungszähler kennt die API nicht; DMs zählen als Erwähnung.
- Google Chat: Die Suche findet keine Nachrichten aus stummgeschalteten Spaces und von Chat-Apps.
  Ob eine Reaktion von dir stammt, liefert die API nicht – Klick auf eine Reaktion fügt sie hinzu
  bzw. entfernt deine eigene.
- Mattermost: Emoji-Reaktionen werden für die gängigen Emoji in Unicode übersetzt, andere als `:name:`
  angezeigt.
- Slack als Backend gibt es noch nicht (nur die Optik).

## Konto entfernen

**+ → Konten → Entfernen** löscht Token und Cache des Kontos. Die Anmeldung beim Dienst selbst
(z. B. Google-Freigabe) widerrufen: <https://myaccount.google.com/permissions>.

## Releases signieren

Nur für Maintainer. Einmalig einen Updater-Schlüssel erzeugen:

```sh
npm run tauri signer generate -- -w ~/.tauri/chatty.key
```

- Inhalt von `~/.tauri/chatty.key.pub` in `src-tauri/tauri.conf.json` unter
  `plugins.updater.pubkey` eintragen und committen.
- In GitHub unter *Settings → Secrets and variables → Actions* die Secrets
  `TAURI_SIGNING_PRIVATE_KEY` (Inhalt von `~/.tauri/chatty.key`) und
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` anlegen.
- Den privaten Schlüssel sicher aufbewahren: ohne ihn können bestehende Installationen keine
  Updates mehr annehmen.

Release: Version in `package.json`, `src-tauri/Cargo.toml` und `src-tauri/tauri.conf.json`
anheben, committen, Tag `vX.Y.Z` pushen, den Release-Entwurf auf GitHub prüfen und veröffentlichen.
