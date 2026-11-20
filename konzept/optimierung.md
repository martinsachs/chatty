# Analyse und Optimierung (Stand 2026-11-20, v0.1.1)

> Hinweis: Seit ADR 08 liegen Backend, Store, Config und Secrets in `src-tauri/core/src/`, die Beispiele in
> `src-tauri/core/examples/`. Pfadangaben unten beziehen sich auf den Stand vor dieser Teilung.

## Ziel

chatty soll mit wenig Speicher laufen, unter Last flüssig bleiben und den Server schonen.

| Größe | Ziel | Quelle | Stand v0.1.1 |
|---|---|---|---|
| RAM (PSS-Summe aller Prozesse, Linux, 60 s nach Start) | < 250 MB mit 1 Konto (Linux/WebKitGTK; bis 2026-11-20: < 150 MB mit 2 Konten) | Entscheidung Martin 2026-11-20 | 241 MB mit 1 Mattermost-Konto (chatty 88, WebKitWebProcess 135, WebKitNetworkProcess 17); 2026-11-20 nach P2: 237 MB (86/134/17), chatty davon eigener Heap 14 MB, Bibliotheken 57 MB |
| REST-Aufrufe je eingehender Mattermost-Nachricht (Kanal und Absender bekannt) | 0 | dieses Dokument | 3–4 |
| Abrisse der WebSocket-Verbindung am echten Server in 5 min | 0 | dieses Dokument | 0 (seit Client-Ping, [ADR 01](../adr/01_mattermost_websocket.md)) |

Gemessen wird mit 1 Mattermost-Konto, weil Google noch nicht live angebunden ist (kein OAuth-Client).
Mit Google als zweitem Konto wird nachgemessen, sobald Google läuft.

Dieses Dokument hält Befunde fest und legt Umsetzung, Dateien und Nachweise fest. Entscheidungen mit
Tragweite stehen als ADR unter `adr/`. ADRs im Status „vorgeschlagen“ nimmt Martin an; S1 ohne
Messnachweis setzt [ADR 04](../adr/04_webview_speicher.md) auf „abgelehnt“.

## Vorgehen

Zwei getrennte Code-Durchsichten (Rust-Kern und Svelte-Frontend) am Stand `f853635`, nur lesend.
Dazu die Messungen aus dem Plan und der 5-Minuten-Mitschnitt gegen den echten Mattermost-Server.
Nutzenangaben sind Schätzungen, solange keine Messung genannt ist.

## Befunde

Priorität: **H** = vor dem nächsten Release, **M** = danach, **N** = bei Gelegenheit.
Entscheidung: **umsetzen**, **messen** (nur übernehmen, wenn die Messung den Nutzen zeigt), **nein** (mit Grund).

### Speicher

| # | Pri | Befund | Maßnahme | Entscheidung |
|---|---|---|---|---|
| S7 | H | Für die 88 MB des Hauptprozesses gibt es keine Aufschlüsselung. Unklar, ob das Ziel erreichbar ist. | `/proc/<pid>/smaps` auswerten: eigener Heap (`Private_Dirty` von `[heap]` und anonymen Bereichen) gegenüber geteilten Bibliotheken (GTK, WebKit). Ergebnis in die Tabelle „Ziel“. | umsetzen (erster Schritt von P3) |
| S1 | H | WebKitGTK läuft mit Standard-Cachemodell; die WebView ist mit 135 MB der größte Posten. | Cachemodell `DocumentViewer` am `WebContext` per `with_webview` im `setup`. Neue Abhängigkeit `webkit2gtk = "2.0"` (wie über wry im Lockfile, 2.0.2), nur `cfg(target_os = "linux")`. Ob die Einstellung nach dem Start des Hauptfensters noch greift, zeigen `get_cache_model()` im Log und die Messung; haben Popouts einen eigenen Context, auch in `open_space_window`. | gemessen: kein Nutzen (237 → 237 MB), abgelehnt, [ADR 04](../adr/04_webview_speicher.md) |
| S2 | M | Nachrichtenliste wächst beim Zurückscrollen unbegrenzt; die 60-s-Messung berührt das nicht. | Fenster nach [ADR 05](../adr/05_nachrichtenfenster.md). | umgesetzt (Entscheidung Martin 2026-11-20, ohne Vorab-Messung) |
| S3 | M | `messages` ist tief reaktiv (`$state`). | Nur zusammen mit S2, siehe ADR 05. | zurückgestellt (KISS): das Kürzen allein begrenzt den Speicher |
| S4 | M | Abgelöstes Fenster lädt beim Start Spaces aller Konten aus Cache und Server. | Popout lädt nur sein Konto. | umsetzen |
| S5 | N | Avatar-Cache klont Bytes je Anfrage. | – | nein: der Responder braucht ohnehin einen eigenen Body |
| S6 | N | `countedReplies`, `items` wachsen. | – | nein: wenige String-IDs |

### Netz und Server

| # | Pri | Befund | Maßnahme | Entscheidung |
|---|---|---|---|---|
| N1 | H | `handle()` läuft in `ws.rs` innerhalb der `select!`-Schleife. Hängt ein REST-Aufruf, gehen keine Pings raus. Der Mattermost-Client hat keinen Timeout. | `handle()` in `tokio::time::timeout(10 s, …)`; bei Ablauf verfällt das Ereignis wie heute bei Fehlern (Abstand zwischen Pings höchstens 30 s). REST-Timeout 30 s zentral in `send_req`; neues `send_file_req` ohne Gesamt-Timeout für Upload, Download, Avatar und Server-Emoji; `login`/`restore` setzen `.timeout(30 s)` selbst; WebSocket-Upgrade ohne. `connect_timeout(10 s)` am Client. | umsetzen |
| N2 | H | Jede fremde `posted`-Nachricht ruft `fetch_space` (3–4 REST-Aufrufe). | Das Backend rechnet den Space fort: `spaces: Mutex<HashMap<String, Space>>` aus `list_spaces`/`fetch_space`. Bei `posted` (auch eigene) in bekanntem Kanal: `last_active = create_at`, `msg_count += 1`; bei fremden: `unread = true` wenn `!is_reply`; `mention_count += 1` wenn `mentions_me || (kind != Channel && !muted && !is_reply)` (gleiche Regel wie `markUnread`/`notify`). Danach `SpaceUpdated` (0 REST-Aufrufe). Unbekannter Kanal → `fetch_space`. `channel_viewed`/`multiple_channels_viewed` bleiben bei `fetch_space` (selten) und aktualisieren den gemerkten Space. Erwähnungen in Thread-Antworten zählen wie heute über den Server erst beim nächsten `list_spaces` (bewusst). | umsetzen |
| N3 | M | Jede WebSocket-Sitzung meldet `online`; das Frontend lädt dann in jedem Fenster alle Spaces. | Nur Frontend: jedes `online` → `catchUp()` in jedem Fenster; `loadSpaces()` nur im Hauptfenster (ein `list_spaces` je Abriss hält Ungelesen-Stände aus der Lücke richtig; Abrisse sind seit dem Ping selten). Backend unverändert. | umsetzen, [ADR 01](../adr/01_mattermost_websocket.md) |
| N4 | M | Stiller Reconnect wartet immer 1 s. Schließt der Server sofort nach dem Upgrade, entsteht eine Schleife ohne Offline-Meldung. | „War verbunden“ nur bei einer Sitzung > 30 s; sonst Backoff und Offline-Meldung. | umsetzen |
| N5 | M | Google: offener Space wird auch ohne Fensterfokus alle 5 s abgefragt. | Bei jedem `WindowEvent::Focused`/`Destroyed` `webview_windows().values().any(is_focused)` neu auswerten und in `AppState.focused: watch::Sender<bool>` (Start `true`) schreiben; `notify()` liest diesen Wert. `ChatBackend::start_events` bekommt `focused: watch::Receiver<bool>` (Mattermost ignoriert ihn). Poller: ohne Fokus entfällt `poll_active`, und `poll_spaces` behandelt den offenen Space wie jeden anderen (Takt `spaces_secs`); mit Fokus wie heute. Kein neuer Konfigurationsschlüssel. | umsetzen, [ADR 07](../adr/07_polling_fokus.md) |
| N6 | M | Google: ein Fehler in `messages_since` für einen Space verwirft alle Ereignisse des Durchlaufs, bei jedem Tick erneut. | `is_transient` (`poll.rs`) entscheidet: transient → Abbruch wie bisher; sonst Fehler loggen, Space überspringen und seinen neuen Eintrag in `State.spaces` übernehmen (seine Nachrichten dieses Durchlaufs verfallen, kein Dauerabruf). | umsetzen |
| N7 | M | Google: 429 wird als „offline“ gemeldet. | Poller meldet bei 429 kein `offline`; Backoff bleibt. `Retry-After` erst auswerten, wenn ein Live-Test zeigt, dass Google ihn sendet. | umsetzen |
| N8 | N | Google: ein Fehler beim Auflösen eines DM-Namens (`space_name`) lässt `list_spaces` scheitern. | Rückfall auf die Space-ID; gemerkt nur bei nicht-transienten Fehlern (Muster wie `user_name`). `is_transient` in `poll.rs` wird `pub(super)`. | umsetzen |
| N9 | N | Mattermost: Nutzer-IDs ohne Treffer (gelöschte Nutzer, Bots) werden bei jedem Post erneut angefragt. | Ersatz-`ApiUser` mit leerem `username` eintragen; `space_name` und `map_post` behandeln leer wie fehlend (DM-Name bleibt `display_name`, Absender bleibt die ID). | umsetzen |

### Lokale Daten und CPU

| # | Pri | Befund | Maßnahme | Entscheidung |
|---|---|---|---|---|
| L1 | H | SQLite im WAL-Modus mit Standard `synchronous=FULL`: fsync je Ereignis, unter dem Store-Mutex. | `PRAGMA synchronous=NORMAL` in `Store::init`. | umsetzen |
| L2 | M | `notify()` und `set_space_flag` lesen alle Spaces aus JSON, um einen zu finden. | `Store::space(account, id)`. | umsetzen |
| L3 | M | Trimmen auf 500 Nachrichten läuft bei jedem Einzelereignis. | – | nein (vorerst): indiziertes `DELETE`, das teure fsync entfällt mit L1 |
| L4 | M | Seitenleiste filtert und sortiert je Render dreifach, auch bei jeder Mausbewegung beim Ziehen; `sortSpaces` sortiert bei jedem `space_updated`; `unreadTotal` zweimal je Konto. | Ein `$derived` für alle Bereiche, gemeinsamer Collator, `drag` nur bei Zieländerung, `unreadTotal` als `$derived`; `sortSpaces` in `chat.svelte.ts` entfällt (die Seitenleiste sortiert selbst). | umsetzen |
| L5 | N | `day()` mehrfach je Nachricht und Render. | – | nein: nur bei S2/S3 relevant |
| L6 | N | Store-Zugriffe synchron auf Tokio-Threads. | – | nein (vorerst): L1 nimmt den Großteil der Last |

### Korrektheit

| # | Pri | Befund | Maßnahme |
|---|---|---|---|
| K1 | H | `openSpace` prüft nach `listMessages` den Space, aber nicht nach `cachedMessages` und nicht das Konto; `loadOlder` prüft nach `await` gar nicht. Schneller Wechsel (auch A→B→A) mischt Verläufe. | Zähler `loadSeq`, erhöht von `openSpace`, `selectAccount` und `catchUp` beim Ersetzen; `loadOlder` merkt sich den Wert und verwirft bei Abweichung. Ersetzt die vorhandenen Space-Vergleiche. |
| K2 | H | `catchUp` bricht ab, wenn gerade älterer Verlauf lädt; nach langer Trennung entsteht eine Lücke. | Flags `loadingInitial`/`loadingOlder` statt `loading`. `catchUp` läuft auch während `loadingOlder`; während `loadingInitial` entfällt es (die Seite ist frisch). Überlappt die neue Seite nicht mit dem Bestand, ersetzt sie den Verlauf, setzt `cursor = page.next_cursor` und erhöht `loadSeq`. `ChannelView`: „Lade …“ und „Anfang von …“ nach `loadingInitial`, Nachladen nach `loadingOlder`. |
| K3 | M | „Anfang von …“ blitzt bei jedem Space-Wechsel auf. | Nur ohne `loadingInitial` zeigen (mit K2). |
| K4 | M | Verlauf landet beim Öffnen nicht sicher unten (`contain-intrinsic-size` schätzt Höhen). | `ResizeObserver` auf ein inneres Wrapper-Element der Liste; hält unten, solange der Abstand < 40 px ist. |
| K5 | N | `selectAccount` meldet den alten Space nicht als inaktiv. | `setActive(alt, null)`. |
| K6 | N | Ziehen/Breite ändern reagieren nicht auf `pointercancel`. | Wie `pointerup`. |
| K7 | N | `delete_account` lässt Zeilen in `flags` liegen. | Mitlöschen, in einer Transaktion. |

Alle K-Punkte: umsetzen.

### Sicherheit

| # | Pri | Befund | Maßnahme | Entscheidung |
|---|---|---|---|---|
| X1 | M | `send_message(files)` liest und `download_attachment(dest)` schreibt beliebige Pfade aus dem Frontend. | Dialoge im Backend (Callback-API von `tauri-plugin-dialog` mit `oneshot`-Kanal). `save_attachment(account, attachment) -> bool` (`false` = Abbruch; bei Fehler bleibt im Frontend der Rückfall `openUrl`); `download_attachment` entfällt. `pick_files() -> Vec<PickedFile { id: u64, name }>`; Zustand `AppState.picked: Picked`, `struct Picked(Mutex<HashMap<u64, PathBuf>>)` mit Zähler-IDs und `add(Vec<PathBuf>) -> Vec<PickedFile>`, `paths(&[u64]) -> Result<Vec<PathBuf>>` (unbekannt → „Datei nicht mehr verfügbar, bitte neu wählen“), `remove(&[u64])` nach erfolgreichem Senden. `Composer` leert seine Liste erst nach Erfolg. `dialog:default` fällt aus der Capability, `@tauri-apps/plugin-dialog` aus `package.json`. Kein Datei-Drag&Drop. | umsetzen, [ADR 06](../adr/06_dateipfade_ipc.md) |
| X2 | N | `cache.sqlite` (Klartext) und Datenverzeichnis mit Standard-umask; `chatty.toml`/`accounts.json` nicht atomar geschrieben. | `config_dir()`/`data_dir()` setzen 0700 (auch bestehende Verzeichnisse); `write_private(&Path, &[u8])` schreibt über eine Temp-Datei mit 0600 und `rename` und ersetzt die Logik in `save_config` und `save_accounts`. Unter Windows nur Temp-Datei und `rename` (Rechte über das Profilverzeichnis). | umsetzen |
| X3 | N | CSP erlaubt `img-src https:`, genutzt wird es nicht. | `img-src 'self' data: avatar: http://avatar.localhost`. | umsetzen |
| X4 | N | Keyring-Aufrufe blockieren Tokio-Threads. | – | nein: nur bei Anmelden, Start und Löschen, kein Betriebsproblem |

Kein Befund: Markdown-Rendering (`format.ts`) escaped vor dem Ersetzen, Links nur `http(s)`; CSP verbietet Inline-Skripte.

### Build

| # | Pri | Befund | Maßnahme | Entscheidung |
|---|---|---|---|---|
| B1 | M | aws-lc-rs kommt nur über chattys reqwest-Feature `rustls` (`cargo tree -e features -i aws-lc-rs`); ring ist über `rustls-webpki` ohnehin im Baum. aws-lc-rs braucht cmake/C (Windows: nasm). | reqwest mit `rustls-no-provider`, `rustls = { version = "0.23", default-features = false, features = ["ring", "std", "tls12"] }`. `backend::init_tls()` (idempotent per `std::sync::Once`) installiert ring als Standard; Aufruf in `lib.rs::run` und in beiden `http_client()`. `GoogleBackend::for_test` baut seinen Client über `http_client()` statt `Client::new()`, damit Tests den Provider haben. | umsetzen, [ADR 03](../adr/03_tls_ring.md) |
| B2 | N | tokio-tungstenite mit `rustls-tls-webpki-roots` und `connect`, obwohl das Upgrade über reqwest läuft. | `default-features = false, features = ["handshake"]`; Modulkommentar in `ws.rs` anpassen. | umsetzen |
| B3 | N | reqwest-Feature `stream` unbenutzt. | Feature entfernen. | umsetzen |

Nicht verfolgt: Umstieg von SvelteKit auf reines Svelte (einige 10 KB JS); virtualisierte Nachrichtenliste ([ADR 05](../adr/05_nachrichtenfenster.md)).

## Umsetzung in Paketen

**Stand 2026-11-20:** P0–P6 umgesetzt (Commits `e8aee26` P0, `b482cea` P1, `eda38da` P2, `e79635a` P3, `a53eacb` P4,
`95e52c2` P6, `f9a214c` P5). Abweichungen: S1 gemessen ohne Nutzen und verworfen; S2/S3-Messung (Hochscrollen)
und die manuellen UI-Prüfschritte stehen aus; P1-Logik zusätzlich automatisch über `scripts/test-chat-store.mjs`
(CI). P5 nur gegen den HTTP-Mock.


Jedes Paket ist ein Commit mit grüner CI (`cargo test`, `cargo clippy --all-targets -D warnings`,
`svelte-check`). Der Mattermost-Integrationstest (`backend/mattermost/it.rs`, `#[ignore]`) läuft manuell
gegen den Testserver aus P0. UI-Nachweise sind manuelle Prüfschritte mit erwartetem Ergebnis (kein
UI-Testgerüst, YAGNI).

Reihenfolge: P0 → P1 → P2 → P3 → P4 → P6 → P5.

### P0 Testwerkzeug

Dateien (neu): `scripts/mm-test-server.sh`, `src-tauri/examples/mm_login.rs`,
`src-tauri/examples/mm_watch.rs`; ändern: `src-tauri/src/backend/mattermost/it.rs` (Kopfkommentar).

`scripts/mm-test-server.sh` (bash, idempotent: vorhandene Container werden ersetzt):
- Startet `mattermost/mattermost-preview` als `chatty-mm` (Port 8065) und wartet auf `/api/v4/system/ping`.
- Startet `nginx:stable` als `chatty-mm-proxy` mit `--network host` auf Port 8066, reicht HTTP und WebSocket an
  `127.0.0.1:8065` durch. `--delay` setzt `limit_rate 2k` für `/api/v4/channels/*/posts`, damit „ältere laden“
  sichtbar lange dauert.
- Legt per REST an: `tester` (erster Nutzer, damit Systemadmin), `anna`, `bert`; Teams `team1` und `team2`,
  `tester` in beiden, `anna` in `team1`; Kanal „Dev“ in `team1` mit `tester` und `anna`; die DM tester–anna.
- `--messages N` (Standard 0): `anna` sendet N nummerierte Nachrichten in „Dev“.
  `--channels N` (Standard 0): `tester` legt N offene Kanäle `c-<n>` in `team1` an und ist Mitglied.
- `--send N`: nur N weitere Nachrichten als `anna` an Port 8065 in „Dev“ senden (für Trenn-Tests), ohne Neuaufbau.
- Schreibt `src-tauri/target/mm-test.env` (außerhalb von `build/`, das der Frontend-Build leert) mit
  `CHATTY_MM_URL`, `CHATTY_MM_PROXY_URL`, `CHATTY_MM_USER`, `CHATTY_MM_PASSWORD`, `CHATTY_MM_USER2`,
  `CHATTY_MM_PASSWORD2`, `CHATTY_MM_USER3`, `CHATTY_MM_PASSWORD3`, `CHATTY_MM_DEV`. Passwörter zufällig, keine
  Ausgabe.

Beispiele:
- `cargo run --example mm_login -- <konto-id> [--delete]` legt das Session-Token von `CHATTY_MM_USER` über
  `CHATTY_MM_PROXY_URL` im Keyring ab bzw. löscht den Eintrag.
- `cargo run --example mm_watch -- <konto-id> <server-url> <sekunden>` stellt das Konto mit `restore` her und
  gibt alle Backend-Ereignisse mit Zeitstempel aus. Für den echten Server: Konto-ID und URL aus
  `~/.config/chatty/accounts.json`.

Nachweis: Skript zweimal hintereinander ausführbar; Integrationstest mit `set -a; . src-tauri/target/mm-test.env`
grün.

### P1 Korrektheit (K1–K7)

Dateien (ändern): `src/lib/chat.svelte.ts`, `src/lib/components/ChannelView.svelte`,
`src/lib/components/Sidebar.svelte`, `src-tauri/src/store.rs`.

Nachweis:
- Unit-Test `store.rs`: `delete_account` entfernt `spaces`, `messages`, `flags`.
- UI (Konto über `CHATTY_MM_PROXY_URL`): Space A öffnen, sofort B, sofort A → nur Nachrichten von A, keine
  Dopplung.
- Testserver mit `--messages 300 --delay`. UI: in „Dev“ hochscrollen, während „ältere laden“ läuft (gedrosselt)
  Space wechseln → neuer Space ohne fremde Nachrichten.
- UI: `docker stop chatty-mm-proxy`, `scripts/mm-test-server.sh --send 60`,
  `docker start chatty-mm-proxy` → nach dem Reconnect alle 60 sichtbar, in richtiger Reihenfolge; dasselbe,
  während „ältere laden“ läuft.
- UI: Space-Wechsel → kein „Anfang von …“ während des Ladens; Verlauf steht unten.

### P2 Netz Mattermost (N1, N2, N3, N4, N9)

Dateien (ändern): `src-tauri/src/backend/mattermost/ws.rs`, `src-tauri/src/backend/mattermost/mod.rs`,
`src-tauri/src/backend/mattermost/api.rs` (N9 in `map_post`), `src-tauri/src/backend/mattermost/it.rs`,
`src/lib/chat.svelte.ts`.

Nachweis (Integrationstest gegen einen frisch aufgesetzten Testserver):
- Aufrufzähler: `AtomicUsize` im `MattermostBackend`, erhöht in `send_req`/`send_file_req`, unter
  `#[cfg(test)]` lesbar.
- Fremde Nachricht in „Dev“ von `anna` → Zählerdifferenz 0, `SpaceUpdated` mit `unread` und `mention_count > 0`
  (bestehende Prüfung bleibt).
- Nach dem bestehenden `mark_read`/`channel_viewed`: Thread-Antwort von `anna` ohne Erwähnung → `SpaceUpdated`
  mit `!unread` und unverändertem `mention_count`.
- `bert` meldet sich an (`CHATTY_MM_USER3`), legt die DM mit `tester` an (`POST /channels/direct`) und sendet →
  Zählerdifferenz ≥ 1 (Wert wird ausgegeben), danach `SpaceUpdated` der neuen DM.
- Trennung: zweites Backend per `restore` über `CHATTY_MM_PROXY_URL`, `start_events`, 35 s warten,
  `docker stop chatty-mm-proxy` und `docker start chatty-mm-proxy` per `std::process::Command` → innerhalb von
  15 s `Status { online: true }`; ein kurzes `online: false` ist erlaubt.
- N4: gleiches Backend, Proxy in den ersten 5 s einer Sitzung neu starten → `Status { online: false }` wird
  gemeldet (keine stille Schleife).
- Mitschnitt `mm_watch` 5 min am echten Server: kein `status`-Ereignis außer dem ersten.

### P3 Speicher (S7, S1, S4; S2/S3 nur messen)

Dateien: neu `scripts/ram.sh`; ändern `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` (`webkit2gtk`, Linux), `konzept/optimierung.md` (Tabelle „Ziel“), `adr/04_webview_speicher.md` (Status),
`src-tauri/src/app.rs` (`setup`, ggf. `open_space_window`), `src/lib/chat.svelte.ts` und
`src/routes/+page.svelte` (S4), `adr/00_plan.md` (Messwerte).

Nachweis: Messverfahren unten; S7-Aufschlüsselung; S1 gegen den Build des Vorgänger-Commits (je Median aus
drei Läufen); Läufe „50“ und „300 Nachrichten“ als Grundlage für S2/S3.

### P4 Lokal (L1, L2, L4)

Dateien (ändern): `src-tauri/src/store.rs`, `src-tauri/src/app.rs`, `src/lib/components/Sidebar.svelte`,
`src/lib/chat.svelte.ts`.

Nachweis:
- Unit-Test `store.rs`: `space()` findet genau einen Eintrag, `None` bei unbekannter ID.
- UI in `npm run tauri dev` mit `scripts/mm-test-server.sh --channels 500`: einen Kanal in „Favoriten“ ziehen →
  in der Timeline des WebInspectors kein Eintrag > 50 ms (vorher/nachher notieren).

### P6 Härtung und Build (X1–X3, B1–B3)

Dateien (ändern): `src-tauri/src/app.rs` (`pick_files`, `save_attachment`, `send_message`,
`download_attachment` entfernen), `src-tauri/src/lib.rs` (Kommandos, `init_tls`),
`src-tauri/src/backend/mod.rs` (`init_tls`), `src-tauri/src/backend/mattermost/mod.rs` und
`src-tauri/src/backend/google/mod.rs` (`http_client` ruft `init_tls`, `for_test` nutzt `http_client`), `src-tauri/src/backend/mattermost/ws.rs`
(Kommentar), `src-tauri/src/config.rs` (`private_dir`, `write_private`), `src-tauri/tauri.conf.json` (CSP),
`src-tauri/capabilities/default.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src/lib/api.ts`,
`src/lib/chat.svelte.ts` (`send` mit IDs), `src/lib/components/Composer.svelte` (Anzeige `name`, Schlüssel
`id`), `src/lib/components/MessageItem.svelte`, `package.json`, `package-lock.json`, `adr/00_plan.md` (Binary-Größe),
`.github/workflows/build.yml` (Schritt im Arbeitsverzeichnis `src-tauri`).

Nachweis:
- CI-Schritt `! cargo tree --target all -i aws-lc-rs >/dev/null 2>&1` grün; Release-Binary-Größe
  vorher/nachher in `adr/00_plan.md`.
- Unit-Tests: `config.rs` (`private_dir` → 0700, `write_private` → 0600, Inhalt vollständig);
  `Picked` (`paths` mit unbekannter ID → Fehler; bekannte ID bleibt bis `remove`).
- UI: Anhang wählen und senden; Anhang speichern; Speichern-Dialog abbrechen → keine Meldung.

### P5 Google (N5–N8)

Dateien (ändern): `src-tauri/src/backend/mod.rs` (`start_events`-Signatur),
`src-tauri/src/backend/google/{mod,poll}.rs`, `src-tauri/src/backend/mattermost/mod.rs` (Signatur),
`src-tauri/src/backend/mattermost/it.rs` und `src-tauri/examples/mm_watch.rs` (Aufruf `start_events`),
`src-tauri/src/app.rs` und `src-tauri/src/lib.rs` (Fokus-`watch`, `on_window_event`, `notify`), `usage.md`.

Nachweis (Unit-Tests im Testmodul von `google/mod.rs`, wo der HTTP-Mock liegt; ohne Zeitsteuerung):
- Entscheidungen als reine Funktionen in `poll.rs`: „Status bei Fehler“ (429 → kein `offline`), „offenen Space
  überspringen“ (nur mit Fokus).
- Mock: ein Space liefert 404 in `messages_since` → übrige Spaces melden ihre Ereignisse; im nächsten Durchlauf
  kein Abruf des 404-Space.
- Mock: DM-Name nicht auflösbar (404) → `list_spaces` liefert den Space mit seiner ID als Namen.
- Live-Test mit Google-Konto, sobald ein OAuth-Client existiert.

## Messverfahren RAM

1. `scripts/mm-test-server.sh --messages 300` (P0).
2. `scripts/ram.sh [--bin PFAD] [--wait-key] [sekunden=60]` liest `src-tauri/target/mm-test.env`, legt per
   `mm_login` den Keyring-Eintrag `chatty-ram-test` an (der Keyring ist nicht über `XDG_*` isolierbar), schreibt
   in ein temporäres `XDG_CONFIG_HOME` die Datei `chatty/accounts.json` mit dem Array `[{ "id": "chatty-ram-test",
   "kind": "mattermost", "server": $CHATTY_MM_PROXY_URL, "label": "ram-test", "login": $CHATTY_MM_USER }]`,
   startet das Binary (Standard `src-tauri/target/release/chatty`; für S1 der Build des Vorgänger-Commits, vorher
   nach `src-tauri/target/chatty-before` kopiert) mit isoliertem
   `XDG_CONFIG_HOME`/`XDG_DATA_HOME`, wartet die Sekunden (oder mit `--wait-key` auf Enter), summiert PSS und
   RSS aller Prozesse des Baums (Kinder rekursiv über die Eltern-PID), gibt für den Hauptprozess zusätzlich die
   S7-Aufschlüsselung aus, beendet die Prozessgruppe und löscht den Keyring-Eintrag (`mm_login --delete`).
3. Läufe: „Start“ (60 s, kein Kanal offen); „50 Nachrichten“ (`--wait-key`: Kanal „Dev“ öffnen, 30 s warten,
   Enter); „300 Nachrichten“ (`--wait-key`: „Dev“ öffnen, fünfmal ans obere Ende scrollen, bis 300 geladen
   sind, 30 s warten, Enter). Alles im normalen Release-Build.
4. Je Messung drei Läufe, Median. Ergebnisse in die Tabelle „Ziel“ und in `adr/00_plan.md`.

## Offene Punkte

- S7 und S1 (2026-11-20): Der Hauptprozess hat nur 14 MB eigenen Heap, der Rest sind GTK/WebKit-Bibliotheken;
  der WebProcess liegt im Leerlauf bei 134 MB und reagiert nicht auf das Cachemodell. Entschieden (Martin,
  2026-11-20): Ziel für Linux < 250 MB mit einem Konto; erreicht (237 MB).
- S2/S3: entschieden (Martin, 2026-11-20) ohne Vorab-Messung: Fenster nach [ADR 05](../adr/05_nachrichtenfenster.md),
  ältere Nachrichten erst beim Hochscrollen wieder nachladen.
- Windows (WebView2) und macOS (WKWebView) sind nicht gemessen.
- Google-Maßnahmen (P5) sind nur gegen den Mock geprüft, bis ein OAuth-Client existiert.
