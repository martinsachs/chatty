# ADR 01: Mattermost-Echtzeit über WebSocket mit Client-Ping

Status: angenommen (2026-11-20); Ergänzung N1–N4 vorgeschlagen ([Konzept](../konzept/optimierung.md))

## Kontext

Mattermost liefert Ereignisse über `/api/v4/websocket`. Der Firmenserver steht hinter einem Proxy, der
Verbindungen ohne Verkehr nach 30–90 s ohne Close-Frame trennt (Mitschnitt 2026-11-20: 3 Abrisse in
5 min, `Connection reset without closing handshake`). Der Server selbst pingt seltener.
`tokio_tungstenite::connect_async` mit `wss://` bricht ab, weil zwei rustls-Provider im Baum sind und
keiner als Standard installiert ist.

## Entscheidung

- Das Upgrade läuft über den reqwest-Client des Backends (HTTP/1.1, `Response::upgrade`), danach
  `WebSocketStream::from_raw_socket`. Damit nutzt `wss://` dieselbe TLS-Konfiguration wie REST.
- Der Client sendet alle 20 s `{"action":"ping"}` (wie die Mattermost-Webapp). Kommt 60 s nichts an,
  gilt die Verbindung als tot.
- Bricht eine bestehende Verbindung ab, verbindet der Client still neu. „Offline“ meldet er erst, wenn
  das Neuverbinden scheitert. Danach holt der offene Space verpasste Nachrichten nach.
- Vorgeschlagen (Konzept N1–N4):
  - `handle()` läuft mit 10 s Timeout, REST-Aufrufe mit 30 s; ein hängender Aufruf hält Pings so höchstens
    10 s auf.
  - Bei `posted` in bekannten Kanälen rechnet das Backend den Space selbst fort (Zeit, Zähler,
    ungelesen, Erwähnung) statt ihn per REST zu laden.
  - Jede Sitzung meldet weiter `online`. Das Frontend holt danach in jedem Fenster den offenen Space nach;
    die Space-Liste lädt nur das Hauptfenster.
  - Eine Sitzung zählt erst nach 30 s als „war verbunden“, sonst gilt normales Backoff.

## Alternativen

- WebSocket-Ping-Frames statt `action: ping`: halten den Proxy ebenso wach, die Antwort des Servers
  ist aber nicht in jeder Version garantiert. Verworfen zugunsten des Webapp-Verhaltens.
- Polling wie bei Google: einfacher, aber Verzögerung und Serverlast. Verworfen.

## Folgen

- Ein Ping je 20 s und Verbindung: vernachlässigbare Last.
- Ereignisse während eines Abrisses kommen über das Nachladen des offenen Space und die Space-Liste des
  Hauptfensters (ein `list_spaces` je Abriss; seit dem Ping selten).
