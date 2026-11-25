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

### Native Variante `chatty-native`

Gleiche Funktionen ohne WebView (deutlich weniger Speicher, [ADR 08](adr/08_native_gui.md)). Dateien im
selben Release, Namen beginnen mit `chatty-native`:

| System | Datei |
|---|---|
| Linux | `chatty-native_…_amd64.deb` oder `chatty-native-linux-x86_64.AppImage` |
| Windows | `chatty-native_…-setup.exe` (Installation ohne Administratorrechte) |
| macOS | `chatty-native_….dmg` (Apple Silicon) bzw. `…-x86_64.dmg` (Intel) |

Beide Varianten teilen Konten, Schlüsselbund, Cache und `chatty.toml`. Selbst-Update wie oben
(Windows, macOS, Linux nur als AppImage; die deb aktualisiert die Paketverwaltung).

## Konto hinzufügen

Beim ersten Start öffnet sich der Dialog **Konto hinzufügen**; später über **+** in der linken Leiste.

### Mattermost

Server-URL (z. B. `https://mattermost.example.org`), Benutzername oder E-Mail und Passwort eingeben.
Nur `https://` wird angenommen (`http://` allein für `localhost`, z. B. einen Testserver), damit Passwort und
Sitzungs-Token nicht im Klartext übers Netz gehen.
Ist Zwei-Faktor-Anmeldung aktiv, zusätzlich den aktuellen MFA-Code. chatty speichert danach nur das
Sitzungs-Token im Schlüsselbund, nicht das Passwort. Läuft die Sitzung ab, zeigt das Konto „getrennt“;
unter **Konten → Neu anmelden** erneut einloggen.

### Google Chat

Google Chat braucht einen OAuth-Client der eigenen Organisation. Den legt einmalig ein
Google-Workspace- bzw. Cloud-Admin an: [konzept/google-setup.md](konzept/google-setup.md).
Der Admin verteilt `client_id` und `client_secret`; jeder Nutzer trägt sie unter
**Einstellungen → Konto hinzufügen → Google Chat** ein. chatty speichert sie in `chatty.toml`
(unter Linux/macOS nur für den Nutzer lesbar). Alternativ direkt in die Datei:

```toml
[google]
client_id = "123456789-abc.apps.googleusercontent.com"
client_secret = "GOCSPX-..."
```

Danach **Mit Google anmelden** (nach Bearbeiten der Datei von Hand vorher chatty neu starten). Der Browser öffnet die Google-Anmeldung;
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
theme = "mattermost"      # oder "slack", "ehex", "mirc", "dos", "seequ"
notifications = true      # Desktop-Benachrichtigungen
# sound = true            # Ton bei DMs und Erwähnungen; ohne Eintrag nur bei "seequ" an (nur Tauri-Variante)

[google]
client_id = "..."
client_secret = "..."

[gif]                     # GIF-Suche, optional; ohne diesen Abschnitt aus
api_url = "https://api.giphy.com/v1/gifs"   # GIPHY-kompatible API (z. B. GIPHY, heypster), nur https
api_key = "..."

[poll]                    # nur Google Chat
active_secs = 5           # offener Space, solange ein chatty-Fenster den Fokus hat
                          # (ohne Fokus läuft er im Takt der Space-Liste mit)
spaces_secs = 20          # Space-Liste / Ungelesen
```

Die GIF-Suche schickt Suchbegriffe und die IP-Adresse an den eingetragenen Anbieter. Schalte sie nur ein,
wenn deine Organisation das erlaubt (Auftragsverarbeitung). Einrichten auch unter **+ → Darstellung**.

Optik und Benachrichtigungen lassen sich auch unter **+ → Darstellung** ändern, dort auch die Schriftgröße
(nicht in `chatty.toml`: Tauri merkt sie in der WebView, native in `native.json`). Längere
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
| Kanal beitreten (Mattermost) | Namen ins Filterfeld tippen → „Weitere Kanäle“ → anklicken |
| Seitenleiste breiter/schmaler | rechten Rand ziehen, Doppelklick = Standardbreite |
| Sortieren | Auswahl neben dem Filterfeld: Neueste (letzte Nachricht), Name, Meiste (Anzahl Nachrichten) |
| Favorit | ☆ beim Überfahren des Eintrags, oder Eintrag in „Favoriten“ ziehen; dort per Ziehen ordnen, herausziehen entfernt |
| Stumm schalten | 🔔 beim Überfahren; stumme Kanäle grau, Benachrichtigung nur bei Erwähnung |
| Emojis | `:name:` im Text wird zum Emoji (volle Mattermost-Liste); eigene Server-Emojis erscheinen als Bild, auch in Reaktionen; Status-Emoji der Person steht neben der Direktnachricht. Emojis sind etwas größer als der Text, Nachrichten nur aus Emojis (bis 8–10) groß |
| Schriftgröße | **+ → Darstellung → Schriftgröße** (80–200 %) oder Strg + Plus/Minus, Strg + 0 = 100 %; gilt je Rechner für die ganze Oberfläche |
| Kanal-Header (Mattermost) | erste Zeile unter dem Kanalnamen, Links anklickbar; ganzer Text als Tooltip (Tauri) |
| Zur neuesten Nachricht | Knopf ↓ unten rechts, sobald hochgescrollt |
| Alte Direktnachrichten | ohne Nachricht seit 90 Tagen ausgeblendet; über das Filterfeld weiter auffindbar |
| Senden | Enter; neue Zeile mit Shift+Enter |
| Letzte eigene Nachricht bearbeiten | ↑ im leeren Eingabefeld |
| Nachricht bearbeiten/löschen | Maus über die Nachricht → ✎ / 🗑 |
| Reaktion | Maus über die Nachricht → ☺, oder vorhandene Reaktion anklicken (an/aus) |
| Thread | ↩ an der Nachricht oder „n Antworten“ → rechte Seitenleiste |
| Datei senden | 📎 im Eingabefeld |
| Bilder | angehängte Bilder (PNG, JPEG, GIF, WebP) stehen als Vorschau im Verlauf; Klick speichert das Original. Mattermost liefert eine verkleinerte Fassung, Google das Original (bis 10 MiB, größere nur als Datei) |
| Datei laden | Anhang anklicken → Speicherort wählen |
| Bild einfügen | Bildschirmfoto oder kopiertes Bild mit Strg+V ins Eingabefeld: wird als Anhang `bild-<Zeit>.png` vorgemerkt, mit Vorschaubild über dem Feld (auch für per 📎 gewählte Bilder). Enthält die Zwischenablage auch Text, wird der Text eingefügt |
| Vorschau | 👁 unter dem Eingabefeld zeigt den Text formatiert (fett, Links, Emojis), wie er gesendet erscheint |
| Text kopieren | Tauri: Text mit der Maus markieren, Strg+C; ⎘ beim Überfahren kopiert die ganze Nachricht. Native: ⎘ öffnet die Nachricht zum Markieren (Maus, Shift+Pfeile, Strg+A), Strg+C oder „Kopieren“ übernimmt die Auswahl, „Fertig“ oder Esc beendet |
| Suchen | Suchfeld oben rechts, Enter; Treffer anklicken springt in den Space |
| Chat in eigenem Fenster | ⧉ oben rechts |
| Ältere Nachrichten | nach oben scrollen |

Formatierung: `**fett**` bzw. `*fett*`, `_kursiv_`, `~durchgestrichen~`, `` `Code` ``,
` ```Block``` `, Links (auch `[Text](https://…)`) werden klickbar.

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
- Mattermost: Thread-Antworten erscheinen auch im Kanalverlauf (Balken links); der Thread-Bereich rechts zeigt
  sie gesammelt. Google Chat zeigt Antworten nur im Thread-Bereich.
- Slack als Backend gibt es noch nicht (nur die Optik).
- `chatty-native`: Favoriten nicht per Ziehen (Favorit/Stumm per Knopf beim Überfahren), keine abgelösten
  Fenster; eigene Server-Emojis nur in Reaktionen als Bild, im Text als `:name:`
  ([Abgleich](konzept/drift-native-tauri.md)).

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
anheben, committen, Tag `vX.Y.Z` pushen. Der Workflow veröffentlicht den Release automatisch, wenn alle
Plattform-Builds erfolgreich waren; scheitert einer, bleibt ein Entwurf stehen.
