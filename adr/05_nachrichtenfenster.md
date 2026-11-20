# ADR 05: Begrenztes Nachrichtenfenster statt Virtualisierung

Status: zurückgestellt (2026-11-20) bis zur Messung „50 gegenüber 300 geladenen Nachrichten“ (Kanal öffnen bzw. fünfmal hochscrollen) ([Konzept](../konzept/optimierung.md) S2)

## Kontext

Die Nachrichtenliste rendert jede geladene Nachricht als eigene Komponente. Beim Zurückscrollen kommen
je Seite 50 hinzu, entfernt wird nichts. `content-visibility: auto` spart Layout und Zeichnen, nicht DOM
und JS-Speicher. Die Daten sind tief reaktiv (`$state`).

## Entscheidung

Falls die Messung im WebProcess mehr als 20 MB Unterschied zwischen 50 und 300 geladenen Nachrichten
zeigt:

- Weiche Grenze 300: Beim Zurückscrollen darf die Liste wachsen. Erreicht der Nutzer wieder das Ende
  (Abstand < 40 px), fallen die ältesten bis auf 300 heraus, und der Cursor für „ältere laden“ wird auf
  die älteste verbliebene Nachricht gesetzt. Neue Live-Nachrichten kürzen ebenso, wenn der Nutzer unten ist.
- Nur für Mattermost: dort ist der Cursor eine Post-ID. Bei Google ist er ein Server-`pageToken`, der
  sich nicht aus einer Nachricht ableiten lässt; dort wird nicht gekürzt.
- `messages` als `$state.raw`. Anzupassen in `chat.svelte.ts`: `toggleReaction`, `reply_count++` in
  `upsert`, Ersetzen/`push` in `upsert`, `messages.sort` in `catchUp`.

## Alternativen

- Virtualisierte Liste mit variablen Höhen (Bibliothek oder eigene): verlässlich wenig DOM, aber
  aufwendig wegen variabler Höhen, nachladender Bilder und Scroll-Anker. Verworfen (KISS), solange
  300 Nachrichten flüssig bleiben.
- Keine Grenze: heute so; wächst bei langem Zurückscrollen unbegrenzt.

## Folgen

- Wer weit zurückscrollt und wieder nach unten springt, muss ältere Nachrichten neu laden.
- Bearbeiten/Reaktionen müssen Nachrichten ersetzen statt mutieren (Code anpassen).
