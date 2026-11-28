# Drift: chatty-native gegenüber Tauri-Oberfläche (Stand 2026-11-21)

Auftrag Martin (2026-11-21): „das Fenster links lässt sich nicht vergrößern, Sortierung und Ausblendung wie beim
Tauri fehlen. Prüfe den Drift native vs. Tauri.“ Grundlage: Svelte-Komponenten (`src/lib/`) gegen
`src-tauri/native/src/`. Beide nutzen denselben Kern (`chatty_core::App`); Drift entsteht nur in der Oberfläche.

Status: **gleich** (schon vorher gleich), **angeglichen** (mit diesem Auftrag nachgezogen), **Abweichung** (bewusst
offen, mit Grund).

## Seitenleiste

| Funktion (Tauri) | native vorher | Status |
|---|---|---|
| Breite per Ziehen am rechten Rand (160–520 px, Doppelklick 240, gemerkt) | feste 260 px | angeglichen (`native.json`) |
| Bereiche Favoriten / Kanäle / Direktnachrichten mit Überschrift | eine Liste, Favoriten oben | angeglichen |
| Sortierung Neueste / Name / Meiste (gemerkt) | fest „Neueste“ | angeglichen |
| DMs ohne Nachricht seit 90 Tagen ausblenden (außer ungelesen, Favorit, offen, Filter) | fehlte | angeglichen |
| Filterfeld; bei Mattermost zusätzlich beitretbare Kanäle vom Server, „Beitreten“ | fehlte | angeglichen |
| Konten als Leiste mit Initialen, Erwähnungszähler je Konto, Offline-Punkt | Kontoname als Knopf, „(getrennt)“ | angeglichen (Zähler je Konto, „getrennt“) |
| Favorit/Stumm-Knöpfe beim Überfahren | gleich | gleich |
| Retro-Optiken (mIRC, DOS): ungelesen mit „*“ vor dem Kanal und Farbe (mIRC rot, DOS gelb), weil Festbreitenschriften kaum fett wirken (Auftrag 2026-11-21) | – | gleich (beide) |
| Status-Emoji, Erwähnungszähler, Fettdruck ungelesen, Stumm gedämpft | Status nur Unicode | gleich (eigene Server-Emojis als Status: Abweichung, nur Unicode) |
| Optik „seequ“ (Messenger 2001): DMs mit Status-Punkt, ungelesen grün blinkend (Auftrag 2026-11-24) | – | Abweichung: native nur Farben, ohne Status-Punkt |
| Online-Status (Mattermost): Punkt je DM, in seequ als Kleeblatt; Statusmenü im Kontokopf mit sieben Stufen (Zwischenstufen als Status-Text mit festem Emoji); Status der DM-Partner per Abfrage alle 60 s, eigener per WebSocket | – | Abweichung: native ohne Status-Anzeige und ohne Menü; `Space.presence` und `OwnPresence` liegen bereit |
| Passwort und Client-Secret mit Augen-Knopf zum Anzeigen (`PasswordInput.svelte`) | verdeckt, ohne Anzeigen | Abweichung: native ohne Augen-Knopf |
| Ton bei DMs und Erwähnungen: Kern entscheidet (`Alert.sound`, Einstellung `sound`, sonst an für `SOUND_THEMES`), Ton je Optik in `sound.ts` (seequ „Uh-oh“, sonst Ping) | – | Abweichung: native ohne Ton und ohne Schalter (keine Audio-Abhängigkeit); `Alert.sound` liegt bereit |
| Einstellungen: ⓘ neben „Google Chat“ mit Kurzanleitung und Direktlinks in die Google Cloud Console, „Scopes kopieren“ (Auftrag 2026-11-24) | – | Abweichung: native nur Link auf `konzept/google-setup.md` |
| Favoriten per Ziehen umsortieren bzw. in/aus Favoriten ziehen | fehlte | Abweichung: iced hat kein Drag&Drop; Favorit per Knopf |

## Kanal-Ansicht und Eingabe

| Funktion (Tauri) | native vorher | Status |
|---|---|---|
| Mattermost: Thread-Antworten auch im Hauptverlauf, links mit Balken markiert; Klick auf ↩ öffnet den Thread mit Startnachricht; Antwortzähler aus den geladenen Antworten (Auftrag 2026-11-21) | – | gleich (beide) |
| Mattermost: Kanal-Header (erste Zeile, Markdown mit klickbaren Links) unter dem Kanalnamen; Retro-Optiken einzeilig „Name: Header“; Änderung per WebSocket `channel_updated` (Auftrag 2026-11-23) | – | gleich (beide; Tooltip mit ganzem Header nur Tauri) |
| Bildvorschau angehängter Bilder (max. 400×300, Klick speichert); Mattermost `/files/{id}/preview`, Google Original bis 10 MiB (Auftrag 2026-11-24) | – | gleich (beide; native dekodiert bis 4096×4096) |
| Schriftgröße 80–200 % in Darstellung und per Strg + Plus/Minus/0 (Auftrag 2026-11-24) | – | gleich (beide; Tauri WebView-Zoom, native `scale_factor`) |
| Text im Verlauf markieren und mit Strg+C kopieren, ⎘ kopiert die ganze Nachricht (Auftrag 2026-11-25) | – | gleich (beide; native über `iced_selection`, Auswahl je Absatz; über Nachrichten hinweg nur in der Textansicht „Als Text“, Strg+Umschalt+A) |
| Bild per Strg+V als Anhang mit Vorschaubild, auch für gewählte Bilder; Text in der Zwischenablage hat Vorrang (Auftrag 2026-11-25) | – | gleich (beide; native liest Bilder über `arboard`) |
| Formatierte Vorschau des Eingabetexts (👁) (Auftrag 2026-11-25) | – | gleich (beide; nur Hauptfeld, nicht im Thread) |
| Emojis im Text 1,3× Schriftgröße, Nachricht nur aus Emojis groß; native setzt Farb-Emoji-Schrift ausdrücklich (sonst Schwarz-Weiß-Glyphen aus DejaVu Sans) (Auftrag 2026-11-24) | – | gleich (beide) |
| Tagestrenner „Heute“, „Gestern“, Datum | fehlte | angeglichen |
| Gruppierung Folgenachrichten (5 min, gleicher Tag), Uhrzeit beim Überfahren | ohne Tagesgrenze, ohne Uhrzeit | angeglichen |
| Knopf „↓“ nach unten nach > 300 px Hochscrollen | fehlte | angeglichen |
| Nachrichtenfenster 300 (ADR 05, Mattermost) | fehlte | angeglichen |
| Mehrzeilige Eingabe: Enter sendet, Shift+Enter neue Zeile, ↑ im leeren Feld bearbeitet letzte eigene | einzeilig | angeglichen (`text_editor`) |
| Hinweiszeile „Enter senden · Shift+Enter …“, Senden-Knopf | fehlte | angeglichen |
| Text/Dateien erst nach erfolgreichem Senden leeren | gleich | gleich |
| Bearbeiten direkt in der Nachricht | im Eingabefeld | Abweichung (gleiche Wirkung, Esc bricht ab) |
| Löschen mit Rückfrage | ohne Rückfrage | angeglichen (zweiter Klick bestätigt) |
| Reaktions-Schnellauswahl 12 Emojis | 6 | angeglichen |
| Thread-Leiste: Startnachricht aktualisiert sich bei Bearbeiten | nur Antworten | angeglichen |
| Abgelöstes Fenster (⧉) | fehlte | Abweichung: braucht iced-Mehrfenster-Umbau (`iced::daemon`), offen |

## Hinweise, Fokus, Einstellungen

| Funktion (Tauri) | native vorher | Status |
|---|---|---|
| In-App-Hinweise (Toasts): neue DM/Erwähnung in anderem Space mit „Öffnen“, Fehler, „gespeichert“ | Fehlerzeile | angeglichen |
| Ungelesen/Erwähnungszähler bei neuer Nachricht in anderem Space sofort | erst mit `space_updated` | angeglichen |
| Fenster bekommt Fokus → offenen Space als gelesen markieren | fehlte | angeglichen |
| Verbindungsfehler mit Grund als Hinweis | fehlte | angeglichen |
| Update-Hinweis mit „Installieren“ | Banner | gleich (Banner statt Toast) |
| Konto neu anmelden (Mattermost-Formular vorbelegt, Google erneut) | fehlte | angeglichen |
| Konto entfernen mit Rückfrage | ohne Rückfrage | angeglichen |
| Einstellungen beim Start öffnen, wenn kein Konto | Hinweis im Hauptfenster | angeglichen |

## Fenster

| Funktion | Status |
|---|---|
| Startgröße: Hauptfenster 2/3 des Bildschirms, zentriert, erst danach sichtbar (Auftrag 2026-11-21) | gleich (beide); abgelöste Tauri-Fenster 480×640 |
| macOS: Titelleiste in der Farbe der Kontoleiste der Optik, Titeltext hell oder dunkel nach Helligkeit (Auftrag 2026-11-28) | Abweichung: native mit System-Titelleiste |

## Optiken

Tauri-Optiken sind CSS (Schatten, Rahmen, Schriften aus dem System); native bildet Farben und Schrift nach. Die
Retro-Optiken (mIRC 3D-Rahmen, DOS-Doppelrahmen, `C:\CHATTY>`) sind nur angenähert (Abweichung, Darstellung).
