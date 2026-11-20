# Manuelle UI-Prüfung (Stand 2026-11-20)

Was die automatischen Tests (Rust-Unit, Mattermost-Integrationstest, `scripts/test-chat-store.mjs`) nicht
abdecken. Ergebnis je Schritt mit Datum und ✅/❌ eintragen.

## Vorbereitung

```sh
scripts/mm-test-server.sh --messages 400   # Mattermost + Proxy, Nutzer tester/anna/bert, Kanal „Dev“
npm run tauri build -- --no-bundle         # Release-Build
scripts/ram.sh --wait-key                  # startet chatty mit leerem Profil und Testkonto; Enter beendet und räumt auf
```

Zweiter Nutzer für Gegenrichtung: Browser auf `http://localhost:8065`, Anmeldung als `anna` bzw. `bert`
(Passwörter in `src-tauri/target/mm-test.env`).

## Schritte

| # | Bereich | Schritt | Erwartung | Ergebnis |
|---|---|---|---|---|
| 1 | Verlauf | Kanal „Dev“ öffnen | neueste Nachrichten unten, Ladeanzeige verschwindet | |
| 2 | Nachladen | ganz nach oben scrollen, mehrmals | je 50 ältere kommen dazu, Position springt nicht | |
| 3 | Fenster (ADR 05) | nach > 300 geladenen mit ↓-Knopf nach unten | unten bleibt alles ruhig; erneutes Hochscrollen lädt wieder nach | |
| 4 | Nach-unten-Knopf | ein Stück hochscrollen | Knopf ↓ erscheint, Klick scrollt weich ans Ende, Knopf verschwindet | |
| 5 | Live | als anna im Browser in „Dev“ schreiben | Nachricht erscheint sofort; unten → mitgescrollt, oben → Knopf ↓ | |
| 6 | Erwähnung | als anna `@tester hallo` in anderem Kanal | Badge in der Seitenleiste, Hinweis-Toast, Desktop-Benachrichtigung (nur ohne Fokus) | |
| 7 | Senden | Text mit `:smile:` und `**fett**` senden | 😄 und Fettdruck sichtbar | |
| 8 | Anhang wählen | 📎 → Datei wählen → senden | Chip mit Dateiname, nach dem Senden leer; Datei in der Nachricht | |
| 9 | Anhang speichern | Klick auf Datei in einer Nachricht | Speichern-Dialog, Datei korrekt gespeichert | |
| 10 | Anhang abbrechen | 📎 → Dialog abbrechen | nichts passiert, kein Fehler | |
| 11 | Bearbeiten | ↑ im leeren Eingabefeld → ändern → Enter | „(bearbeitet)“, Inhalt neu | |
| 12 | Löschen | 🗑 an eigener Nachricht | Nachricht weg, auch im Browser | |
| 13 | Reaktion | ☺ → 👍; im Browser zweite Reaktion | Zähler stimmt in beiden Richtungen | |
| 14 | Thread | ↩ → Antwort schreiben | rechte Leiste, Antwortzähler im Kanal steigt | |
| 15 | Suche | Suchfeld oben → Begriff → Enter | Treffer rechts, Klick öffnet Kanal | |
| 16 | Favorit | ☆ an einem Kanal | wandert in „Favoriten“, im Browser ebenfalls Favorit | |
| 17 | Ziehen | Kanal per Maus in „Favoriten“ und zurück ziehen | Markierung beim Ziehen, Ablegen setzt/entfernt Favorit | |
| 18 | Ziehen abbrechen | Ziehen beginnen, Esc bzw. außerhalb loslassen | nichts ändert sich | |
| 19 | Stumm | 🔔 an einem Kanal → als anna dort schreiben | kein Fettdruck/Toast; Erwähnung zählt trotzdem | |
| 20 | Sortierung | Neueste / Name / Meiste | Reihenfolge entsprechend, bleibt nach Neustart | |
| 21 | Status-Emoji | als bert im Browser Status mit Emoji setzen | Emoji neben der DM mit bert | |
| 22 | Optiken | Einstellungen → Mattermost, Slack, ehex, mIRC, DOS | alles lesbar, keine abgeschnittenen Bedienelemente | |
| 23 | Abgelöstes Fenster | ⧉ im Kanalkopf | eigenes Fenster mit dem Kanal, Live-Nachrichten kommen an | |
| 24 | Verbindung | `docker restart chatty-mm-proxy` | „getrennt“ in der Seitenleiste, danach wieder verbunden, verpasste Nachrichten da | |
| 25 | Seitenleiste | Breite ziehen, Doppelklick | Breite ändert sich, Doppelklick setzt 240 px | |

## Nicht abgedeckt

- Google Chat (kein OAuth-Client), Windows, macOS.
