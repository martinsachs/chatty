# ADR 03: Ein Krypto-Provider (ring) für rustls

Status: vorgeschlagen (2026-11-20)

## Kontext

aws-lc-rs kommt allein über chattys reqwest-Feature `rustls` in den Baum (`cargo tree -e features -i
aws-lc-rs`); `tauri-plugin-updater` nutzt reqwest mit `rustls-no-provider`. ring ist über
`rustls-webpki` (Zertifikatsprüfung) ohnehin vorhanden. Beide Provider landen im Binary. aws-lc-rs braucht zum Bauen einen C-Compiler und cmake, unter Windows zusätzlich
nasm, und verlängert CI-Builds. Weil zwei Provider vorhanden sind, wählt rustls keinen Standard;
`tokio_tungstenite::connect_async` bricht deshalb ab (siehe [ADR 01](01_mattermost_websocket.md)).

## Entscheidung

reqwest mit `rustls-no-provider`, dazu `rustls = { version = "0.23", default-features = false,
features = ["ring", "std", "tls12"] }`. `backend::init_tls()` installiert
`rustls::crypto::ring::default_provider()` einmalig (`std::sync::Once`) als Standard; aufgerufen in
`lib.rs::run` und in beiden `http_client()`; `GoogleBackend::for_test` baut seinen Client über `http_client()`. tokio-tungstenite mit `default-features = false, features = ["handshake"]`.

## Alternativen

- Nur aws-lc-rs: der Updater müsste umgestellt werden (nicht als Feature angeboten). Verworfen.
- native-tls (OpenSSL/Schannel/Security.framework): zusätzliche Systemabhängigkeit unter Linux.
  Verworfen.

## Folgen

- Kleineres Binary und kürzere Builds (Größe vor/nach messen).
- CI prüft `! cargo tree --target all -i aws-lc-rs >/dev/null 2>&1`.
