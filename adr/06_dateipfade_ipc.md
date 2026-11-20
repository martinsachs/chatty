# ADR 06: Dateipfade nur aus nativen Dialogen

Status: vorgeschlagen (2026-11-20)

## Kontext

`send_message` nimmt Dateipfade vom Frontend entgegen und lädt sie hoch, `download_attachment`
schreibt an einen vom Frontend genannten Pfad. Das Frontend rendert fremde Inhalte (Nachrichten).
Gelänge dort Skriptausführung, könnte es beliebige Dateien des Nutzers hochladen oder überschreiben.
Heute verhindern das Escaping und die CSP; diese Entscheidung ist eine zweite Linie.

## Entscheidung

- Dialoge öffnet das Backend (blockierende API von `tauri-plugin-dialog` in `spawn_blocking`).
- Download: `save_attachment(account, attachment) -> bool` öffnet den Speichern-Dialog und schreibt
  selbst; `false` = abgebrochen. Bei Fehler bleibt im Frontend der Rückfall „im Browser öffnen“.
  `download_attachment` entfällt.
- Upload: `pick_files() -> Vec<PickedFile { id: u64, name: String }>` öffnet den Öffnen-Dialog und merkt
  die Pfade in `AppState.picked: Mutex<HashMap<u64, PathBuf>>`. `send_message(…, files: Vec<u64>)` liest
  sie; nach erfolgreichem Senden werden die IDs entfernt, bei Fehler bleiben sie gültig. Unbekannte ID →
  „Datei nicht mehr verfügbar, bitte neu wählen“.
- Das Frontend ruft `@tauri-apps/plugin-dialog` nicht mehr auf; `dialog:default` fällt aus
  `capabilities/default.json`.
- Kein Datei-Drag&Drop (gibt es heute nicht; YAGNI).

## Alternativen

- Frontend schickt Dateiinhalte statt Pfade: große Dateien durch IPC, mehr RAM. Verworfen.
- Pfad-Prüfung per Allowlist von Verzeichnissen: lückenhaft (Home-Verzeichnis). Verworfen.
- Pfad-Freigabe über den fs-Scope von Tauri: chatty nutzt das fs-Plugin nicht; eigener kleiner
  Mechanismus ist einfacher als ein weiteres Plugin.

## Folgen

- Gewählte, aber nie gesendete Dateien bleiben bis zum Programmende in `picked` (wenige Pfade).
- Das Frontend sieht nur Dateinamen.
