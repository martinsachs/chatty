# ADR 04: Speicherprofil der WebView

Status: abgelehnt (2026-11-20): Messung ohne Nutzen

## Kontext

Unter Linux belegt der WebKitWebProcess 135 von 241 MB PSS. Das Ziel liegt bei < 150 MB gesamt.
WebKitGTK läuft mit Standard-Cachemodell (`WebBrowser`), das Seiten- und Ressourcen-Caches für
Browser-Navigation vorhält. chatty hat eine einzige Seite ohne Navigation.

## Entscheidung

Unter Linux im `setup` per `with_webview` das Cachemodell `DocumentViewer` am `WebContext` setzen.
Ob es nach dem Start des Hauptfensters noch greift, zeigen `get_cache_model()` im Log und die Messung;
haben abgelöste Fenster einen eigenen Context, auch in `open_space_window`. Neue Abhängigkeit `webkit2gtk = "2.0"` nur unter `cfg(target_os = "linux")`, dieselbe Version
wie über wry im Lockfile (2.0.2). Gemessen wird S1 allein gegen den Build des Vorgänger-Commits. Übernommen wird es nur, wenn die Messung nach `konzept/optimierung.md` (Median aus drei
Läufen) mindestens 10 MB PSS spart und die Oberfläche unverändert funktioniert.
Hardware-Beschleunigung bleibt an (Abschalten kostet CPU beim Scrollen; nur bei Bedarf messen).

## Alternativen

- Echte Virtualisierung der Oberfläche oder anderes UI-Toolkit: großer Umbau, verworfen.
- Ziel für Linux anheben: Rückfall, falls die Messung zu wenig bringt.

## Folgen

- Windows/macOS unverändert (anderes WebView, keine vergleichbare Einstellung nötig).
- Weniger Cache kann wiederholtes Laden von Avataren auslösen; die Antwort von `avatar://` setzt
  `Cache-Control`, das Backend hält die Bytes ohnehin im Speicher.

## Messung (2026-11-20)

`scripts/ram.sh`, Release-Build, 1 Mattermost-Konto über den Test-Proxy, 60 s, je drei Läufe (Median):

| Build | Summe PSS | chatty | WebKitWebProcess | WebKitNetworkProcess |
|---|---|---|---|---|
| ohne S1 (`eda38da`) | 237 MB | 86 MB | 134 MB | 17 MB |
| mit `DocumentViewer` | 237 MB | 85 MB | 134 MB | 17 MB |

Das Log bestätigt, dass das Modell gesetzt ist. Im Leerlauf sind die Caches ohnehin leer; die WebView-Grundlast
hängt nicht am Cachemodell. Die Änderung wurde nicht übernommen.
