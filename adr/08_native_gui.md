# ADR 08: Zweite Variante mit nativer GUI (ohne WebView)

Status: vorgeschlagen (2026-11-20), Entscheidung Martin offen: Toolkit und Umfang

## Kontext

Martin wünscht neben der Tauri-Variante eine zweite mit nativer GUI für Linux, Windows und macOS. Grund
ist vor allem der Speicher: Von 237 MB (PSS, Linux, ein Konto) entfallen 151 MB auf WebKit
(WebProcess 134, NetworkProcess 17) und ein Großteil der 86 MB des Hauptprozesses auf GTK-/WebKit-
Bibliotheken; der eigene Heap von chatty liegt bei 14 MB ([Konzept](../konzept/optimierung.md) S7).

Was sich nachnutzen lässt (Stand `312c28a`, Zeilen ohne Emoji-Tabelle):

| Teil | Zeilen | Nachnutzbar |
|---|---|---|
| `backend/` (Trait, Mattermost REST+WS, Google REST+OAuth+Poller) | ~4 000 | ja, unverändert bis auf `tauri::async_runtime::spawn` → `tokio::spawn` |
| `store.rs`, `config.rs`, `secrets.rs`, `model.rs`, `error.rs` | ~740 | ja |
| `app.rs` (Tauri-Kommandos, Konten, Benachrichtigungen, `avatar://`) | ~850 | teilweise: Kontoverwaltung, Ereignisverteilung und Benachrichtigungslogik in den Kern ziehen; Kommandos und Protokoll-Handler bleiben Tauri |
| Svelte-Frontend (`src/`) | ~2 350 | nein: Oberfläche, Optiken und Store-Logik (`chat.svelte.ts`) neu in Rust |

Damit ist das Backend (rund zwei Drittel des Rust-Codes) gemeinsam; die Oberfläche entsteht neu.

## Entscheidung (Vorschlag)

1. **Workspace teilen:** Crate `chatty-core` (backend, store, config, secrets, model, Kontoverwaltung als
   Fassade mit `tokio::sync::mpsc` für Ereignisse), darauf `chatty` (Tauri, wie heute) und `chatty-native`.
   Dieser Schritt lohnt sich unabhängig vom Toolkit und ändert am Verhalten nichts.
2. **Toolkit für `chatty-native`: egui (eframe)**, vorbehaltlich eines Prototyps:
   - reines Rust, ein Prozess, kein WebView; erwartbar deutlich weniger Speicher (im Prototyp messen,
     mit `scripts/ram.sh --bin`).
   - Optiken (Mattermost, Slack, ehex, mIRC, DOS) sind nur Farb- und Schriftsätze (`egui::Visuals`, `Style`).
   - Nachteil: kein nativer Plattform-Look (eigene Widgets wie bei allen Rust-Toolkits außer Qt/GTK);
     Farb-Emojis nur als Bilder; Textauswahl über mehrere Nachrichten hinweg eingeschränkt.
3. **Prototyp vor der Entscheidung** (Umfang: ein Mattermost-Konto, Kanalliste, Verlauf mit Nachladen,
   Senden, Live-Ereignisse): RAM messen, Emojis und lange Verläufe prüfen. Erst danach Threads, Reaktionen,
   Anhänge, Suche, Google, Benachrichtigungen, Updater.

## Alternativen

- **Slint:** deklarative `.slint`-Dateien, Stile mit Plattformanmutung (Fluent, Cupertino, Material, Qt).
  Gute Wahl, wenn der native Look wichtiger ist als die Entwicklungszeit. Lizenz: GPLv3 oder lizenzfrei mit
  Namensnennung für Desktop. Rich Text (Links, Code, Erwähnungen) im Prototyp prüfen.
- **iced:** Elm-Architektur, eigener Look; ähnlich wie egui, aber mehr Strukturaufwand.
- **Qt (cxx-qt) / GTK4 (relm4):** echter nativer Look nur auf je einer Plattform gut (GTK) bzw. großer
  C++-Unterbau (Qt, LGPL). Widerspricht „schlank“.
- **Je Plattform eigene GUI (WinUI, AppKit, GTK):** dreifacher Oberflächencode. Verworfen.
- **Keine zweite Variante:** Ziel < 250 MB ist mit Tauri erreicht; die zweite Variante doppelt den
  Pflegeaufwand für jede neue Funktion.

## Folgen

- Jede neue Funktion muss in beiden Oberflächen gebaut werden; das Backend nur einmal.
- CI baut beide Varianten; Releases und Updater brauchen für `chatty-native` eigene Artefakte.
- Der Kern-Schnitt (Schritt 1) macht `app.rs` schlanker und die Logik ohne Tauri testbar.
