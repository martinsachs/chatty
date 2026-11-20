# ADR 05: Begrenztes Nachrichtenfenster statt Virtualisierung

Status: angenommen und umgesetzt (2026-11-20, Entscheidung Martin: ohne Vorab-Messung, ältere Nachrichten erst
beim Hochscrollen wieder nachladen) ([Konzept](../konzept/optimierung.md) S2)

## Kontext

Die Nachrichtenliste rendert jede geladene Nachricht als eigene Komponente. Beim Zurückscrollen kommen
je Seite 50 hinzu, entfernt wird nichts. `content-visibility: auto` spart Layout und Zeichnen, nicht DOM
und JS-Speicher. Die Daten sind tief reaktiv (`$state`).

## Entscheidung


- Weiche Grenze 300: Beim Zurückscrollen darf die Liste wachsen. Erreicht der Nutzer wieder das Ende
  (Abstand < 40 px), fallen die ältesten bis auf 300 heraus, und der Cursor für „ältere laden“ wird auf
  die älteste verbliebene Nachricht gesetzt. Neue Live-Nachrichten kürzen ebenso, wenn der Nutzer unten ist.
- Nur für Mattermost: dort ist der Cursor eine Post-ID. Bei Google ist er ein Server-`pageToken`, der
  sich nicht aus einer Nachricht ableiten lässt; dort wird nicht gekürzt.
- Umsetzung: `Chat.trimToWindow()` in `chat.svelte.ts`, aufgerufen aus `ChannelView.onScroll`, sobald die
  Liste unten ist (auch nach Live-Nachrichten, weil das Mitscrollen ein Scroll-Ereignis auslöst). Test in
  `scripts/test-chat-store.mjs`.
- Nicht umgesetzt (KISS): `messages` als `$state.raw` (S3); das Kürzen allein begrenzt den Speicher.

## Alternativen

- Virtualisierte Liste mit variablen Höhen (Bibliothek oder eigene): verlässlich wenig DOM, aber
  aufwendig wegen variabler Höhen, nachladender Bilder und Scroll-Anker. Verworfen (KISS), solange
  300 Nachrichten flüssig bleiben.
- Keine Grenze: bisheriger Stand; wächst bei langem Zurückscrollen unbegrenzt.

## Folgen

- Wer weit zurückscrollt und wieder nach unten springt, muss ältere Nachrichten neu laden.
