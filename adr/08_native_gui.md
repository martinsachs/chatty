# ADR 08: Zweite Variante mit nativer GUI (ohne WebView)

Status: angenommen (2026-11-20, Martin: Kern herauslösen, Prototyp mit **iced**; Bibliotheken nur, wenn die
Lizenz kommerzielle Nutzung erlaubt). Kern und Prototyp umgesetzt, siehe „Umsetzung“.

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

## Entscheidung

1. **Workspace teilen:** Crate `chatty-core` (backend, store, config, secrets, model, Kontoverwaltung als
   Fassade mit `tokio::sync::mpsc` für Ereignisse), darauf `chatty` (Tauri, wie heute) und `chatty-native`.
   Dieser Schritt lohnt sich unabhängig vom Toolkit und ändert am Verhalten nichts.
2. **Toolkit für `chatty-native`: iced** (Entscheidung Martin; ursprünglicher Vorschlag war egui):
   - reines Rust, ein Prozess, kein WebView; erwartbar deutlich weniger Speicher (im Prototyp messen,
     mit `scripts/ram.sh --bin`).
   - Optiken (Mattermost, Slack, ehex, mIRC, DOS) sind nur Farb- und Schriftsätze (`Theme::custom`, `Font`).
   - Nachteil: kein nativer Plattform-Look (eigene Widgets wie bei allen Rust-Toolkits außer Qt/GTK);
     Farb-Emojis nur als Bilder; Textauswahl über mehrere Nachrichten hinweg eingeschränkt.
3. **Prototyp vor der Entscheidung** (Umfang: ein Mattermost-Konto, Kanalliste, Verlauf mit Nachladen,
   Senden, Live-Ereignisse): RAM messen, Emojis und lange Verläufe prüfen. Erst danach Threads, Reaktionen,
   Anhänge, Suche, Google, Benachrichtigungen, Updater.

## Alternativen

- **Slint:** deklarative `.slint`-Dateien, Stile mit Plattformanmutung (Fluent, Cupertino, Material, Qt).
  Gute Wahl, wenn der native Look wichtiger ist als die Entwicklungszeit. Lizenz: GPLv3 oder lizenzfrei mit
  Namensnennung für Desktop. Rich Text (Links, Code, Erwähnungen) im Prototyp prüfen.
- **egui:** Immediate Mode, schnell gebaut; ursprünglicher Vorschlag.
- **Qt (cxx-qt) / GTK4 (relm4):** echter nativer Look nur auf je einer Plattform gut (GTK) bzw. großer
  C++-Unterbau (Qt, LGPL). Widerspricht „schlank“.
- **Je Plattform eigene GUI (WinUI, AppKit, GTK):** dreifacher Oberflächencode. Verworfen.
- **Keine zweite Variante:** Ziel < 250 MB ist mit Tauri erreicht; die zweite Variante doppelt den
  Pflegeaufwand für jede neue Funktion.

## Folgen

- Jede neue Funktion muss in beiden Oberflächen gebaut werden; das Backend nur einmal.
- CI baut beide Varianten; Releases und Updater brauchen für `chatty-native` eigene Artefakte.
- Der Kern-Schnitt (Schritt 1) macht `app.rs` schlanker und die Logik ohne Tauri testbar.

## Umsetzung (2026-11-20)

- Cargo-Workspace in `src-tauri/`: `chatty-core` (`core/`: Backends, Store, Config, Secrets, Modell und die
  Fassade `chatty_core::App` mit Kontoverwaltung, Ereignis-Cache und Benachrichtigungsentscheidung),
  `chatty` (Tauri, `src/app.rs` nur noch Kommandos, Dialoge, Fenster, `avatar://`), `chatty-native` (`native/`).
  Beispiele `mm_login`/`mm_watch` und der Integrationstest liegen im Kern (`cargo run -p chatty-core --example …`,
  `cargo test -p chatty-core mattermost::it -- --ignored`).
- Prototyp `chatty-native` (iced 0.14, Renderer tiny-skia, kein wgpu): gespeicherte Konten, Kanalliste
  (Favoriten zuerst), Verlauf mit Nachladen beim Hochscrollen (`anchor_bottom`), Senden, Live-Ereignisse,
  Fensterfokus an den Kern, alle fünf Optiken aus `chatty.toml`. Es fehlen: Konto anlegen (nur über die
  Tauri-Variante), Threads, Reaktionen, Bearbeiten/Löschen, Anhänge, Suche, Formatierung (Markdown wird roh
  angezeigt), eigene Server-Emojis/Profilbilder, OS-Benachrichtigungen, Installer und Updater.
- Prüfhilfe: `chatty-native --open <space-id>`; mit `CHATTY_SHOT=<datei.ppm>` bleibt das Fenster unsichtbar,
  die App speichert nach dem Laden ein Bild von sich und beendet sich. Anlass: ein sichtbares Testfenster nahm
  beim Start den Tastaturfokus und damit Eingaben vom Desktop des Nutzers an.

### Lizenzen (geprüft 2026-11-20, `cargo-license` und Manifeste)

Alle Abhängigkeiten von `chatty-native` (282 Crates) erlauben kommerzielle Nutzung: überwiegend MIT bzw.
Apache-2.0, dazu BSD, ISC, Zlib, Unicode-3.0. Einzelfälle: `option-ext` (MPL-2.0, über `dirs`, schon im Kern;
Copyleft nur auf Dateiebene, bei unveränderter Nutzung keine Pflichten außer Lizenzhinweis) und `self_cell`
(Apache-2.0 **oder** GPL-2.0, Apache gewählt). iced bettet die Schriften Fira Sans (SIL OFL 1.1, Weitergabe
mit Lizenzhinweis erlaubt) und Iced-Icons ein. Für Releases: Lizenzhinweise der Abhängigkeiten beilegen.

### Messung (Linux, Release-Build, 1 Mattermost-Konto, Kanal „Dev“ offen, 60 s, drei Läufe)

| Variante | PSS | RSS | Prozesse |
|---|---|---|---|
| Tauri (WebKitGTK, sichtbar, ohne offenen Kanal) | 237 MB | 455 MB | 3 |
| chatty-native (iced/tiny-skia, **unsichtbares** Fenster) | 18 MB | 24 MB | 1 |

Die native Messung ist eine Untergrenze: Ein unsichtbares Fenster zeichnet nicht laufend und hat keinen
Darstellungspuffer (bei 1100×700 etwa 3 MB). Messung mit sichtbarem Fenster:
`scripts/ram.sh --bin src-tauri/target/release/chatty-native` (startet ein Fenster mit Fokus, daher vom
Nutzer selbst auszuführen). Binärgröße: 12,6 MB (Tauri 14,5 MB).

### Offen

- Entscheidung, ob und in welcher Reihenfolge die fehlenden Funktionen nachgezogen werden.
- Windows und macOS ungetestet (iced/winit unterstützen beide); keine CI-Artefakte für `chatty-native`.
