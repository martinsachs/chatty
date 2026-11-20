# ADR 07: Google-Polling nach Fensterfokus

Status: vorgeschlagen (2026-11-20)

## Kontext

Google Chat wird abgefragt (keine Push-Ereignisse ohne Pub/Sub, siehe [00_plan](00_plan.md)). Der
offene Space alle 5 s, die Space-Liste alle 20 s, auch wenn kein chatty-Fenster den Fokus hat. Das
Projekt teilt sich 3000 Lesezugriffe pro Minute mit allen Nutzern.

## Entscheidung

- Bei jedem `WindowEvent::Focused` oder `Destroyed` wertet das Backend
  `webview_windows().values().any(is_focused)` neu aus und schreibt das Ergebnis in
  `AppState.focused: watch::Sender<bool>` (Startwert `true`). So stört die Reihenfolge der Ereignisse beim
  Fensterwechsel nicht. `notify()` liest denselben Wert.
- `ChatBackend::start_events` bekommt zusätzlich `focused: watch::Receiver<bool>`; Mattermost
  ignoriert ihn.
- Der Google-Poller fragt den offenen Space mit `poll.active_secs` ab, solange ein Fenster fokussiert
  ist, sonst mit dem neuen Schlüssel `poll.unfocused_secs` (Standard 30). Bei einem Wechsel erzeugt der
  Poller das Interval neu.
- Die Space-Liste bleibt bei `poll.spaces_secs`, damit Benachrichtigungen nicht später kommen.

## Alternativen

- Fokus aus dem Frontend per Kommando: zusätzlicher IPC-Weg je Fenster; das Backend kennt die
  Fensterereignisse ohnehin. Verworfen.
- `set_active(None)` bei Fokusverlust: ändert zugleich die Sichtbarkeit für Benachrichtigungen.
  Verworfen.

## Folgen

- Ohne Fokus bis zu 30 s Verzögerung im offenen Space; neue Nachrichten in anderen Spaces kommen
  weiter spätestens nach `spaces_secs`.
