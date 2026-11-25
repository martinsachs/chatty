# Sicherheit: Analyse, Pentest, Maßnahmen (Stand 0.2.3, 2026-11-23)

Anlass: externe Meldung zu 0.2.2 (HTML-Injection, ungepinnte Actions, Mattermost über http). Danach zwei
unabhängige Reviews (Tauri-Oberfläche; Rust-Kern und native), Abhängigkeits-Audit und ein Pentest-Lauf gegen
den lokalen Testserver. Meldeweg für neue Funde: [SECURITY.md](../SECURITY.md) (Private Vulnerability
Reporting ist aktiv).

## Angreifermodelle

| Angreifer | Kontrolliert |
|---|---|
| Anderer Chat-Nutzer | Nachrichtentext, Kanal-Header, Dateinamen und -inhalte, Anzeigename, Profilbild, eigene Emojis |
| Bösartiger oder übernommener Server, MITM | alle Antworten von REST/WebSocket, Weiterleitungen, Größe und Tempo der Antworten |
| Lokaler Prozess ohne Rechte | Verbindungen zum OAuth-Loopback |
| Lieferkette | GitHub Actions, Crates, npm-Pakete, Update-Kanal |

## Befunde und Maßnahmen

| # | Befund | Schwere | OWASP | Maßnahme | Prüfung |
|---|---|---|---|---|---|
| 1 | Markdown-Link direkt nach URL: fertiges Link-HTML landet im `href` und bricht aus dem Attribut aus (unsichtbarer Vollbild-Link, Tauri) – seit 0.2.2, über Emoji-Platzhalter schon vorher | mittel | A03 Injection, DA1 | URL-Muster schließen Platzhalter aus, echte `\u0000` entfernt (Patch der meldenden Person) | `scripts/test-format.mjs`: feste Fälle + 50 000 Fuzz-Eingaben gegen Tag-Whitelist, in CI |
| 2 | Signaturschlüssel in Jobs mit nur per Tag gepinnten Drittanbieter-Actions | mittel | A08 Integrität, CICD-SEC-3/8 | Actions auf Commit-SHA (gegen Upstream-Tags geprüft). Release: Schlüssel nur in `release` (nur GitHub-Actions + gepinnte tauri-action) und `native-sign`; native Builds ohne Schlüssel; keine Caches in Release-Jobs; cargo-packager fest 0.11.8 | Release-Lauf 0.2.3 |
| 3 | Mattermost-Login über `http://` (Passwort, MFA, Token im Klartext) | niedrig–mittel | A02 Kryptografie | nur `https://`, `http://` nur für Loopback | Unit-Test |
| 4 | OAuth-Loopback: fremder `state` bricht Anmeldung ab, stumme Verbindung blockiert | niedrig | A07 Auth | fremder `state` wird ignoriert, 5 s je Verbindung | Unit-Test `loopback_receives_code` |
| 5 | Native Updater ohne Downgrade-Schutz (alte, gültig signierte Datei als neue Version) | niedrig | A08 | Version und Plattform im Asset-Namen; signierter Kommentar `file:` muss passen; nur `https`, max. 300 MiB | Unit-Test `signature_bound_to_version_and_platform` |
| 6 | Markup in OS-Benachrichtigungen (Linux-Dienste werten `<a>`, `<img>` aus) | niedrig–mittel | A03 | `<`/`>` aus Titel und Text ersetzt (beide Varianten) | Unit-Test |
| 7 | Downloads ohne Obergrenze, kein Lese-Timeout (Speicher-DoS durch Server, Avatare laden ohne Klick) | mittel | A04 Design, DA-DoS | `read_limited`: Bilder 5 MiB, Anhänge 512 MiB; Lese-Timeout 60 s | Unit-Test mit simuliertem Server |
| 8 | Bild-Dekompressionsbombe als Avatar/Emoji (native) | niedrig | A04 | Abmessungen vor dem Dekodieren prüfen (max. 2048×2048) | Unit-Test |
| 9 | WebSocket ohne Größenlimit (64 MiB je Nachricht) | niedrig | A04 | 4 MiB je Nachricht, 1 MiB je Frame | Integrationstest |
| 10 | Konto entfernen lässt Sitzung/Refresh-Token serverseitig gültig | niedrig | A07 | Mattermost `POST /users/logout`, Google Token-Revoke | Integrationstest („nach Logout ist das Token ungültig“) |
| 11 | Dateiname vom Absender: RLO (`rechnung‮fdp.exe` erscheint als „…exe.pdf“), `../`, `.bashrc` | niedrig | A04 | `safe_file_name` beim Abbilden der Anhänge (Anzeige) und im Speichern-Dialog | Unit-Test, Pentest-Screenshot |
| 12 | Datei-ID ungeprüft im API-Pfad (`/files/../…` gegen den eigenen Server) | info | A01 | Mattermost nur alphanumerisch, Google ohne `..`/`?`/`#` | Integrationstest |
| 13 | `mailto:?attach=/pfad` (native) | niedrig | A01 | `mailto:` ohne Parameter öffnen | Code-Review |
| 14 | `avatar://` reicht Content-Type des Servers durch | info | A05 Fehlkonfiguration | nur png/jpeg/gif/webp, `nosniff`, CSP `default-src 'none'` | Code-Review |
| 15 | Mittelklick auf Link öffnet evtl. WebView-Fenster mit fremder Seite | info (unsicher) | A05 | globaler Klick-/Mittelklick-Abfang, nur http(s) an den Opener | Code-Review |
| 16 | Popout-Fenster mit Updater-/Neustart-Rechten, Opener ohne Scope | info | A01, A05 | eigene Capability `popout` (nur Links), Opener auf `http(s)://*` beschränkt, Notification-Recht entfernt (nur Rust nutzt es) | Build (Capability-Prüfung) |
| 17 | CSP ohne `object-src`, `base-uri`, `form-action` | info | A05 | ergänzt, dazu `frame-ancestors 'none'` | – |
| 18 | Irreführender Linktext `[https://bank](https://evil)` | info | – | Ziel als Tooltip (`title`) | Unit-Test |
| 19 | `TokenResponse` mit `Debug` (Token-Leck bei späterem Logging) | info | A09 Logging | `Debug` entfernt | Build |
| 20 | Gelöschte Cache-Daten bleiben in freien SQLite-Seiten | info | A02 | `PRAGMA secure_delete=ON` | – |
| 21 | `url` 2.5.8 (RUSTSEC-2026-0592), `cookie` 0.18.2 (RUSTSEC-2026-0594) | niedrig | A06 Komponenten | `url` 2.5.9, `cookie` 0.18.3; `cargo audit` und `npm audit` in CI | `cargo audit`: 0 Schwachstellen |

## Ohne Befund geprüft

- **Injection:** SQL durchgehend parametrisiert. Nur zwei `{@html}`-Stellen, beide über `formatText`. Kein
  `withGlobalTauri`, keine Remote-Domains in den Capabilities. `script-src` fällt auf `'self'` zurück.
- **`avatar://`:** Konto, Nutzer und Emoji-Name per Whitelist, kein Path-Traversal, kein SSRF.
- **Weiterleitungen:** reqwest entfernt `Authorization` bei Host-, Port- oder Schemawechsel.
- **Geheimnisse:** Tokens nur im Schlüsselbund. Konfiguration und Cache mit 0600/0700. Keine Tokens in Logs
  oder Fehlertexten.
- **OAuth:** PKCE S256, `state`-Prüfung, Loopback nur auf 127.0.0.1.
- **Native:** Links nur `http(s)`/`mailto`, Raw-HTML in Markdown wird verworfen (Pentest-Screenshot).

## Pentest-Lauf (2026-11-23)

Gegen frischen Testserver (`scripts/mm-test-server.sh`):

1. Integrationstest `mattermost::it` mit neuen Sicherheitsprüfungen: grün (Datei-ID mit `../` abgelehnt,
   Token nach Logout ungültig, Kanal-Header per WebSocket).
2. `scripts/pentest-mm.sh`: 14 Angriffsnachrichten, Markup-DMs, bösartiger Kanal-Header, Dateien mit RLO- und
   `../`-Namen. `formatText` gegen die echten Server-Daten: 63 Nachrichten, 0 Befunde.
3. chatty-native per `CHATTY_SHOT` auf Kanal Dev: kein Raw-HTML, `javascript:`-Link ohne Wirkung, Header
   harmlos, Dateinamen nach Fix `rechnungfdp.exe` und `bashrc`.
4. Fuzzing `formatText`: 400 000 Zufallseingaben im Review, 50 000 dauerhaft in CI. Der Fuzzer findet den
   Fehler aus 0.2.2 im alten Code.

**Nicht getestet:** Tauri-Oberfläche im echten Fenster (Klick, Mittelklick, Popout-Rechte), Verhalten der
Benachrichtigungsdienste von GNOME und KDE, Windows und macOS, der Release-Workflow mit getrenntem Signieren
(erst beim Tag 0.2.3), Mattermost bei übergroßen Profilbildern.

## Nachtrag Bildvorschau (2026-11-24)

Angehängte Bilder laden jetzt ohne Klick (wie Profilbilder). Absicherung: nur Anhänge mit MIME-Typ png/jpeg/
gif/webp und API-ID; höchstens 10 MiB (`MAX_PREVIEW`); Mattermost liefert die serverseitig verkleinerte Fassung
(`/files/{id}/preview`), ID nur alphanumerisch; Google ID per `path_safe_id`. Tauri: Auslieferung über `avatar://`
mit derselben Typ-Whitelist, `nosniff` und CSP; Vorschauen werden im Kern nicht gemerkt (kein unbegrenzter
Cache). Native: Abmessungen vor dem Dekodieren geprüft (max. 4096×4096). Integrationstest: Vorschau geladen,
ID mit `../` abgelehnt. Ein Absender erfährt wie bei jedem Chat-Client nichts über das Laden: Bilder kommen vom
eigenen Server, nicht von fremden Adressen.

## Nachtrag Einfügen und Kopieren (2026-11-25)

Eingefügte Bilder landen als Datei im Datenverzeichnis (`paste/`, 0700, Datei 0600) und gehen den Weg gewählter
Anhänge (ADR 06). Typ nach den ersten Bytes (png/jpeg/gif/webp, kein SVG), höchstens 32 MiB, Name vom Programm
(`bild-<Zeit>.<endung>`), kein Pfad aus der Oberfläche. Nach dem Senden gelöscht, Reste beim Start nach 24 h.
Tauri: Rohdaten per IPC-Body an `paste_image`, das Frontend erhält nur ID und Namen; Vorschau über
`avatar://localhost/picked/<id>` nur für vorgemerkte IDs und Bildendungen, bis 10 MiB, mit derselben
Typ-Whitelist. Native liest die Zwischenablage nur bei Strg+V im Eingabefeld. Die Textvorschau nutzt
`formatText` (Tauri, Fuzz-geprüft) bzw. den Markdown-Viewer (native, ohne Raw-HTML). Kopieren schreibt nur
Nachrichtentext in die Zwischenablage. Neue Abhängigkeit `arboard` 3.7.0 (native); `cargo audit` ohne neue Funde.

## Offen (bewusst nicht umgesetzt)

- **Ereigniskanal unbegrenzt:** Ein Server kann Ereignisse fluten. WebSocket-Limit gesetzt, begrenzter Kanal
  wäre ein Umbau.
- **Fehlertexte des Servers erscheinen wörtlich:** Phishing-Text durch einen bösartigen Server möglich, der
  aber ohnehin den Inhalt bestimmt.
- **Warnungen von `cargo audit`:** 12 transitive Crates sind unmaintained oder unsound (GTK/glib, zbus,
  paste …). Kommen über Tauri und iced, Update erst mit deren neuen Versionen.
- **Dateiinhalt vom Absender:** Gespeicherte Anhänge sind nicht geprüft (wie in jedem Chat-Client). Die
  Endung zeigt jetzt den echten Typ.
