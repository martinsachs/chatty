# Google Chat: OAuth-Client für chatty einrichten (Admin-Anleitung)

chatty greift mit dem Konto des jeweiligen Nutzers (Nutzer-Authentifizierung, OAuth 2.0) auf die
Google Chat API zu. Dafür braucht es **einmal je Organisation** ein Google-Cloud-Projekt mit einem
OAuth-Client vom Typ „Desktop-App“. Die Schritte erledigt ein Workspace- bzw. GCP-Admin.

Nicht nötig: Pub/Sub, Service-Konto, domänenweite Delegierung. chatty fragt per Polling ab.

In chatty öffnet **ⓘ** neben „Google Chat“ (Konto hinzufügen) eine Kurzfassung mit Direktlinks.

## 1. Projekt anlegen

1. <https://console.cloud.google.com/projectcreate> (bzw. Projektauswahl → **Neues Projekt**).
2. Name frei wählen (z. B. `chatty`), Organisation des Workspace auswählen.

## 2. APIs aktivieren

**APIs & Dienste → Bibliothek**, nacheinander aktivieren:

- **Google Chat API**: <https://console.cloud.google.com/apis/library/chat.googleapis.com>
- **People API** (eigener Name, Namen anderer Nutzer in Direktnachrichten):
  <https://console.cloud.google.com/apis/library/people.googleapis.com>

## 3. Chat-App konfigurieren

Auch bei reiner Nutzer-Authentifizierung verlangt die Chat API eine konfigurierte Chat-App,
sonst antworten alle Aufrufe mit Fehler 404/403.

**APIs & Dienste → Google Chat API → Konfiguration**
(<https://console.cloud.google.com/apis/api/chat.googleapis.com/hangouts-chat>):

- **App-Name**: z. B. `chatty`
- **Avatar-URL**: beliebiges öffentlich erreichbares PNG/JPG (Pflichtfeld)
- **Beschreibung**: z. B. `Desktop-Client`
- **Interaktive Funktionen**: deaktivieren (chatty ist kein Bot)
- Sichtbarkeit: nicht nötig, solange interaktive Funktionen aus sind
- Speichern.

## 4. OAuth-Zustimmungsbildschirm

**APIs & Dienste → OAuth-Zustimmungsbildschirm** (neuere Konsole: **Google Auth Platform → Branding /
Zielgruppe / Datenzugriff**: <https://console.cloud.google.com/auth/branding>,
<https://console.cloud.google.com/auth/audience>, <https://console.cloud.google.com/auth/scopes>):

- **Nutzertyp: Intern** – nur Konten der eigenen Workspace-Organisation, keine Google-Prüfung nötig.
- App-Name, Support-E-Mail, Kontakt-E-Mail des Entwicklers ausfüllen.
- **Bereiche (Scopes)** hinzufügen, genau diese:

| Scope | Zweck |
|---|---|
| `https://www.googleapis.com/auth/chat.spaces.readonly` | Spaces/DMs auflisten |
| `https://www.googleapis.com/auth/chat.messages` | Nachrichten lesen, senden, bearbeiten, löschen, Reaktionen, Anhänge, Suche |
| `https://www.googleapis.com/auth/chat.memberships.readonly` | Mitglieder von DMs/Gruppen (Anzeigenamen) |
| `https://www.googleapis.com/auth/chat.users.readstate` | Ungelesen-Status lesen und „gelesen“ setzen |
| `https://www.googleapis.com/auth/directory.readonly` | Namen von Kollegen über die People API |
| `https://www.googleapis.com/auth/userinfo.profile` | eigener Name |
| `https://www.googleapis.com/auth/userinfo.email` | eigene E-Mail als Kontobezeichnung |

Die Chat-Scopes sind als „eingeschränkt“ eingestuft; bei Nutzertyp *Intern* ist das ohne Prüfverfahren
erlaubt. Falls in der Admin-Konsole der API-Zugriff für Drittanbieter-Apps beschränkt ist: unter
**Sicherheit → API-Steuerung → App-Zugriff verwalten** die Client-ID von chatty als vertrauenswürdig
eintragen.

## 5. OAuth-Client anlegen

**APIs & Dienste → Anmeldedaten → Anmeldedaten erstellen → OAuth-Client-ID**
(<https://console.cloud.google.com/auth/clients/create>):

- **Anwendungstyp: Desktop-App**
- Name: z. B. `chatty desktop`
- Weiterleitungs-URIs sind bei Desktop-Apps nicht einzutragen; chatty nutzt
  `http://127.0.0.1:<zufälliger Port>` (Loopback) mit PKCE.

Ergebnis: **Client-ID** (`….apps.googleusercontent.com`) und **Clientschlüssel**. Bei Desktop-Apps gilt
der Clientschlüssel laut Google nicht als vertraulich; er wird trotzdem nicht ins öffentliche Repository
eingetragen, sondern an die Nutzer verteilt (z. B. Intranet, Softwareverteilung).

## 6. Eintrag beim Nutzer

Datei `chatty.toml` im Konfigurationsverzeichnis anlegen bzw. ergänzen:

| System | Verzeichnis |
|---|---|
| Linux | `~/.config/chatty/` |
| Windows | `%APPDATA%\chatty\` |
| macOS | `~/Library/Application Support/chatty/` |

```toml
[google]
client_id = "<Client-ID>"
client_secret = "<Clientschlüssel>"
```

Oder in chatty unter **Konto hinzufügen → Google Chat** eintragen; chatty schreibt dann die Datei.

Danach in chatty **Konto hinzufügen → Google**: Der Browser öffnet die Google-Anmeldung, nach der
Zustimmung zeigt er „Anmeldung abgeschlossen“. Das Refresh-Token liegt danach im Schlüsselbund des
Betriebssystems, nicht in der Konfigurationsdatei.

Optional die Abfrageintervalle (Sekunden):

```toml
[poll]
active_secs = 5    # offener Space
spaces_secs = 20   # Space-Liste / Ungelesen
```

## Kontingente

Die Chat API begrenzt u. a. (Stand der Google-Dokumentation, siehe
<https://developers.google.com/workspace/chat/limits>):

- **je Projekt ca. 3000 Lesezugriffe pro Minute** – gilt für alle chatty-Nutzer der Organisation
  zusammen, da alle denselben OAuth-Client nutzen;
- je Space 1 Schreibzugriff pro Sekunde und eine begrenzte Zahl Lesezugriffe pro Sekunde.

chatty braucht je laufendem Client mit den Standardintervallen grob 15 Lesezugriffe pro Minute
(offener Space alle 5 s, Space-Liste alle 20 s, Lesestatus nur für geänderte Spaces), dazu einmalig
beim Start je Space eine Abfrage des Lesestatus. Bei vielen gleichzeitigen Nutzern die Intervalle
erhöhen oder unter **APIs & Dienste → Google Chat API → Kontingente** eine Erhöhung beantragen.
Bei HTTP 429 wartet chatty mit wachsendem Abstand (bis 5 min) und meldet sich als offline.

## Widerruf

Nutzer: <https://myaccount.google.com/permissions> → chatty → Zugriff entfernen.
Admin: OAuth-Client löschen oder in der Admin-Konsole sperren; chatty meldet beim nächsten Start
„bitte neu anmelden“.
