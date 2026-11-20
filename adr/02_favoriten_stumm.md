# ADR 02: Favoriten und Stummschaltung beim Server, lokal nur als Rückfall

Status: angenommen (2026-11-20, Entscheidung Martin)

## Kontext

Die Seitenleiste bekommt einen Favoriten-Bereich (sortierbar per Ziehen) und stumme Kanäle. Nutzer
arbeiten parallel in der Mattermost-Webapp und der Handy-App.

## Entscheidung

- Mattermost: Favorit = Preference `favorite_channel` (Wert `"true"`/`"false"`), stumm =
  `notify_props.mark_unread = "mention"`. Änderungen aus anderen Clients kommen über die
  WebSocket-Ereignisse `preferences_changed` und `channel_member_updated`.
- Backends ohne eigene Unterstützung (Google): Tabelle `flags` im lokalen SQLite-Cache. Das Backend
  meldet über `set_favorite`/`set_muted` → `Ok(false)`, dass es lokal gespeichert werden muss.
- Die Reihenfolge innerhalb der Favoriten speichert das Frontend lokal je Konto (`localStorage`).
- Stumm wirkt auf Darstellung (grau, nicht fett) und Desktop-Benachrichtigung (nur bei Erwähnung).

## Alternativen

- Alles lokal: eine Logik für alle Backends, aber Webapp und chatty laufen auseinander. Verworfen.
- Mattermost-Sidebar-Kategorien (`/channels/categories`) statt Preference: bildet auch die
  Reihenfolge ab, ist aber deutlich mehr API. Der Server hält Kategorie „Favoriten“ und Preference
  synchron. Zurückgestellt (YAGNI).

## Folgen

- Die Favoriten-Reihenfolge der Webapp wird nicht übernommen.
- Google-Flags gehen beim Entfernen des Kontos verloren (gewollt).
